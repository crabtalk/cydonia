//! Putting an agent on disk and finding what it left there.

use crate::{
    registry::{Agent, Binary, Distribution},
    utils,
};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
};

/// A local installation: the command to spawn, its arguments, and anything the
/// agent needs in its environment to speak ACP at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub id: String,
    pub version: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl Agent {
    /// Install under `data_dir`, streaming installer output to `on_line`.
    /// Replaces any previous install of the same agent.
    pub fn install(&self, data_dir: &Path, on_line: impl FnMut(&str)) -> Result<Installed> {
        let dir = agent_dir(data_dir, &self.id)?;
        let (command, args, env) = match &self.distribution {
            Distribution::Npm { package, args } => (
                install_npm(&dir, package, on_line)?,
                args.clone(),
                BTreeMap::new(),
            ),
            Distribution::Binary(binary) => (
                install_binary(&dir, binary, on_line)?,
                binary.args.clone(),
                binary.env.clone(),
            ),
            Distribution::Unsupported { .. } => bail!("{} cannot be installed yet", self.name),
        };

        let record = Installed {
            id: self.id.clone(),
            version: self.version.clone(),
            command,
            args,
            env,
        };
        std::fs::write(
            record_file(data_dir, &self.id)?,
            serde_json::to_string_pretty(&record)?,
        )
        .context("recording the installation")?;
        Ok(record)
    }
}

impl Installed {
    /// The recorded installation for `id`, if the agent is installed and its
    /// command still exists.
    pub fn find(data_dir: &Path, id: &str) -> Option<Self> {
        let body = std::fs::read_to_string(record_file(data_dir, id).ok()?).ok()?;
        let record: Self = serde_json::from_str(&body).ok()?;
        Path::new(&record.command).exists().then_some(record)
    }

    /// Delete an installed agent. Succeeds whether or not it was there.
    pub fn remove(data_dir: &Path, id: &str) -> Result<()> {
        let dir = agent_dir(data_dir, id)?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        }
        Ok(())
    }
}

fn agent_dir(data_dir: &Path, id: &str) -> Result<PathBuf> {
    utils::contained(&data_dir.join("agents"), id)
}

fn record_file(data_dir: &Path, id: &str) -> Result<PathBuf> {
    Ok(agent_dir(data_dir, id)?.join("install.json"))
}

/// The package name in a spec like `@scope/name@1.2.3` — the version separator
/// is the last `@` that isn't the scope's leading one.
pub fn package_name(spec: &str) -> &str {
    match spec.rfind('@') {
        Some(ix) if ix > 0 => &spec[..ix],
        _ => spec,
    }
}

/// Install one npm package into `dir` (replacing whatever was there), streaming
/// installer output to `on_line`. Returns the executable path.
pub(crate) fn install_npm(
    dir: &Path,
    package: &str,
    mut on_line: impl FnMut(&str),
) -> Result<String> {
    // Resolved rather than named: on Windows `npm` is `npm.cmd`, which
    // `Command::new("npm")` would not find. (cydonia: Windows support.)
    let Some(npm) = utils::which("npm") else {
        bail!("npm was not found on PATH — install Node.js to add agents");
    };
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    on_line(&format!("npm install {package}"));
    let mut child = utils::command(&npm)
        .arg("install")
        .arg("--prefix")
        .arg(dir)
        // A dependency's `postinstall` runs as this user, before the agent is
        // ever launched — the usual way a compromised package gets execution.
        .args([
            "--ignore-scripts",
            "--no-fund",
            "--no-audit",
            "--loglevel",
            "http",
        ])
        .arg(package)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running npm")?;

    // npm splits progress across both streams; merge them so the caller sees
    // output in the order it arrives.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for stream in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    for line in rx {
        let line = line.trim_end();
        if !line.is_empty() {
            on_line(line);
        }
    }

    let status = child.wait().context("waiting for npm")?;
    if !status.success() {
        bail!("npm install failed ({status})");
    }
    let command = binary_path(dir, package)?;
    on_line("installed");
    Ok(command)
}

/// Download a release archive into `dir`, unpack it there, and return the
/// executable named by [`Binary::cmd`].
///
/// The archive is fetched with an HTTP client rather than handed to the OS,
/// which is what keeps macOS from stamping `com.apple.quarantine` on it — the
/// ad-hoc signature every Rust and Go release carries would not survive
/// Gatekeeper's assessment, and does not have to face it.
pub(crate) fn install_binary(
    dir: &Path,
    binary: &Binary,
    mut on_line: impl FnMut(&str),
) -> Result<String> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    let archive = dir.join(archive_name(&binary.archive));
    on_line(&format!("downloading {}", binary.archive));
    let digest = download(&binary.archive, &archive, &mut on_line)?;

    match &binary.sha256 {
        Some(want) if !want.eq_ignore_ascii_case(&digest) => {
            bail!("checksum mismatch — expected {want}, got {digest}")
        }
        Some(_) => on_line("checksum ok"),
        // Roughly half the catalog publishes none. The registry is the trust
        // anchor either way: it is what handed us the URL.
        None => on_line("no checksum published — installing unverified"),
    }

    unpack(&archive, dir, &mut on_line)?;
    let _ = std::fs::remove_file(&archive);

    let command = utils::contained(dir, &binary.cmd)?;
    if !command.exists() {
        bail!("{} is missing after unpacking", command.display());
    }
    make_executable(&command)?;
    on_line("installed");
    Ok(command.display().to_string())
}

/// Stream the body to `into`, returning its hex sha256. Streamed rather than
/// buffered: these archives run to tens of megabytes.
fn download(url: &str, into: &Path, on_line: &mut impl FnMut(&str)) -> Result<String> {
    let mut resp = ureq::get(url)
        .call()
        .with_context(|| format!("fetching {url}"))?;
    let mut reader = resp.body_mut().as_reader();
    let mut file =
        std::fs::File::create(into).with_context(|| format!("creating {}", into.display()))?;

    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    let mut announced = 0u64;
    loop {
        let n = reader.read(&mut buf).context("reading the download")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).context("writing the download")?;
        total += n as u64;
        // Every few megabytes, so a long download is visibly moving without
        // flooding whoever is reading these lines.
        if total - announced >= 8 * 1024 * 1024 {
            announced = total;
            on_line(&format!("downloaded {} MB", total / (1024 * 1024)));
        }
    }
    file.flush().context("flushing the download")?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Unpack into `dir`. `tar` reads every format the registry publishes when it is
/// libarchive's (macOS, Windows 10+); GNU tar handles the compressed tarballs
/// and leaves zip to `unzip`.
fn unpack(archive: &Path, dir: &Path, on_line: &mut impl FnMut(&str)) -> Result<()> {
    let is_zip = archive
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if let Some(tar) = utils::which("tar") {
        on_line("unpacking");
        let out = utils::command(&tar)
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(dir)
            .output()
            .context("running tar")?;
        if out.status.success() {
            return Ok(());
        }
        if !is_zip {
            bail!(
                "tar failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
    }
    if is_zip && let Some(unzip) = utils::which("unzip") {
        on_line("unpacking with unzip");
        let out = utils::command(&unzip)
            .args(["-q", "-o"])
            .arg(archive)
            .arg("-d")
            .arg(dir)
            .output()
            .context("running unzip")?;
        if out.status.success() {
            return Ok(());
        }
        bail!(
            "unzip failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    bail!(
        "no tool to unpack {} — install {}",
        archive.display(),
        if is_zip { "tar or unzip" } else { "tar" }
    )
}

/// Archives usually carry the bit already; a zip need not.
fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        if perms.mode() & 0o111 == 0 {
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms)
                .with_context(|| format!("marking {} executable", path.display()))?;
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// The archive's own filename, which is what tells `unpack` the format. Falls
/// back to a neutral name for a URL that ends in a path segment we can't read.
fn archive_name(url: &str) -> String {
    url.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(|name| name.split(['?', '#']).next().unwrap_or(name).to_owned())
        .unwrap_or_else(|| "archive".to_owned())
}

/// The executable npm linked for `package`, read from the installed package's
/// own `bin` field.
fn binary_path(dir: &Path, package: &str) -> Result<String> {
    let name = package_name(package);
    let manifest = dir.join("node_modules").join(name).join("package.json");
    let body = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&body)?;

    let bin_name = match &manifest["bin"] {
        serde_json::Value::String(_) => name.rsplit('/').next().unwrap_or(name).to_owned(),
        serde_json::Value::Object(map) => map
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| anyhow!("{name} declares no executable"))?,
        _ => bail!("{name} declares no executable"),
    };

    let bin = dir.join("node_modules").join(".bin").join(&bin_name);
    // npm writes three launchers for each executable: a POSIX shell script
    // under the bare name, and `.cmd` and `.ps1` beside it for Windows. The
    // bare one is the command everywhere but Windows, where it is a script
    // nothing can run and the `.cmd` is what a terminal would pick.
    // (cydonia: Windows support.)
    let bin = if cfg!(windows) {
        let mut cmd = bin.clone().into_os_string();
        cmd.push(".cmd");
        let cmd = std::path::PathBuf::from(cmd);
        if cmd.exists() { cmd } else { bin }
    } else {
        bin
    };
    if !bin.exists() {
        bail!("{} is missing after install", bin.display());
    }
    Ok(bin.display().to_string())
}
