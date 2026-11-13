use crate::{
    model::watch::path::tests as changes,
    view::component::file::{
        picture::Picture,
        pictures::{Assets, Pictures},
    },
};
use bezel::gpui::{self, App, Context, Entity, TestAppContext, WeakEntity, prelude::*, px};
use std::path::Path;

fn write(path: &Path, height: u32) {
    image::RgbaImage::new(10, height).save(path).unwrap();
}

fn height(path: &Path, cx: &mut App) -> Option<u32> {
    let resource = gpui::Resource::Path(path.into());
    let image = cx.fetch_asset::<gpui::ImgResourceLoader>(&resource)?.ok()?;
    Some(image.size(0).height.0 as u32)
}

#[gpui::test]
fn image_nodes_share_the_same_file_watch(cx: &mut TestAppContext) {
    let path =
        std::env::temp_dir().join(format!("cydonia-image-watch-{}.png", uuid::Uuid::now_v7()));
    let _changes = cx.update(|cx| changes::open(&path, cx));
    let first = cx.update(|cx| Picture::open(path.clone(), WeakEntity::new_invalid(), cx));
    let alias = path
        .parent()
        .unwrap()
        .join(".")
        .join(path.file_name().unwrap());
    let second = cx.update(|cx| Picture::open(alias, WeakEntity::new_invalid(), cx));
    assert_eq!(first.entity_id(), second.entity_id());
}

#[gpui::test]
fn a_saved_picture_reloads(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("cydonia-pictures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("picture.png");
    write(&file, 10);
    let changed = cx.update(|cx| changes::open(&file, cx));
    let pictures = cx.new(|cx| {
        Pictures::new(
            dir.clone(),
            Assets::Dir(dir.clone()),
            WeakEntity::new_invalid(),
            cx,
        )
    });
    pictures.update(cx, |this, cx| this.watch(file.to_str().unwrap(), cx));
    cx.update(|cx| height(&file, cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| height(&file, cx)), Some(10));

    write(&file, 60);
    cx.update(|cx| changes::publish(&changed, cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| height(&file, cx)), Some(60));
}

struct Page {
    editor: Entity<editor::Editor>,
    _pictures: Entity<Pictures>,
}

impl gpui::Render for Page {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div().size_full().child(self.editor.clone())
    }
}

#[gpui::test]
fn a_saved_picture_repaints_in_the_editor(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("cydonia-editor-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    let file = dir.join("assets/picture.png");
    write(&file, 10);
    let changed = cx.update(|cx| changes::open(&file, cx));
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Dark, cx));
    let window = cx.add_window({
        let dir = dir.clone();
        move |_, cx| {
            let pictures = cx.new(|cx| {
                Pictures::new(
                    dir.clone(),
                    Assets::Dir(dir.clone()),
                    WeakEntity::new_invalid(),
                    cx,
                )
            });
            let overlay = Pictures::overlay(&pictures);
            let editor = cx.new(|cx| {
                editor::Editor::new("![](assets/picture.png)", cx)
                    .with_image_overlay(overlay)
                    .with_base(&dir)
            });
            cx.observe(&pictures, {
                let editor = editor.downgrade();
                move |_, _, cx| {
                    let _ = editor.update(cx, |_, cx| cx.notify());
                }
            })
            .detach();
            Page {
                editor,
                _pictures: pictures,
            }
        }
    });
    let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(gpui::size(px(400.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let page = window.root(&mut cx).unwrap();
    let shown = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            page.read(cx)
                .editor
                .read(cx)
                .layouts()
                .picture_bounds(0)
                .map(|b| b.size.height)
        })
    };
    let before = shown(&mut cx);

    let temp = dir.join("assets/.picture.tmp.png");
    write(&temp, 60);
    std::fs::rename(&temp, &file).unwrap();
    cx.update(|_, cx| changes::publish(&changed, cx));
    cx.run_until_parked();
    assert_eq!((before, shown(&mut cx)), (Some(px(10.)), Some(px(60.))));
}

#[gpui::test]
fn a_picture_saved_between_documents_reloads(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("cydonia-between-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("picture.png");
    write(&file, 10);
    let initial = cx.update(|cx| changes::open(&file, cx));
    let first = cx.new(|cx| {
        Pictures::new(
            dir.clone(),
            Assets::Dir(dir.clone()),
            WeakEntity::new_invalid(),
            cx,
        )
    });
    first.update(cx, |this, cx| this.watch(file.to_str().unwrap(), cx));
    cx.update(|cx| height(&file, cx));
    cx.run_until_parked();
    drop(first);
    drop(initial);
    cx.run_until_parked();

    write(&file, 60);
    let _changed = cx.update(|cx| changes::open(&file, cx));
    let second = cx.new(|cx| {
        Pictures::new(
            dir.clone(),
            Assets::Dir(dir.clone()),
            WeakEntity::new_invalid(),
            cx,
        )
    });
    second.update(cx, |this, cx| this.watch(file.to_str().unwrap(), cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| height(&file, cx)), Some(60));
}

#[gpui::test]
fn deleted_and_recreated_picture_discards_cached_decode_errors(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!(
        "cydonia-recreated-picture-{}",
        uuid::Uuid::now_v7()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("picture.png");
    write(&file, 10);
    let changed = cx.update(|cx| changes::open(&file, cx));
    let pictures = cx.new(|cx| {
        Pictures::new(
            dir.clone(),
            Assets::Dir(dir.clone()),
            WeakEntity::new_invalid(),
            cx,
        )
    });
    pictures.update(cx, |pictures, cx| {
        pictures.watch(file.to_str().unwrap(), cx)
    });
    cx.update(|cx| height(&file, cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| height(&file, cx)), Some(10));

    std::fs::remove_file(&file).unwrap();
    cx.update(|cx| changes::publish(&changed, cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| height(&file, cx)), None);

    write(&file, 40);
    cx.update(|cx| changes::publish(&changed, cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| height(&file, cx)), Some(40));
    std::fs::remove_dir_all(dir).unwrap();
}
