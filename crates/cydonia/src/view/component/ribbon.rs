//! The formatting ribbon: the bar that floats over a run of selected text.
//!
//! Notion's shape. The bar arrives where the run ends, carries the block's own
//! name and the marks over it, and goes when the run does. Nothing is offered
//! at a bare caret: with no words picked out there is nothing for a button to
//! be about.
//!
//! It is the app's rather than the editor's because [`editor::Formatting`] is
//! written to be read by exactly this — which marks are lit, what the caret's
//! block is called, and whether code here means a fence — and because the
//! editor cannot know that this app has spent `cmd-b` elsewhere.
//!
//! What it offers is what markdown spells: bold, italic, strikethrough, code
//! and a link. Notion's own bar carries underline, a highlight and a colour
//! too, and none of the three has a spelling CommonMark reads — see
//! `markdown::Marks`. A document written with a delimiter this app invented is
//! a document only this app can read back, and these are files on somebody's
//! disk.

use crate::view::{
    component::menu::{self, Menu},
    root::Cydonia,
};
use bezel::{
    gpui::{
        self, Anchor, AnyElement, Bounds, Context, Entity, Focusable as _, KeyBinding, Pixels,
        Point, SharedString, Window, actions, div, point, prelude::*, px,
    },
    motion,
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons::{self, Icon},
        input::TextField,
        menu::Item,
        popover, surface,
        tooltip::Tooltip,
        widgets::Buttons as _,
    },
};
use artifact::layout::Member;
use editor::{Editor, Formatting, Mode};
use markdown::{BlockKind, Mark};

actions!(cydonia_ribbon, [ConfirmLink, DismissLink]);

/// Claimed on the ribbon's URL field, so `enter` files the link and `escape`
/// drops it. Everywhere else those two stay a newline and nothing.
pub const LINK_CONTEXT: &str = "CydoniaArticleLink";

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(LINK_CONTEXT);
    vec![
        KeyBinding::new("enter", ConfirmLink, ctx),
        KeyBinding::new("escape", DismissLink, ctx),
    ]
}

/// How far the bar stands off the line it belongs to.
const GAP: f32 = 8.;

/// A button's box. Square for the marks, and the height every other thing in
/// the bar takes so the row has one line to sit on.
const BUTTON: f32 = 26.;

/// How wide the URL field runs. Wide enough for a link worth reading and no
/// wider: the bar is anchored to a word, and one that outgrew the column would
/// stop reading as that word's bar.
const FIELD_WIDTH: f32 = 240.;

/// The ribbon's own state, hung on [`Cydonia`].
pub(crate) struct Ribbon {
    /// The pointer is dragging a run out. The bar waits for the release: one
    /// that followed the pointer would sit over the very words being chosen,
    /// and would move on every sample.
    pub(crate) selecting: bool,
    /// The link being written, while the URL field is up.
    pub(crate) linking: Option<Linking>,
    /// That field. Built once with the window, like the rename field — one
    /// made where the bar is drawn would be a new entity every frame, and a
    /// new entity every frame is a field that cannot be typed into.
    pub(crate) field: Entity<TextField>,
}

impl Ribbon {
    pub(crate) fn new(cx: &mut Context<Cydonia>) -> Self {
        let field = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_key_context(LINK_CONTEXT)
                .with_placeholder("Paste a link…")
        });
        Self {
            selecting: false,
            linking: None,
            field,
        }
    }
}

/// An open URL field.
pub(crate) struct Linking {
    /// The link the selection already carries, when one is being changed
    /// rather than made. It comes off before the new one goes on: a mark *is*
    /// its URL, so adding alone would leave two links over the same words and
    /// the serializer with a choice nobody made.
    pub(crate) replacing: Option<String>,
}

/// One of the bar's mark buttons.
pub struct Format {
    pub mark: Mark,
    /// What the button is called, in its tooltip.
    pub label: &'static str,
    pub icon: Icon,
    /// The selection carries this mark throughout — the button is lit.
    pub lit: bool,
}

/// The marks the bar offers, in the order Notion's own puts them, each paired
/// with whether the selection already carries it.
///
/// Code is the one whose name changes with what is picked: over more than one
/// line the editor makes a fence out of the selection rather than an inline
/// span, and the tooltip is the only place to say so before the click. See
/// [`Formatting::fenceable`].
pub fn formats(formatting: &Formatting) -> Vec<Format> {
    let code = match formatting.fenceable {
        true => "Code block",
        false => "Code",
    };
    [
        (Mark::Bold, "Bold", icons::text::Bold),
        (Mark::Italic, "Italic", icons::text::Italic),
        (Mark::Strike, "Strikethrough", icons::text::Strikethrough),
        (Mark::Code, code, icons::text::Code),
    ]
    .into_iter()
    .map(|(mark, label, icon)| Format {
        lit: formatting.marks.contains(&mark),
        mark,
        label,
        icon: Icon::glyph(icon),
    })
    .collect()
}

/// The chord that reaches the same mark from the keyboard, for the tooltip to
/// print beside the name.
///
/// Bold has none, and that is the whole reason this bar is reachable at all.
/// `cmd-b` is the sidebar's here — the menu bar carries it, so AppKit takes
/// the chord before the window is offered it, see
/// [`crate::view::keymap::Command`] — and the editor's own bold is never
/// reached. A tooltip printing ⌘B would be documenting a lie.
///
/// Code is the second one spent that way: ⌘E is Plain text here. Both are
/// chords the reader can move, so what a moved one leaves behind is the
/// editor's own mark, reachable from this bar either way.
pub fn keystroke(mark: &Mark) -> Option<&'static str> {
    match mark {
        Mark::Italic => Some("⌘I"),
        Mark::Strike => Some("⇧⌘X"),
        _ => None,
    }
}

/// The link the selection carries throughout, if it carries one.
///
/// [`Formatting::marks`] holds only marks covering the whole run, so a link
/// found here is one the whole selection is inside — which is the only case
/// where changing it has one answer.
pub fn link(formatting: &Formatting) -> Option<&str> {
    formatting.marks.iter().find_map(|mark| match mark {
        Mark::Link(url) => Some(url.as_str()),
        _ => None,
    })
}

/// Where the bar's bottom-left corner goes, in window coordinates: just above
/// the row the selection ends on.
///
/// `None` when that row is not inside `view`, the document's scroll box. The
/// editor answers where the selection last painted whether or not it is still
/// on screen, and a bar left pointing at a line scrolled away is a bar
/// floating over the wrong words.
pub fn perch(view: Bounds<Pixels>, head: Bounds<Pixels>) -> Option<Point<Pixels>> {
    let top = head.origin.y;
    let bottom = top + head.size.height;
    (top >= view.origin.y && bottom <= view.origin.y + view.size.height)
        .then(|| point(head.origin.x, top - px(GAP)))
}

impl Cydonia {
    // ── mutations ────────────────────────────────────────────────

    /// Note that the pointer is choosing a run, and that it has stopped. The
    /// document's own press is what the editor hears; this is only the bar
    /// getting out of the way of it.
    pub(crate) fn set_selecting(&mut self, selecting: bool, cx: &mut Context<Self>) {
        if self.leaf().ribbon.selecting != selecting {
            self.leaf_mut().ribbon.selecting = selecting;
            cx.notify();
        }
    }

    /// Put the bar back to rest — what leaving the pane does, and what a
    /// re-read that replaced the document does.
    ///
    /// The URL is dropped rather than filed. Filing it would put the link on
    /// whichever document is open by the time this runs, and by the time this
    /// runs that is no longer the one the run was in.
    pub(crate) fn rest_ribbon(&mut self, cx: &mut Context<Self>) {
        if self.leaf_mut().ribbon.linking.take().is_some() {
            self.leaf()
                .ribbon
                .field
                .update(cx, |field, cx| field.clear(cx));
            cx.notify();
        }
        self.set_selecting(false, cx);
    }

    /// Turn every block the selection touches into `kind` — the bar's own
    /// reach past the gutter handle's menu, which acts on the one block it
    /// hangs off.
    ///
    /// One step per block, so undoing a five-paragraph turn takes five. The
    /// editor's step is per call and there is no way from out here to open one
    /// around the run.
    fn turn_blocks(&mut self, kind: BlockKind, cx: &mut Context<Self>) {
        let Some(editor) = self.open_editor(cx) else {
            return;
        };
        editor.update(cx, |editor, cx| {
            let (start, end) = editor.selection().ordered();
            for ix in start.block..=end.block {
                editor.set_block(ix, kind.clone(), cx);
            }
        });
    }

    /// Put the URL field up, over whatever link the run already carries.
    fn open_link(
        &mut self,
        replacing: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let held = replacing.clone().unwrap_or_default();
        self.leaf()
            .ribbon
            .field
            .update(cx, |field, cx| field.set_content(held, cx));
        self.leaf_mut().ribbon.linking = Some(Linking { replacing });
        window.focus(&self.leaf().ribbon.field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// File what was typed, and give the caret back to the document.
    ///
    /// An empty field takes the link off. It is the only thing emptying one
    /// could mean, and it saves a second button for the removal.
    fn confirm_link(&mut self, _: &ConfirmLink, window: &mut Window, cx: &mut Context<Self>) {
        let Some(linking) = self.leaf_mut().ribbon.linking.take() else {
            return;
        };
        let url = self.leaf().ribbon.field.read(cx).content().trim().to_string();
        if let Some(editor) = self.open_editor(cx) {
            editor.update(cx, |editor, cx| {
                if let Some(old) = linking.replacing {
                    editor.toggle_mark(Mark::Link(old), cx);
                }
                if !url.is_empty() {
                    editor.toggle_mark(Mark::Link(url), cx);
                }
            });
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Drop the field and leave the run as it was.
    fn dismiss_link(&mut self, _: &DismissLink, window: &mut Window, cx: &mut Context<Self>) {
        if self.leaf_mut().ribbon.linking.take().is_none() {
            return;
        }
        if let Some(editor) = self.open_editor(cx) {
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// The open document's editing surface, or nothing where no article is
    /// open — every one of the bar's acts asks for it.
    fn open_editor(&self, cx: &Context<Self>) -> Option<Entity<Editor>> {
        self.workspace
            .read(cx)
            .active_article()
            .and_then(|article| article.editor.clone())
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The bar, when there is a run of text for it to be about.
    /// `on` is the pane drawing it. The bar is built from the selection in the
    /// focused pane's editor, so a pane that is not the focused one draws none
    /// — otherwise every article pane on screen carries a copy of it, perched
    /// where another pane's document put its selection.
    pub(crate) fn ribbon(
        &self,
        on: Option<&Member>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if on.is_some() && self.leaf().entry.as_ref() != on {
            return None;
        }
        // Held down: the run is still being chosen.
        if self.leaf().ribbon.selecting {
            return None;
        }
        let article = self.workspace.read(cx).active_article()?;
        let editor = article.editor.clone()?;
        let port = article.scroll.bounds();
        let (formatting, head) = {
            let held = editor.read(cx);
            (held.formatting(), held.selection_bounds()?)
        };
        // In the source the markup is already spelled out and the editor
        // refuses every toggle, so there is nothing here to light or to do.
        if formatting.mode != Mode::Blocks {
            return None;
        }
        // Writing a link takes the focus off the document, and the bar is what
        // is holding the field it went to.
        let linking = self.leaf().ribbon.linking.is_some();
        if !linking && !editor.focus_handle(cx).contains_focused(window, cx) {
            return None;
        }
        let at = perch(port, head)?;

        let theme = Theme::of(cx).clone();
        let card = popover::popover_card(&theme)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .on_action(cx.listener(Self::confirm_link))
            .on_action(cx.listener(Self::dismiss_link));
        let card = match linking {
            true => card.child(self.link_field(&theme, cx)),
            false => card
                .child(self.turn_trigger(&formatting, &theme, cx))
                .child(divider(&theme))
                .children(
                    formats(&formatting)
                        .into_iter()
                        .map(|format| self.mark_button(format, editor.clone(), &theme, cx)),
                )
                .child(self.link_button(&formatting, &theme, cx)),
        };
        Some(floated("article-ribbon", at, card.into_any_element()))
    }

    /// One mark, lit where the run already carries it.
    fn mark_button(
        &self,
        format: Format,
        editor: Entity<Editor>,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let Format {
            mark,
            label,
            icon,
            lit,
        } = format;
        // The lit state is the glyph's colour and not a fill behind it, so it
        // never argues with the hover wash the button already paints.
        let tint = match lit {
            true => theme.accent,
            false => theme.text_muted,
        };
        let key = keystroke(&mark);
        theme
            .ghost(SharedString::from(format!("ribbon-{label}")))
            .w(px(BUTTON))
            .h(px(BUTTON))
            .justify_center()
            .child(icons::icon(icon).size(px(15.)).text_color(tint))
            .tooltip(move |window, cx| match key {
                Some(key) => Tooltip::with_keystroke(label, key, window, cx),
                None => Tooltip::text(label, window, cx),
            })
            .on_click(cx.listener(move |_, _, _, cx| {
                editor.update(cx, |editor, cx| editor.toggle_mark(mark.clone(), cx));
            }))
    }

    /// The link button: it opens the field either way, over the link the run
    /// carries or over nothing.
    fn link_button(
        &self,
        formatting: &Formatting,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let held = link(formatting).map(str::to_owned);
        let label = match held.is_some() {
            true => "Edit link",
            false => "Link",
        };
        let tint = match held.is_some() {
            true => theme.accent,
            false => theme.text_muted,
        };
        theme
            .ghost("ribbon-link")
            .w(px(BUTTON))
            .h(px(BUTTON))
            .justify_center()
            .child(
                icons::icon(icons::text::Link)
                    .size(px(15.))
                    .text_color(tint),
            )
            .tooltip(move |window, cx| Tooltip::text(label, window, cx))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_link(held.clone(), window, cx);
            }))
    }

    /// The URL field, in the bar's own box rather than a panel under it: the
    /// bar is already anchored to the words the link is about.
    fn link_field(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement + use<> {
        div()
            .w(px(FIELD_WIDTH))
            .h(px(BUTTON))
            .px(px(6.))
            .flex()
            .items_center()
            .text_style(TextStyle::Callout)
            .text_color(theme.text)
            // Pressing anywhere else is finishing, the way the rename field
            // has it: the URL typed is the URL meant. `escape` discards.
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.confirm_link(&ConfirmLink, window, cx);
            }))
            .child(self.leaf().ribbon.field.clone())
    }

    /// What the caret's block is called, and the menu of what it could be.
    fn turn_trigger(
        &self,
        formatting: &Formatting,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let current = formatting.block.clone();
        let button = theme
            .ghost("ribbon-turn")
            .h(px(BUTTON))
            .px(px(6.))
            .gap(px(4.))
            .relative()
            .text_style(TextStyle::Callout)
            .text_color(theme.text)
            .child(
                // A block the vocabulary does not offer — a bookmark — has no
                // name to show, so the button says what it is for instead.
                current.clone().unwrap_or_else(|| "Turn into".into()),
            )
            .child(
                icons::icon(icons::arrows::ChevronDown)
                    .size(px(12.))
                    .text_color(theme.text_faint),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(Menu::Turn, cx);
            }))
            .children(self.turn_menu(current, cx));
        self.menu_press(button, Menu::Turn, cx)
    }

    /// Every block the run could be turned into — the editor's own vocabulary,
    /// which is what the slash menu and the gutter handle both offer.
    fn turn_menu(
        &self,
        current: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu.as_ref() != Some(&Menu::Turn) {
            return None;
        }
        let rows = editor::turns()
            .into_iter()
            .map(|(label, kind)| {
                let checked = current.as_ref() == Some(&label);
                menu::row(Item::action(label).checked(checked), move |this, _, cx| {
                    this.turn_blocks(kind.clone(), cx);
                })
            })
            .collect();
        let id = SharedString::from("ribbon-turn-card");
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }
}

/// The rule between the block's name and the marks. Notion breaks the two
/// clusters the same way, and a bar without it reads as one long row of
/// unrelated controls.
fn divider(theme: &Theme) -> impl IntoElement {
    div()
        .w(px(1.))
        .h(px(16.))
        .mx(px(2.))
        .flex_none()
        .bg(theme.border)
}

/// The bar's own layer: over everything, at a window position, with its
/// **bottom** left corner at `at` so it stands above the line rather than on
/// it. The transcript's selection bar stands on this too.
///
/// Its own rather than [`popover::menu_at`], which pins a card's top-left —
/// the one corner a bar over a line of text must not be pinned by. Near the
/// top of the window gpui's `anchored` switches the corner itself and the bar
/// drops below the line, which is where Notion's goes there too.
pub(crate) fn floated(id: &'static str, at: Point<Pixels>, content: AnyElement) -> AnyElement {
    gpui::deferred(
        gpui::anchored()
            .position(at)
            .anchor(Anchor::BottomLeft)
            .child(motion::menu_in(
                id,
                // Hitboxes are paint order only, so without this a press on a
                // button would also land on the document underneath it and
                // move the caret out of the run being formatted.
                div()
                    .occlude()
                    .child(surface::popover(Theme::surface_radius(), content)),
            )),
    )
    .priority(1)
    .into_any_element()
}
