//! `cydonia-guest`'s [`app!`]. Use it through `cydonia_guest::app!`.

use manifest::{Declared, SECTION};
use proc_macro::{TokenStream, TokenTree};
use std::path::PathBuf;

/// Export an app's type to Cydonia, with its manifest. Once, in the app's
/// crate root:
///
/// ```ignore
/// cydonia_guest::app!(Yc);
/// ```
///
/// The manifest is the crate's name and version and what its `Cargo.toml`
/// declares under `[package.metadata.cydonia]`:
///
/// ```toml
/// [package.metadata.cydonia]
/// name = "Y Combinator"
/// calls = ["tables"]
/// hosts = ["yc-oss.github.io"]
/// ```
///
/// `name` defaults to the crate's name; `calls` names the groups in
/// `cydonia_guest::manifest::calls` the app makes, and `hosts` the hosts it
/// fetches from. A key the table does not know is a compile error.
///
/// The expansion names `::cydonia_guest`: the app depends on `cydonia-guest`
/// under that name.
#[proc_macro]
pub fn app(app: TokenStream) -> TokenStream {
    let mut depth = 0i32;
    let more = app.clone().into_iter().any(|token| {
        let TokenTree::Punct(punct) = token else {
            return false;
        };
        match punct.as_char() {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' => return depth == 0,
            _ => {}
        }
        false
    });
    let section = match more {
        true => Err(
            "app! takes the app's type alone: name, calls and hosts go under \
             [package.metadata.cydonia] in the crate's Cargo.toml"
                .to_owned(),
        ),
        false => section(),
    };
    match section {
        Ok(section) => format!("{section}\n::cydonia_guest::__exports!({app});"),
        Err(error) => format!("::std::compile_error!({error:?});"),
    }
    .parse()
    .unwrap()
}

/// The manifest's custom section, as items. Rustc reads the crate's
/// `Cargo.toml` through `include_bytes!` as well, so a change to it builds the
/// crate again.
fn section() -> Result<String, String> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| "app! is expanded by cargo")?;
    let path = PathBuf::from(dir).join("Cargo.toml");
    let toml = std::fs::read_to_string(&path)
        .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
    let declared = Declared::from_cargo(&toml)
        .map_err(|error| format!("[package.metadata.cydonia] in {}: {error}", path.display()))?;
    let env = |key: &str| std::env::var(key).map_err(|_| format!("cargo sets no {key}"));
    let manifest = declared.manifest(env("CARGO_PKG_NAME")?, env("CARGO_PKG_VERSION")?);
    let bytes = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    let listed: Vec<String> = bytes.iter().map(u8::to_string).collect();
    Ok(format!(
        "#[cfg(target_arch = \"wasm32\")]
        const _: () = {{
            const _: &[u8] = ::std::include_bytes!({path:?});
            #[unsafe(link_section = {SECTION:?})]
            #[used]
            static MANIFEST: [u8; {len}] = [{listed}];
        }};",
        path = path.display().to_string(),
        len = bytes.len(),
        listed = listed.join(","),
    ))
}
