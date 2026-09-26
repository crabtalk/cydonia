//! Pinned grammar downloads, verified before caching or registration.

use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use syntax::{
    lang::{Grammar, Lang},
    registry::{self, Entry},
};

const VERSION: &str = "2.0.1";
const BASE: &str = "https://cdn.jsdelivr.net/npm/tree-sitter-wasm@2.0.1/out";
const MAX_CACHE: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
struct Asset {
    path: String,
    size: u64,
    sha256: String,
}

use super::Status;

#[derive(Deserialize)]
struct Spec {
    name: String,
    symbol: String,
    aliases: Vec<String>,
    files: Vec<String>,
    wasm: Asset,
    queries: Vec<Asset>,
}

#[derive(Serialize, Deserialize)]
struct Bundle {
    wasm: String,
    queries: Vec<String>,
}

fn catalog() -> &'static [Spec] {
    static CATALOG: OnceLock<Vec<Spec>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("catalog.json")).expect("grammar catalog")
    })
}

fn spec(name: &str) -> Option<&'static Spec> {
    catalog().iter().find(|spec| spec.name == name)
}

/// Every language the catalogue can install, cached or not — what the fence
/// picker offers. [`super::paintable`] is the cached subset.
pub fn names() -> Vec<&'static str> {
    catalog().iter().map(|spec| spec.name.as_str()).collect()
}

/// The catalogue name a fence's label means, following aliases: a fence tagged
/// `rs` or `sh` names a grammar under another name.
pub fn resolve(label: &str) -> Option<&'static str> {
    let label = label.trim().to_ascii_lowercase();
    catalog()
        .iter()
        .find(|spec| spec.name == label || spec.aliases.contains(&label))
        .map(|spec| spec.name.as_str())
}

/// Whether any download is still running. The poll that repaints a fence when
/// its grammar lands stops on this.
pub fn working() -> bool {
    states()
        .lock()
        .unwrap()
        .values()
        .any(|status| status.active())
}

/// One poll loop at a time, however many fences ask for one.
static WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) fn begin_watch() -> bool {
    !WATCHING.swap(true, std::sync::atomic::Ordering::SeqCst)
}

pub(super) fn end_watch() {
    WATCHING.store(false, std::sync::atomic::Ordering::SeqCst);
}

fn states() -> &'static Mutex<HashMap<String, Status>> {
    static STATES: OnceLock<Mutex<HashMap<String, Status>>> = OnceLock::new();
    STATES.get_or_init(Mutex::default)
}

fn set_status(name: &str, status: Status) {
    states().lock().unwrap().insert(name.into(), status);
}

pub fn status(name: &str) -> Status {
    super::installed();
    states()
        .lock()
        .unwrap()
        .get(name)
        .cloned()
        .unwrap_or(Status::Missing)
}

pub fn available(name: &str) -> bool {
    spec(name).is_some()
}

fn directory() -> Result<PathBuf> {
    Ok(super::super::settings::data_dir()?
        .join("grammars")
        .join(VERSION))
}

pub(super) fn initialize() {
    for spec in catalog() {
        // Override the named row too, so aliases and extensions agree before installation.
        register(spec, None);
        let result = directory().and_then(|root| read_cached(spec, &root));
        match result {
            Ok(Some((wasm, query))) => {
                register(spec, Some((wasm, query)));
                set_status(&spec.name, Status::Ready);
            }
            Ok(None) => {}
            Err(error) => set_status(&spec.name, Status::Failed(format!("{error:#}"))),
        }
    }
}

fn engine() -> Result<&'static syntax::wasmtime::Engine> {
    static ENGINE: OnceLock<Result<syntax::wasmtime::Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let engine = syntax::wasmtime::Engine::new(&syntax::wasmtime::Config::new())
                .map_err(|error| error.to_string())?;
            syntax::set_engine(engine.clone());
            Ok(engine)
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!(error.clone()))
}

pub(super) fn ensure_runtime() -> Result<()> {
    engine().map(|_| ())
}

fn register(spec: &'static Spec, bundle: Option<(Vec<u8>, String)>) {
    let aliases: &'static [&'static str] =
        Box::leak(spec.aliases.iter().map(String::as_str).collect());
    let files: &'static [&'static str] = Box::leak(spec.files.iter().map(String::as_str).collect());
    let lang = bundle.map(|(wasm, query)| {
        // The exported symbol can differ from the registry name (c_sharp / csharp).
        let lang = Lang::new(
            &spec.symbol,
            aliases,
            Grammar::Wasm(Arc::from(wasm)),
            Box::leak(query.into_boxed_str()),
        );
        &*Box::leak(Box::new(lang))
    });
    registry::register(Entry {
        name: &spec.name,
        aliases,
        files,
        lang,
    });
}

fn verify(asset: &Asset, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() as u64 == asset.size,
        "Incomplete grammar asset: {}",
        asset.path
    );
    ensure!(
        format!("{:x}", Sha256::digest(bytes)) == asset.sha256,
        "Grammar checksum mismatch: {}",
        asset.path
    );
    Ok(())
}

fn unpack(spec: &Spec, bundle: &Bundle) -> Result<(Vec<u8>, String)> {
    let wasm = STANDARD.decode(&bundle.wasm)?;
    verify(&spec.wasm, &wasm)?;
    ensure!(
        bundle.queries.len() == spec.queries.len(),
        "Incomplete highlight queries"
    );
    for (asset, query) in spec.queries.iter().zip(&bundle.queries) {
        verify(asset, query.as_bytes())?;
    }
    Ok((wasm, bundle.queries.join("\n")))
}

fn validate(spec: &Spec, wasm: &[u8], query: &str) -> Result<()> {
    let mut store = syntax::tree_sitter::WasmStore::new(engine()?)?;
    let language = store.load_language(&spec.symbol, wasm)?;
    syntax::tree_sitter::Query::new(&language, query).context("Invalid highlight query")?;
    let mut parser = syntax::tree_sitter::Parser::new();
    parser.set_wasm_store(store)?;
    parser.set_language(&language)?;
    ensure!(parser.parse("", None).is_some(), "Grammar failed to parse");
    Ok(())
}

fn read_cached(spec: &Spec, root: &Path) -> Result<Option<(Vec<u8>, String)>> {
    let path = root.join(format!("{}.json", spec.name));
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        file.metadata()?.len() <= MAX_CACHE,
        "Grammar cache is too large"
    );
    let bundle: Bundle = serde_json::from_reader(file.take(MAX_CACHE))?;
    unpack(spec, &bundle).map(Some)
}

fn save(spec: &Spec, root: &Path, bundle: &Bundle) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let target = root.join(format!("{}.json", spec.name));
    let temporary = root.join(format!(".{}-{}.part", spec.name, std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        serde_json::to_writer(&mut file, bundle)?;
        file.flush()?;
        file.sync_all()?;
        std::fs::rename(&temporary, target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

fn download(asset: &Asset, progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>> {
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(Duration::from_secs(180)))
        .build()
        .new_agent();
    let mut response = agent.get(format!("{BASE}/{}", asset.path)).call()?;
    let mut reader = response.body_mut().as_reader().take(asset.size + 1);
    let mut bytes = Vec::with_capacity(asset.size as usize);
    let mut buffer = [0; 32 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        progress(count as u64);
    }
    verify(asset, &bytes)?;
    Ok(bytes)
}

fn install(spec: &'static Spec, root: &Path) -> Result<()> {
    install_with(spec, root, download)
}

fn install_with(
    spec: &'static Spec,
    root: &Path,
    mut fetch: impl FnMut(&Asset, &mut dyn FnMut(u64)) -> Result<Vec<u8>>,
) -> Result<()> {
    let total = spec.wasm.size + spec.queries.iter().map(|asset| asset.size).sum::<u64>();
    let mut received = 0;
    let mut progress = |count| {
        received += count;
        set_status(&spec.name, Status::Downloading { received, total });
    };
    let wasm = fetch(&spec.wasm, &mut progress)?;
    verify(&spec.wasm, &wasm)?;
    let queries = spec
        .queries
        .iter()
        .map(|asset| {
            let bytes = fetch(asset, &mut progress)?;
            verify(asset, &bytes)?;
            String::from_utf8(bytes).map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()?;
    set_status(&spec.name, Status::Checking);
    let query = queries.join("\n");
    validate(spec, &wasm, &query)?;
    save(
        spec,
        root,
        &Bundle {
            wasm: STANDARD.encode(&wasm),
            queries,
        },
    )?;
    register(spec, Some((wasm, query)));
    Ok(())
}

fn begin_install(spec: &Spec) -> bool {
    let mut states = states().lock().unwrap();
    if states
        .get(&spec.name)
        .is_some_and(|status| status.active() || *status == Status::Ready)
    {
        return false;
    }
    states.insert(
        spec.name.clone(),
        Status::Downloading {
            received: 0,
            total: spec.wasm.size + spec.queries.iter().map(|asset| asset.size).sum::<u64>(),
        },
    );
    true
}

/// Called only by the user's Install/Retry action. Concurrent views share one download.
pub fn start_install(name: &str) {
    super::installed();
    let Some(spec) = spec(name) else {
        return;
    };
    if !begin_install(spec) {
        return;
    }
    let started = std::thread::Builder::new()
        .name(format!("grammar-{}", spec.name))
        .spawn(move || {
            let result = directory().and_then(|root| install(spec, &root));
            set_status(
                &spec.name,
                match result {
                    Ok(()) => Status::Ready,
                    Err(error) => Status::Failed(format!("{error:#}")),
                },
            );
        });
    if let Err(error) = started {
        set_status(name, Status::Failed(error.to_string()));
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/grammar.rs"]
mod tests;
