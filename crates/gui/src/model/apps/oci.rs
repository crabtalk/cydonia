//! Pulling an app from an OCI registry, as the distribution API serves one: a
//! manifest naming one layer, the app's `.wasm`.

use super::{Fetcher, Manifest, Request, Response};
use anyhow::{Context as _, Result, bail, ensure};
use artifact::app::Installed;
use sha2::{Digest as _, Sha256};
use std::path::Path;

/// The media types an image manifest is asked for in.
const MANIFESTS: &str = "application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json";
/// The media type of the layer holding the `.wasm`.
pub const WASM: &str = "application/wasm";
/// How many redirects a blob is followed through.
const HOPS: usize = 5;

/// What an app is pulled by: `ghcr.io/crabtalk/cydonia/yc:0.1.0`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// `ghcr.io`, or `localhost:5000`.
    pub registry: String,
    /// `crabtalk/cydonia/yc`.
    pub repository: String,
    /// `latest` where none is written.
    pub tag: String,
}

/// An app pulled and checked: its `.wasm`, that `.wasm`'s digest, and its
/// manifest.
pub struct Pulled {
    pub digest: String,
    pub wasm: Vec<u8>,
    pub manifest: Manifest,
}

impl Reference {
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        let text = text
            .strip_prefix("oci://")
            .or_else(|| text.strip_prefix("https://"))
            .unwrap_or(text);
        let (registry, rest) = text
            .split_once('/')
            .with_context(|| format!("{text} names no registry: write it as ghcr.io/owner/app"))?;
        let (repository, tag) = match rest.rsplit_once(':') {
            Some((repository, tag)) if !tag.contains('/') => (repository, tag),
            _ => (rest, "latest"),
        };
        ensure!(
            registry.contains(['.', ':']) || registry == "localhost",
            "{registry} is not a registry"
        );
        ensure!(
            !repository.is_empty() && !tag.is_empty(),
            "{text} names no app"
        );
        Ok(Self {
            registry: registry.to_owned(),
            repository: repository.to_owned(),
            tag: tag.to_owned(),
        })
    }

    /// The repository, without the tag: what a project lists the app by.
    pub fn repository_path(&self) -> String {
        format!("{}/{}", self.registry, self.repository)
    }

    /// `http` for a registry on this machine, `https` everywhere else.
    fn base(&self) -> String {
        let host = self.registry.split(':').next().unwrap_or_default();
        let scheme = match host {
            "localhost" | "127.0.0.1" => "http",
            _ => "https",
        };
        format!("{scheme}://{}/v2/{}", self.registry, self.repository)
    }

    /// Pull the app and check it: the blob against its digest, the module for
    /// a manifest, the manifest for a contract this Cydonia has.
    pub fn pull<F: Fetcher>(&self) -> Result<Pulled> {
        let mut auth = Auth::default();
        let url = format!("{}/manifests/{}", self.base(), self.tag);
        let index = auth.get::<F>(&url, &[("accept", MANIFESTS)])?;
        let index: serde_json::Value =
            serde_json::from_slice(&index.body).context("The registry's manifest is not JSON")?;
        let layers = index["layers"]
            .as_array()
            .context("The image has no layers")?;
        let layer = match layers.as_slice() {
            [one] => one,
            many => many
                .iter()
                .find(|layer| layer["mediaType"] == WASM)
                .context("The image has no wasm layer")?,
        };
        let digest = layer["digest"]
            .as_str()
            .context("The layer names no digest")?
            .to_owned();
        let blob = auth.get::<F>(&format!("{}/blobs/{digest}", self.base()), &[])?;
        let found = format!("sha256:{:x}", Sha256::digest(&blob.body));
        ensure!(
            found == digest,
            "The layer is {found}, not the {digest} it was named by"
        );
        let manifest = Manifest::read(&blob.body)
            .map_err(anyhow::Error::msg)?
            .context("The layer is not a Cydonia app: it has no manifest")?;
        manifest.supported().map_err(anyhow::Error::msg)?;
        Ok(Pulled {
            digest,
            wasm: blob.body,
            manifest,
        })
    }

    /// Pull the app, keep its `.wasm` and apply it to `project`.
    pub fn install<F: Fetcher>(&self, project: &Path) -> Result<Installed> {
        let pulled = self.pull::<F>()?;
        let path = Installed::wasm_at(&pulled.manifest.id, &pulled.digest)?;
        if !path.is_file() {
            artifact::fs::save(&path, &pulled.wasm)?;
        }
        let installed = Installed {
            id: pulled.manifest.id,
            reference: self.repository_path(),
            tag: self.tag.clone(),
            digest: pulled.digest,
            allowed: None,
        };
        installed.save(project)?;
        Ok(installed)
    }
}

/// A registry's bearer token, once it has asked for one.
#[derive(Default)]
struct Auth {
    token: Option<String>,
}

impl Auth {
    /// `GET url`, answering a registry's challenge for a token once, and
    /// following redirects. Anything but a 2xx in the end is an error.
    fn get<F: Fetcher>(&mut self, url: &str, headers: &[(&str, &str)]) -> Result<Response> {
        let mut url = url.to_owned();
        let mut authorized = true;
        for _ in 0..HOPS {
            let response = F::fetch(&self.request(&url, headers, authorized))?;
            match response.status {
                200..=299 => return Ok(response),
                401 if self.token.is_none() => {
                    let challenge = response
                        .header("www-authenticate")
                        .context("The registry refused without a challenge")?;
                    self.token = Some(token::<F>(challenge)?);
                }
                301 | 302 | 303 | 307 | 308 => {
                    url = response
                        .header("location")
                        .context("A redirect with nowhere to go")?
                        .to_owned();
                    // A blob's storage is another host, which is not handed
                    // the registry's token.
                    authorized = false;
                }
                status => bail!("{url} answered HTTP {status}"),
            }
        }
        bail!("{url} redirected too many times")
    }

    fn request(&self, url: &str, headers: &[(&str, &str)], authorized: bool) -> Request {
        let mut held: Vec<(String, String)> = headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        if let (true, Some(token)) = (authorized, &self.token) {
            held.push(("authorization".into(), format!("Bearer {token}")));
        }
        Request {
            method: "GET".into(),
            url: url.to_owned(),
            headers: held,
            body: None,
        }
    }
}

/// The token a `Bearer realm="…",service="…",scope="…"` challenge hands out
/// to anyone.
fn token<F: Fetcher>(challenge: &str) -> Result<String> {
    let mut rest = challenge
        .strip_prefix("Bearer ")
        .context("The registry asks for a login Cydonia does not have")?
        .trim();
    let mut realm = None;
    let mut query = Vec::new();
    while let Some((name, after)) = rest.split_once('=') {
        let (value, after) = match after.strip_prefix('"') {
            Some(quoted) => quoted.split_once('"').unwrap_or((quoted, "")),
            None => after.split_once(',').unwrap_or((after, "")),
        };
        match name.trim() {
            "realm" => realm = Some(value.to_owned()),
            name @ ("service" | "scope") => query.push((name.to_owned(), value.to_owned())),
            _ => {}
        }
        rest = after.trim_start_matches([',', ' ']);
    }
    let realm = realm.context("The registry's challenge names no realm")?;
    let url = url::Url::parse_with_params(&realm, &query)
        .with_context(|| format!("The registry's realm {realm} is not a URL"))?;
    let response = F::fetch(&Request {
        method: "GET".into(),
        url: url.to_string(),
        headers: Vec::new(),
        body: None,
    })?;
    ensure!(
        response.status == 200,
        "The registry's token service answered HTTP {}",
        response.status
    );
    let body: serde_json::Value = serde_json::from_slice(&response.body)?;
    body["token"]
        .as_str()
        .or_else(|| body["access_token"].as_str())
        .map(str::to_owned)
        .context("The registry's token service gave no token")
}
