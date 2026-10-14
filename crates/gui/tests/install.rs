//! Installing an app from an OCI registry: a registry on a local port that
//! asks for a token and keeps its blobs elsewhere, as ghcr does.

use artifact::app::Installed;
use cydonia_gui::model::apps::{Http, Program, oci::Reference};
use guest::{CONTRACT, manifest::SECTION};
use sha2::{Digest as _, Sha256};
use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    net::TcpListener,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// A wasm module holding only a manifest section saying `json`.
fn module(json: &str) -> Vec<u8> {
    let mut section = vec![SECTION.len() as u8];
    section.extend_from_slice(SECTION.as_bytes());
    section.extend_from_slice(json.as_bytes());
    let mut wasm = b"\0asm\x01\0\0\0\0".to_vec();
    let mut size = section.len();
    loop {
        let byte = (size & 0x7f) as u8;
        size >>= 7;
        if size == 0 {
            wasm.push(byte);
            break;
        }
        wasm.push(byte | 0x80);
    }
    wasm.extend(section);
    wasm
}

fn manifest(contract: u32) -> String {
    format!(
        r#"{{"id":"yc","name":"Y Combinator","version":"0.1.0","contract":{contract},"calls":["tables"],"hosts":["yc-oss.github.io"]}}"#
    )
}

/// A registry serving `blob` as the one layer of `acme/yc:0.1.0`, named by
/// `digest`. Answers with its address and the requests it saw.
fn registry(blob: Vec<u8>, digest: String) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (log, here) = (seen.clone(), address.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split(' ').nth(1).unwrap_or_default().to_owned();
            let mut authorized = false;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                authorized |= header
                    .to_lowercase()
                    .starts_with("authorization: bearer token-1");
            }
            log.lock().unwrap().push(format!("{path} {authorized}"));
            let layer = format!(
                r#"{{"schemaVersion":2,"layers":[{{"mediaType":"application/wasm","digest":"{digest}","size":{}}}]}}"#,
                blob.len()
            );
            let (status, headers, body): (&str, String, Vec<u8>) = match path.as_str() {
                p if p.starts_with("/token?") => {
                    ("200 OK", String::new(), br#"{"token":"token-1"}"#.to_vec())
                }
                "/v2/acme/yc/manifests/0.1.0" if !authorized => (
                    "401 Unauthorized",
                    format!(
                        "WWW-Authenticate: Bearer realm=\"http://{here}/token\",service=\"registry\",scope=\"repository:acme/yc:pull\"\r\n"
                    ),
                    Vec::new(),
                ),
                "/v2/acme/yc/manifests/0.1.0" => ("200 OK", String::new(), layer.into_bytes()),
                p if p.starts_with("/v2/acme/yc/blobs/") && authorized => (
                    "307 Temporary Redirect",
                    format!("Location: http://{here}/storage/yc\r\n"),
                    Vec::new(),
                ),
                "/storage/yc" => ("200 OK", String::new(), blob.clone()),
                _ => ("404 Not Found", String::new(), Vec::new()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
            let _ = stream.flush();
            let _ = reader.read(&mut [0; 1]);
        }
    });
    (address, seen)
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cydonia-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        // The data directory the installed `.wasm` is kept in.
        // SAFETY: nextest runs each test in a process of its own.
        unsafe { std::env::set_var("XDG_DATA_HOME", path.join("data")) };
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_reference_names_its_registry_repository_and_tag() {
    let reference = Reference::parse("ghcr.io/crabtalk/cydonia/yc:0.1.0").unwrap();
    assert_eq!(
        (
            reference.registry.as_str(),
            reference.repository.as_str(),
            reference.tag.as_str()
        ),
        ("ghcr.io", "crabtalk/cydonia/yc", "0.1.0")
    );
    assert_eq!(reference.repository_path(), "ghcr.io/crabtalk/cydonia/yc");
    assert_eq!(Reference::parse("localhost:5000/yc").unwrap().tag, "latest");
    assert!(Reference::parse("yc").is_err());
    assert!(Reference::parse("crabtalk/yc:1").is_err());
}

#[test]
fn an_app_is_pulled_through_a_token_and_a_redirect_and_applied() {
    let scratch = Scratch::new("pull");
    let wasm = module(&manifest(CONTRACT));
    let (address, seen) = registry(wasm.clone(), digest(&wasm));
    let reference = Reference::parse(&format!("{address}/acme/yc:0.1.0")).unwrap();
    let installed = reference.install::<Http>(&scratch.0).unwrap();
    assert_eq!(installed.id, "yc");
    assert_eq!(installed.digest, digest(&wasm));
    assert!(!installed.allowed());
    assert_eq!(std::fs::read(installed.wasm().unwrap()).unwrap(), wasm);
    assert_eq!(
        Installed::list(&scratch.0),
        std::slice::from_ref(&installed)
    );
    let (loaded, manifest) = Program::load(&installed).unwrap();
    assert_eq!((loaded, manifest.name.as_str()), (wasm, "Y Combinator"));

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0], "/v2/acme/yc/manifests/0.1.0 false");
    assert!(
        seen[1].starts_with("/token?service=registry&scope=repository"),
        "{seen:?}"
    );
    assert_eq!(seen[2], "/v2/acme/yc/manifests/0.1.0 true");
    // The storage a blob is redirected to is not handed the registry's token.
    assert_eq!(seen.last().unwrap(), "/storage/yc false");
}

#[test]
fn a_layer_that_is_not_its_digest_is_refused() {
    let scratch = Scratch::new("digest");
    let wasm = module(&manifest(CONTRACT));
    let (address, _) = registry(wasm, digest(b"something else"));
    let reference = Reference::parse(&format!("{address}/acme/yc:0.1.0")).unwrap();
    let error = format!("{:#}", reference.install::<Http>(&scratch.0).unwrap_err());
    assert!(error.contains("not the sha256:"), "{error}");
    assert!(Installed::list(&scratch.0).is_empty());
}

#[test]
fn an_app_built_for_a_newer_contract_is_refused_at_install() {
    let scratch = Scratch::new("contract");
    let wasm = module(&manifest(CONTRACT + 1));
    let (address, _) = registry(wasm.clone(), digest(&wasm));
    let reference = Reference::parse(&format!("{address}/acme/yc:0.1.0")).unwrap();
    let error = format!("{:#}", reference.install::<Http>(&scratch.0).unwrap_err());
    assert!(
        error.contains(&format!("needs view contract {}", CONTRACT + 1)),
        "{error}"
    );
    assert!(Installed::list(&scratch.0).is_empty());
}

#[test]
fn letting_an_app_run_is_kept_for_its_digest() {
    let scratch = Scratch::new("allow");
    let wasm = module(&manifest(CONTRACT));
    let (address, _) = registry(wasm.clone(), digest(&wasm));
    let reference = Reference::parse(&format!("{address}/acme/yc:0.1.0")).unwrap();
    let mut installed = reference.install::<Http>(&scratch.0).unwrap();
    installed.allowed = Some(installed.digest.clone());
    installed.save(&scratch.0).unwrap();
    assert!(Installed::list(&scratch.0)[0].allowed());

    // Another reference cannot take the same id in the same project.
    let mut other = installed.clone();
    other.reference = "ghcr.io/someone/yc".into();
    assert!(other.save(&scratch.0).is_err());
}
