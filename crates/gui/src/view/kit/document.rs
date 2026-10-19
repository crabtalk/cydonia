//! The editor over a [`artifact::document`] directory, which an article and a
//! card both are: its pictures resolve against the directory, pasted and
//! dropped ones land in its `assets/`, and mentions are on.

use crate::{
    model::{media::Shelf, workspace::Workspace},
    view::component::file::pictures::Pictures,
};
use bezel::gpui::{App, AppContext as _, Entity, WeakEntity};
use editor::{Document, Editor};
use std::path::Path;

/// An editor over `document`, the one in `dir`, and the pictures beside it,
/// which file a picture fetched off the web on `shelf`. `configure` sets what
/// differs between hosts — text size, scroll. The pictures must be held as
/// long as the editor is.
pub(crate) fn editor(
    dir: &Path,
    shelf: Shelf,
    document: Entity<Document>,
    workspace: WeakEntity<Workspace>,
    configure: impl FnOnce(Editor) -> Editor,
    cx: &mut App,
) -> (Entity<Editor>, Entity<Pictures>) {
    let pictures = cx.new(|cx| Pictures::new(dir.to_path_buf(), Some(shelf), workspace, cx));
    let overlay = Pictures::overlay(&pictures);
    let editor = cx.new(|cx| {
        configure(
            Editor::for_document(document, cx)
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
