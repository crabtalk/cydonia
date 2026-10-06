//! The editor over a [`artifact::document`] directory, which an article and a
//! card both are: its pictures resolve against the directory, pasted and
//! dropped ones land in its `assets/`, and mentions are on.

use crate::{
    model::workspace::Workspace,
    view::component::file::pictures::{Assets, Pictures},
};
use bezel::gpui::{App, AppContext as _, Entity, WeakEntity};
use editor::Editor;
use std::path::Path;

/// An editor over `text`, the document in `dir`, and the pictures beside it.
/// `configure` sets what differs between hosts — text size, scroll, mode.
/// The pictures must be held as long as the editor is.
pub(crate) fn editor(
    dir: &Path,
    text: &str,
    workspace: WeakEntity<Workspace>,
    configure: impl FnOnce(Editor) -> Editor,
    cx: &mut App,
) -> (Entity<Editor>, Entity<Pictures>) {
    let pictures = cx.new(|cx| {
        let assets = artifact::document::assets(&artifact::document::content(dir));
        Pictures::new(dir.to_path_buf(), Assets::Dir(assets), workspace, cx)
    });
    let overlay = Pictures::overlay(&pictures);
    let editor = cx.new(|cx| {
        configure(
            Editor::new(text, cx)
                .with_image_overlay(overlay)
                .with_chrome(editor::Chrome {
                    mention: true,
                    ..editor::Chrome::default()
                })
                .with_base(dir),
        )
    });
    pictures.update(cx, |pictures, _| pictures.relink_in(editor.downgrade()));
    // A picture saved in another app, or its list of apps arriving, repaints
    // the document.
    cx.observe(&pictures, {
        let editor = editor.downgrade();
        move |_, cx| {
            let _ = editor.update(cx, |_, cx| cx.notify());
        }
    })
    .detach();
    (editor, pictures)
}
