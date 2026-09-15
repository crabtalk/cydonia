#[path = "build/bundle.rs"]
mod bundle;

fn main() {
    println!("cargo:rerun-if-changed=skills");
    println!("cargo:rerun-if-changed=instructions");
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = bundle::generate(&root.join("skills")).expect("valid built-in skills");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("skills.rs"), source).expect("write bundled skills");
}
