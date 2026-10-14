//! The manifest an app declares, as JSON and as a custom section.

use cydonia_guest::{
    CONTRACT,
    manifest::{Declared, Manifest, SECTION},
};

const DECLARED: Declared<'static> = Declared {
    id: "yc",
    name: "Y \"Combinator\"\n",
    version: "0.2.0",
    calls: &["tables"],
    hosts: &["yc-oss.github.io", "www.ycombinator.com"],
};

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
fn a_declared_manifest_is_written_as_json_at_compile_time() {
    const BYTES: [u8; DECLARED.size()] = DECLARED.bytes::<{ DECLARED.size() }>();
    let manifest: Manifest = serde_json::from_slice(&BYTES).unwrap();
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
fn a_manifest_is_read_out_of_its_custom_section() {
    let body = DECLARED.bytes::<{ DECLARED.size() }>();
    let wasm = module(SECTION, &body);
    let manifest = Manifest::read(&wasm).unwrap().unwrap();
    assert_eq!(manifest.id, "yc");
    assert_eq!(Manifest::read(&module("other", b"{}")).unwrap(), None);
    assert!(Manifest::read(b"not wasm").is_err());
}
