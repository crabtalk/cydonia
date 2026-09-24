//! The one thing the binary cannot work out for itself: the commit it was built
//! from. It is asked here rather than at runtime because the app that ships has
//! no repository to ask — a bundle in `/Applications` is a binary and an icon —
//! so the answer is compiled in or it does not exist.

use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo::rustc-env=CYDONIA_COMMIT={}", commit());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_icon();
    }
    // Cargo has no reason of its own to look at git, so without these the
    // stamp is whichever commit was checked out the last time something else
    // forced a rebuild. `--git-path` resolves them through the repository
    // itself, which is what keeps this right inside a worktree, where `.git`
    // is a file pointing somewhere else.
    for file in ["HEAD", "refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", file]) {
            println!("cargo::rerun-if-changed={path}");
        }
    }
}

/// Embeds `assets/icon.png` as icon resource 1, the ID gpui loads for the
/// window and taskbar icon; Explorer shows the same one. The logo is not in
/// git, so a tree without it builds an exe with no icon.
fn windows_icon() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let png = root.join("assets/icon.png");
    println!("cargo::rerun-if-changed={}", png.display());
    let Ok(source) = image::open(&png) else {
        println!(
            "cargo::warning=no {}: building without an icon",
            png.display()
        );
        return;
    };
    let frames: Vec<_> = [16, 24, 32, 48, 64, 256]
        .into_iter()
        .map(|size| {
            let frame = source
                .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
                .into_rgba8();
            image::codecs::ico::IcoFrame::as_png(
                frame.as_raw(),
                size,
                size,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap()
        })
        .collect();

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico = out.join("cydonia.ico");
    image::codecs::ico::IcoEncoder::new(fs::File::create(&ico).unwrap())
        .encode_images(&frames)
        .unwrap();
    let rc = out.join("cydonia.rc");
    fs::write(
        &rc,
        format!(
            "1 ICON \"{}\"\n",
            ico.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}

/// `1a2b3c4`, and `1a2b3c4-dirty` where the tree has been edited since — the
/// suffix git's own `describe` uses, because a build with changes in it is not
/// the commit it names.
///
/// `unknown` where there is no repository at all: a build from a tarball —
/// `cargo install`, a vendored tree — is still a build, so this says what it
/// does not know rather than failing.
fn commit() -> String {
    let Some(short) = git(&["rev-parse", "--short=7", "HEAD"]) else {
        return "unknown".into();
    };
    match git(&["status", "--porcelain"]).is_none_or(|tree| tree.is_empty()) {
        true => short,
        false => format!("{short}-dirty"),
    }
}

/// One git command, or nothing at all: every caller here has an answer for a
/// machine with no git on it.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
}
