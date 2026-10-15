//! The manifest an app declares, in its `Cargo.toml` and as a custom section.

use cydonia_manifest::{CONTRACT, Declared, Manifest, SECTION};

const CARGO: &str = r#"
[package]
name = "yc"
version = "0.2.0"

[dependencies]
cydonia-guest = "0.2"

[package.metadata.other]
anything = true

[package.metadata.cydonia]
name = "Y \"Combinator\"\n"
calls = ["tables"]
hosts = ["yc-oss.github.io", "www.ycombinator.com"]
"#;

fn yc() -> Manifest {
    Manifest::from_cargo(CARGO).unwrap()
}

/// A wasm module holding one custom section `name` with `body`.
fn module(name: &str, body: &[u8]) -> Vec<u8> {
    let mut section = vec![name.len() as u8];
    section.extend_from_slice(name.as_bytes());
    section.extend_from_slice(body);
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    wasm.push(0);
    let mut size = section.len();
    loop {
        let byte = (size & 0x7f) as u8;
        size >>= 7;
        match size {
            0 => {
                wasm.push(byte);
                break;
            }
            _ => wasm.push(byte | 0x80),
        }
    }
    wasm.extend(section);
    wasm
}

#[test]
fn a_manifest_is_declared_in_cargo_toml() {
    let manifest = yc();
    assert_eq!(
        manifest,
        Manifest {
            id: "yc".into(),
            name: "Y \"Combinator\"\n".into(),
            version: "0.2.0".into(),
            contract: CONTRACT,
            calls: vec!["tables".into()],
            hosts: vec!["yc-oss.github.io".into(), "www.ycombinator.com".into()],
        }
    );
    assert!(manifest.calls("tables") && !manifest.calls("fetch"));
}

#[test]
fn a_crate_without_the_table_is_named_by_its_crate() {
    let manifest = Manifest::from_cargo(
        "[package]\nname = \"bare\"\nversion.workspace = true\n\
             [dependencies]\ncydonia-guest.workspace = true\n",
    )
    .unwrap();
    assert_eq!(
        (manifest.name.as_str(), manifest.version.as_str()),
        ("bare", "")
    );
    assert!(manifest.calls.is_empty() && manifest.hosts.is_empty());
}

#[test]
fn a_crate_not_built_against_the_guest_is_not_an_app() {
    let refused = Manifest::from_cargo("[package]\nname = \"web\"\n[dependencies]\ngpui = \"1\"\n");
    assert!(
        refused
            .unwrap_err()
            .contains("does not depend on cydonia-guest")
    );
}

#[test]
fn a_key_the_table_does_not_know_is_refused() {
    let typo = "[package]\nname = \"yc\"\n[package.metadata.cydonia]\nhost = [\"a.com\"]\n";
    assert!(Declared::from_cargo(typo).unwrap_err().contains("host"));
}

#[test]
fn a_manifest_is_read_out_of_its_custom_section() {
    let wasm = module(SECTION, &serde_json::to_vec(&yc()).unwrap());
    let manifest = Manifest::read(&wasm).unwrap().unwrap();
    assert_eq!(manifest.id, "yc");
    assert_eq!(Manifest::read(&module("other", b"{}")).unwrap(), None);
    assert!(Manifest::read(b"not wasm").is_err());
}

#[test]
fn a_url_reaches_only_the_hosts_named() {
    let manifest = Manifest {
        id: "yc".into(),
        name: "YC".into(),
        version: "0.1.0".into(),
        contract: CONTRACT,
        calls: Vec::new(),
        hosts: vec!["yc-oss.github.io".into(), "www.ycombinator.com".into()],
    };
    for url in [
        "https://yc-oss.github.io/api/meta.json",
        "http://WWW.ycombinator.com",
        "https://www.ycombinator.com:443/companies?q=a#x",
    ] {
        assert!(manifest.reaches(url), "{url}");
    }
    for url in [
        "https://ycombinator.com/",
        "https://yc-oss.github.io.evil.com/",
        "https://evil.com/yc-oss.github.io",
        "https://yc-oss.github.io@evil.com/",
        "ftp://yc-oss.github.io/",
        "file:///etc/passwd",
        "yc-oss.github.io",
    ] {
        assert!(!manifest.reaches(url), "{url}");
    }
}

#[test]
fn an_app_built_for_a_newer_contract_is_refused_naming_it() {
    let mut manifest = yc();
    assert!(manifest.supported().is_ok());
    manifest.contract = CONTRACT + 1;
    let refused = manifest.supported().unwrap_err();
    assert!(
        refused.contains(&format!("contract {}", CONTRACT + 1)),
        "{refused}"
    );
}
