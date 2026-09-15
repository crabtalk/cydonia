#[path = "build/bundle.rs"]
mod bundle;

fn main() {
    println!("cargo:rerun-if-changed=resources");
    println!("cargo:rerun-if-changed=instructions");
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = bundle::generate(&root.join("resources")).expect("valid built-in resources");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("resources.rs"), source).expect("write bundled resources");
}
