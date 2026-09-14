//! The MCP section: the tool server this app answers on.
//!
//! Two switches and an address. The port is the system's to choose and nobody
//! types it, so the row is there to say where a thing is listening rather than
//! to be copied out of.
//!
//! What it does not hold yet is the other direction: the servers cydonia
//! *dials*, which `agent/mcp.rs` has been written for and has no caller.

use crate::{
    model::workspace::Workspace,
    view::settings::{SettingsWindow, Switch},
};
use bezel::{
    gpui::{AnyElement, ClipboardItem, Context, div, prelude::*, px},
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        widgets::{ButtonStyle, Buttons, Scaffolding},
    },
};

/// What this section switches. Named rather than reached as fields, for the
/// reason [`crate::model::settings::Feature`] is: one row draws either.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Door {
    Serve,
    Write,
}

impl Door {
    const ALL: [Self; 2] = [Self::Serve, Self::Write];

    /// The mark, the name, and what turning it on means. The two reasons are
    /// not the same kind of thing — one is whether there is a server, the
    /// other is what it will do — so each row says its own.
    fn copy(self) -> (&'static [u8], &'static str, &'static str) {
        match self {
            Self::Serve => (
                icons::development::Plug,
                "Serve",
                "Offers your agents the tools that work a project's boards.",
            ),
            Self::Write => (
                icons::text::Pencil,
                "Agents may edit",
                "Off offers the tools that read a project and none that change it.",
            ),
        }
    }

    fn on(self, workspace: &Workspace) -> bool {
        match self {
            Self::Serve => workspace.settings.mcp.serve,
            Self::Write => workspace.settings.mcp.write,
        }
    }
}

impl SettingsWindow {
    pub(super) fn mcp_body(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        // The other half of what decides whether anything is listening. Said
        // rather than left to be found out from an address that never appears.
        let quiet = workspace.settings.mcp.serve && !workspace.settings.features.sessions;
        theme
            .group_box()
            .children(Door::ALL.map(|switch| self.mcp_switch(switch, cx)))
            .children(quiet.then(|| {
                theme.card_row(false).child(
                    div()
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_muted)
                        .child("Sessions is off in Features, so nothing runs to reach it."),
                )
            }))
            .children(self.mcp_address(cx))
            .into_any_element()
    }

    /// One switch: a mark, what it is, what turning it on means, and the
    /// toggle. The row the features section draws, because they are the same
    /// kind of thing.
    fn mcp_switch(&self, switch: Door, cx: &Context<Self>) -> AnyElement {
        let on = switch.on(self.workspace.read(cx));
        let (glyph, title, blurb) = switch.copy();
        self.switch_row(
            Switch::new(("mcp-switch", switch as usize), title, blurb, on)
                .first(switch == Door::Serve)
                .glyph(glyph)
                .truncate(),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| match switch {
                    Door::Serve => workspace.set_mcp_serve(!on, cx),
                    Door::Write => workspace.set_mcp_write(!on, cx),
                });
            },
        )
    }

    /// Where it is listening, while it is. Nothing at all when it is not — an
    /// address for a port nobody holds is worse than none.
    fn mcp_address(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let url = self.workspace.read(cx).mcp_url()?;
        let copied = url.clone();
        Some(
            theme
                .card_row(false)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(theme.row_title("Address"))
                        .child(
                            div()
                                .mt(px(4.))
                                .truncate()
                                .text_style(TextStyle::Subheadline)
                                .text_color(theme.text_muted)
                                .child(url),
                        ),
                )
                .child(
                    theme
                        .button(
                            "Copy",
                            ButtonStyle::Ghost,
                            Some(Fade::new(painter, "copy-url")),
                        )
                        .id("copy-url")
                        .flex_none()
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                        }),
                )
                .into_any_element(),
        )
    }
}
