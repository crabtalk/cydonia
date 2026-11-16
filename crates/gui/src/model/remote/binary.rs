//! Version-matched release binaries. Resolving a host never requires a checkout
//! or a Rust toolchain on an installed desktop app.
use anyhow::{Context as _, ensure};
use cupola::client::platform::Platform;
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    io::Write as _,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
};

const RELEASES: &str = "https://github.com/crabtalk/cydonia/releases/download";
const LIMIT: u64 = 256 * 1024 * 1024;

pub(super) fn resolve(platform: Platform) -> anyhow::Result<PathBuf> {
    let cache = super::super::settings::data_dir()?.join("servers");
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .new_agent();
    resolve_with(&cache, server::VERSION, platform, |name| {
        let url = format!("{RELEASES}/v{}/{name}", server::VERSION);
        agent
            .get(&url)
            .call()
            .with_context(|| format!("download server from {url}"))?
            .body_mut()
            .with_config()
            .limit(LIMIT)
            .read_to_vec()
            .map_err(Into::into)
    })
}

fn asset(version: &str, platform: Platform) -> String {
    format!("cydonia-server-{version}-{platform}")
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn checksum(bytes: &[u8]) -> anyhow::Result<String> {
    let text = std::str::from_utf8(bytes)?;
    let hash = text.split_whitespace().next().unwrap_or_default();
    ensure!(
        hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid server checksum"
    );
    Ok(hash.to_ascii_lowercase())
}

fn resolve_with(
    cache: &Path,
    version: &str,
    platform: Platform,
    mut fetch: impl FnMut(&str) -> anyhow::Result<Vec<u8>>,
) -> anyhow::Result<PathBuf> {
    ensure!(
        !version.is_empty()
            && version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c)),
        "invalid server version"
    );
    fs::create_dir_all(cache)?;
    let name = asset(version, platform);
    let path = cache.join(&name);
    let sums = cache.join(format!("{name}.sha256"));
    if let (Ok(bytes), Ok(sum)) = (fs::read(&path), fs::read(&sums))
        && checksum(&sum).is_ok_and(|hash| digest(&bytes) == hash)
    {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        return Ok(path);
    }
    let sum = fetch(&format!("{name}.sha256"))?;
    let expected = checksum(&sum)?;
    let bytes = fetch(&name)?;
    ensure!(
        !bytes.is_empty() && digest(&bytes) == expected,
        "server checksum mismatch for {name}"
    );
    // Each replacement is atomic. A crash between the binary and checksum
    // leaves a cache miss, never an unchecked binary accepted as a hit.
    put(cache, &path, &bytes, 0o755)?;
    put(cache, &sums, expected.as_bytes(), 0o644)?;
    Ok(path)
}

fn put(dir: &Path, target: &Path, bytes: &[u8], mode: u32) -> anyhow::Result<()> {
    let mut staged = tempfile::NamedTempFile::new_in(dir)?;
    staged.write_all(bytes)?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    staged.as_file().sync_all()?;
    staged.persist(target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cupola::client::platform::{Arch, Os};

    const PLATFORM: Platform = Platform {
        os: Os::Linux,
        arch: Arch::Aarch64,
    };

    #[test]
    fn release_names_cover_every_supported_platform() {
        for os in [Os::Linux, Os::MacOs] {
            for arch in [Arch::X86_64, Arch::Aarch64] {
                assert_eq!(
                    asset("1.2.3", Platform { os, arch }),
                    format!("cydonia-server-1.2.3-{}-{}", os.as_str(), arch.as_str())
                );
            }
        }
    }

    #[test]
    fn verified_cache_works_offline_and_corruption_is_downloaded_again() {
        let scratch = tempfile::tempdir().unwrap();
        let bytes = b"a release binary";
        let fetch = |name: &str| {
            Ok(if name.ends_with(".sha256") {
                digest(bytes).into_bytes()
            } else {
                bytes.to_vec()
            })
        };
        let path = resolve_with(scratch.path(), "1.2.3", PLATFORM, fetch).unwrap();
        assert_eq!(
            resolve_with(scratch.path(), "1.2.3", PLATFORM, |_| panic!(
                "must use verified cache"
            ))
            .unwrap(),
            path
        );
        fs::write(&path, b"corrupt").unwrap();
        resolve_with(scratch.path(), "1.2.3", PLATFORM, fetch).unwrap();
        assert_eq!(fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn bad_downloads_do_not_replace_a_cached_binary_and_versions_are_separate() {
        let scratch = tempfile::tempdir().unwrap();
        let bytes = b"good";
        let path = resolve_with(scratch.path(), "1.2.3", PLATFORM, |name| {
            Ok(if name.ends_with(".sha256") {
                digest(bytes).into_bytes()
            } else {
                bytes.to_vec()
            })
        })
        .unwrap();
        assert!(
            resolve_with(scratch.path(), "1.2.4", PLATFORM, |name| Ok(
                if name.ends_with(".sha256") {
                    digest(bytes).into_bytes()
                } else {
                    b"wrong".to_vec()
                }
            ))
            .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert!(!scratch.path().join(asset("1.2.4", PLATFORM)).exists());
        assert!(checksum(b"not a checksum").is_err());
        assert!(resolve_with(scratch.path(), "../bad", PLATFORM, |_| unreachable!()).is_err());
    }
}
