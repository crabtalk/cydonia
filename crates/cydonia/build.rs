//! The Windows icon, embedded as a resource the exe carries.

use std::{env, fs, path::PathBuf};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_icon();
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
