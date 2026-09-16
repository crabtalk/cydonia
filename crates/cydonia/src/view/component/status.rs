//! Shared bottom status row and copyable paths.
use bezel::{
    gpui::{self, ClipboardItem, Div, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::tooltip::Tooltip,
};
use std::path::Path;

pub fn bar(theme: &Theme) -> Div {
    div()
        .h(px(30.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(8.))
        .border_t_1()
        .border_color(theme.border)
        .text_style(TextStyle::Caption)
        .text_color(theme.text_muted)
}

pub fn path(path: &Path, label: String) -> gpui::Stateful<Div> {
    let full = path.display().to_string();
    let tooltip = full.clone();
    div()
        .id("status-path")
        .flex_1()
        .min_w_0()
        .truncate()
        .cursor_pointer()
        .tooltip(move |window, cx| {
            Tooltip::text(format!("{tooltip}\nClick to copy path"), window, cx)
        })
        .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(full.clone())))
        .child(label)
}

pub fn terminal(path: &Path, theme: &Theme) -> Div {
    bar(theme)
        .child(self::path(path, path.display().to_string()))
        .child("Terminal")
}

pub fn files_toggle(open: bool, theme: &Theme) -> gpui::Stateful<Div> {
    use bezel::ui::icons;
    div()
        .id("status-files")
        .size(px(24.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .cursor_pointer()
        .hover(|button| button.bg(theme.element_hover))
        .tooltip(move |window, cx| {
            Tooltip::text(if open { "Hide files" } else { "Show files" }, window, cx)
        })
        .on_click(|_, window, cx| window.dispatch_action(Box::new(super::panel::ToggleFiles), cx))
        .child(
            icons::icon(if open {
                icons::files::FolderOpen
            } else {
                icons::files::Folder
            })
            .size(px(13.))
            .text_color(theme.text_muted),
        )
}
