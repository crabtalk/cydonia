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
//! What it offers: Ask and Explain while the article has a composer under it,
//! then bold, italic, strikethrough, code, a highlight (`==text==`,
//! [`editor::HIGHLIGHT_MARK`]) and a link.

use crate::view::kit::menu;
use crate::view::{
    component::composer,
    pane::{Tab, TabMenu},
    window::root::Cydonia,
};
use artifact::reference;
use bezel::{
    gpui::{
        self, Anchor, AnyElement, App, Bounds, Context, Entity, Focusable as _, KeyBinding, Pixels,
        Point, SharedString, Window, actions, div, point, prelude::*, px,
    },
    motion,
    theme::{TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        icons::{self, Icon},
        input::TextField,
        menu::Item,
        popover, surface,
        tooltip::Tooltip,
        widgets::Buttons as _,
    },
};
use editor::{Editor, Formatting, Mode};
use markdown::{BlockKind, Mark};
use std::path::Path;

/// Claimed on the ribbon's URL field, so `enter` files the link and `escape`
/// drops it. Everywhere else those two stay a newline and nothing.
pub const LINK_CONTEXT: &str = "CydoniaArticleLink";

/// Claimed on the ribbon's question field, the same way.
pub const ASK_CONTEXT: &str = "CydoniaArticleAsk";

/// How far the bar stands off the line it belongs to.
const GAP: f32 = 8.;

/// A button's box. Square for the marks, and the height every other thing in
/// the bar takes so the row has one line to sit on.
const BUTTON: f32 = 26.;

/// The most of a passage Explain's question quotes, in characters.
const QUESTION_QUOTE: usize = 60;

/// How wide the URL field runs. Wide enough for a link worth reading and no
/// wider: the bar is anchored to a word, and one that outgrew the column would
/// stop reading as that word's bar.
const FIELD_WIDTH: f32 = 240.;

actions!(
    cydonia_ribbon,
    [ConfirmLink, DismissLink, ConfirmAsk, DismissAsk]
);

pub fn bindings() -> Vec<KeyBinding> {
    let (link, ask) = (Some(LINK_CONTEXT), Some(ASK_CONTEXT));
    vec![
        KeyBinding::new("enter", ConfirmLink, link),
        KeyBinding::new("escape", DismissLink, link),
        KeyBinding::new("enter", ConfirmAsk, ask),
        KeyBinding::new("escape", DismissAsk, ask),
    ]
}

/// The ribbon's own state, hung on its [`Tab`].
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
    /// Whether the question field is up, over a run being asked about.
    pub(crate) asking: bool,
    /// That field, built once like the URL field.
    pub(crate) question: Entity<TextField>,
}

impl Ribbon {
    pub(crate) fn new(cx: &mut bezel::gpui::App) -> Self {
        let field = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_key_context(LINK_CONTEXT)
                .with_placeholder("Paste a link…")
        });
        let question = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_key_context(ASK_CONTEXT)
                .with_placeholder("Ask about this passage…")
        });
        Self {
            selecting: false,
            linking: None,
            field,
            asking: false,
            question,
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
        (
            Mark::Custom(editor::HIGHLIGHT_MARK.into()),
            "Highlight",
            icons::text::Highlighter,
        ),
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
pub fn keystroke(mark: &Mark) -> Option<SharedString> {
    let chord = match mark {
        Mark::Italic => "secondary-i",
        Mark::Strike => "secondary-shift-x",
        Mark::Custom(name) if name == editor::HIGHLIGHT_MARK => "secondary-shift-h",
        _ => return None,
    };
    Some(bezel::ui::keys::printed(chord))
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

impl Tab {
    // ── mutations ────────────────────────────────────────────────

    /// Note that the pointer is choosing a run, and that it has stopped. The
    /// document's own press is what the editor hears; this is only the bar
    /// getting out of the way of it.
    pub(crate) fn set_selecting(&mut self, selecting: bool, cx: &mut Context<Self>) {
        if self.ribbon.selecting != selecting {
            self.ribbon.selecting = selecting;
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
        if self.ribbon.linking.take().is_some() {
            self.ribbon.field.update(cx, |field, cx| field.clear(cx));
            cx.notify();
        }
        if std::mem::take(&mut self.ribbon.asking) {
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
        self.ribbon
            .field
            .update(cx, |field, cx| field.set_content(held, cx));
        self.ribbon.linking = Some(Linking { replacing });
        window.focus(&self.ribbon.field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// File what was typed, and give the caret back to the document.
    ///
    /// An empty field takes the link off. It is the only thing emptying one
    /// could mean, and it saves a second button for the removal.
    fn confirm_link(&mut self, _: &ConfirmLink, window: &mut Window, cx: &mut Context<Self>) {
        let Some(linking) = self.ribbon.linking.take() else {
            return;
        };
        let url = self.ribbon.field.read(cx).content().trim().to_string();
        if let Some(editor) = self.open_editor(cx) {
            let url = typed_link(&url, editor.read(cx).base());
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
        if self.ribbon.linking.take().is_none() {
            return;
        }
        if let Some(editor) = self.open_editor(cx) {
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// The selection as the article's markdown spells it with a chip linking
    /// the lines it is on under it, where the article has a number; and the
    /// passage alone.
    fn quoted_selection(&self, cx: &Context<Self>) -> Option<(String, String)> {
        let editor = self.open_editor(cx)?;
        let editor = editor.read(cx);
        let (start, end) = editor.selection().ordered();
        let (source, at) = markdown::serialize_at_many(
            editor.doc(),
            &[start, end],
            &crate::view::article::marks(),
        );
        let (from, to) = (at[0]?, at[1]?);
        let passage = source.get(from..to)?.trim();
        if passage.is_empty() {
            return None;
        }
        let start = from + (source[from..to].len() - source[from..to].trim_start().len());
        let lines = artifact::article::anchor::span_of(
            &source,
            start..from + source[from..to].trim_end().len(),
        );
        let workspace = self.workspace.read(cx);
        let article = workspace.article_of(self.entry.as_ref(), cx)?;
        let chip = article.number.and_then(|number| {
            let project = &workspace.projects[workspace.article_at(&article.path)?.0];
            let name = reference::project_name(&project.path)?;
            let within = Some(reference::Within::Span(lines));
            Some(composer::mention_chip(
                &reference::Reference::entry(None, number, within).to_string(),
                &reference::Reference::entry(Some(&name), number, within).link(),
            ))
        });
        let quoted = match chip {
            Some(chip) => format!("{passage}\n\n{chip}"),
            None => passage.to_owned(),
        };
        Some((quoted, passage.to_owned()))
    }

    /// Send the selection to the article's session with `question` under it.
    /// The selection collapses: the bar over it has done its work.
    fn send_about(&mut self, question: &str, cx: &mut Context<Self>) {
        let (Some(path), Some((quoted, _))) = (self.thread_article(cx), self.quoted_selection(cx))
        else {
            return;
        };
        let text = format!("{}\n\n{question}", composer::blockquote(&quoted));
        self.workspace.update(cx, |workspace, cx| {
            if let Some(id) = workspace.article_session(&path, cx) {
                workspace.send(id, text, cx);
            }
        });
        if let Some(editor) = self.open_editor(cx) {
            editor.update(cx, |editor, cx| {
                let (_, end) = editor.selection().ordered();
                editor.select(markdown::Selection::at(end), cx);
            });
        }
    }

    /// Ask for the selection to be explained, under the passage's opening:
    /// `Explain “…”`.
    fn explain(&mut self, cx: &mut Context<Self>) {
        let Some((_, passage)) = self.quoted_selection(cx) else {
            return;
        };
        let line = passage.lines().next().unwrap_or_default().trim();
        let mut shown: String = line.chars().take(QUESTION_QUOTE).collect();
        if shown.len() < passage.len() {
            shown.push('…');
        }
        self.send_about(&format!("Explain “{shown}”"), cx);
    }

    /// Put the question field up in the bar's place.
    fn open_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ribbon.question.update(cx, |field, cx| field.clear(cx));
        self.ribbon.asking = true;
        window.focus(&self.ribbon.question.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// Send what was typed about the selection, and give the caret back to
    /// the document. An empty field sends nothing.
    fn confirm_ask(&mut self, _: &ConfirmAsk, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.ribbon.asking) {
            return;
        }
        let question = self.ribbon.question.read(cx).content().trim().to_string();
        if !question.is_empty() {
            self.send_about(&question, cx);
        }
        if let Some(editor) = self.open_editor(cx) {
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Drop the question field and leave the run as it was.
    fn dismiss_ask(&mut self, _: &DismissAsk, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.ribbon.asking) {
            return;
        }
        if let Some(editor) = self.open_editor(cx) {
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// The open document's editing surface, or nothing where no article is
    /// open — every one of the bar's acts asks for it.
    fn open_editor(&self, cx: &App) -> Option<Entity<Editor>> {
        self.article_shown(cx)
            .and_then(|view| view.read(cx).editor().cloned())
    }

    /// The article the pane shows, while article threads are on: what its
    /// composer sends to.
    pub(crate) fn thread_article(&self, cx: &App) -> Option<std::path::PathBuf> {
        let workspace = self.workspace.read(cx);
        if !workspace.settings.features.threads() {
            return None;
        }
        if self.entry.is_none() && self.kind != artifact::kind::Kind::Article {
            return None;
        }
        Some(workspace.article_of(self.entry.as_ref(), cx)?.path.clone())
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The bar, when there is a run of text for it to be about. Only the
    /// focused pane draws one: the run is the focused editor's.
    pub(crate) fn ribbon_bar(
        &self,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !focused {
            return None;
        }
        // Held down: the run is still being chosen.
        if self.ribbon.selecting {
            return None;
        }
        let view = self.article_shown(cx)?;
        let editor = view.read(cx).editor()?.clone();
        let port = view.read(cx).scroll.bounds();
        let (formatting, head) = {
            let held = editor.read(cx);
            (held.formatting(), held.selection_bounds()?)
        };
        // In the source the markup is already spelled out and the editor
        // refuses every toggle, so there is nothing here to light or to do.
        if formatting.mode != Mode::Blocks {
            return None;
        }
        // Writing a link, asking and the turn menu take the focus off the
        // document, and the bar is what is holding what it went to.
        let linking = self.ribbon.linking.is_some();
        let asking = self.ribbon.asking;
        let turning = self.menu.key() == Some(&TabMenu::Turn);
        if !linking && !asking && !turning && !editor.focus_handle(cx).contains_focused(window, cx)
        {
            return None;
        }
        let at = perch(port, head)?;

        let threads = self
            .thread_article(cx)
            .is_some_and(|path| self.workspace.read(cx).thread_reachable(&path, cx));
        let theme = Theme::of(cx).clone();
        let card = popover::popover_card(&theme)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .on_action(cx.listener(Self::confirm_link))
            .on_action(cx.listener(Self::dismiss_link))
            .on_action(cx.listener(Self::confirm_ask))
            .on_action(cx.listener(Self::dismiss_ask));
        let card = match (linking, asking && threads) {
            // A press elsewhere files the URL typed, the way the rename field
            // has it, and drops a question.
            (true, _) => card.child(self.inline_field(
                self.ribbon.field.clone(),
                &theme,
                cx,
                |this, window, cx| this.confirm_link(&ConfirmLink, window, cx),
            )),
            (false, true) => card.child(self.inline_field(
                self.ribbon.question.clone(),
                &theme,
                cx,
                |this, window, cx| this.dismiss_ask(&DismissAsk, window, cx),
            )),
            (false, false) => card
                .when(threads, |card| {
                    card.child(action(
                        "Ask",
                        icons::glyph::MessageCircleQuestionMark,
                        &theme,
                        cx,
                        |this: &mut Self, window, cx| this.open_ask(window, cx),
                    ))
                    .child(action(
                        "Explain",
                        icons::glyph::Lightbulb,
                        &theme,
                        cx,
                        |this: &mut Self, _, cx| this.explain(cx),
                    ))
                    .child(divider(&theme))
                })
                .child(self.turn_trigger(&formatting, &theme, window, cx))
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
            .tooltip(move |window, cx| match key.clone() {
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

    /// A field in the bar's own box rather than a panel under it: the bar is
    /// already anchored to the words it is about. `out` is what a press
    /// anywhere else does.
    fn inline_field(
        &self,
        field: Entity<TextField>,
        theme: &Theme,
        cx: &Context<Self>,
        out: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        div()
            .w(px(FIELD_WIDTH))
            .h(px(BUTTON))
            .px(px(6.))
            .flex()
            .items_center()
            .text_style(TextStyle::Callout)
            .text_color(theme.text)
            .on_mouse_down_out(cx.listener(move |this, _, window, cx| out(this, window, cx)))
            .child(field)
            .into_any_element()
    }

    /// What the caret's block is called, and the menu of what it could be.
    fn turn_trigger(
        &self,
        formatting: &Formatting,
        theme: &Theme,
        window: &mut Window,
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
            .on_click(cx.listener({
                let current = current.clone();
                move |this, _, window, cx| {
                    cx.stop_propagation();
                    let current = current.clone();
                    ContextMenu::toggle(
                        this,
                        |this| &mut this.menu,
                        TabMenu::Turn,
                        Place::Below,
                        move |_, _, _| menu::chooser(Self::turn_rows(current)),
                        window,
                        cx,
                    );
                }
            }))
            .children(self.menu.render_for(
                &TabMenu::Turn,
                "ribbon-turn-card",
                |this| &mut this.menu,
                window,
                cx,
            ));
        ContextMenu::trigger(button, |this: &mut Self| &mut this.menu, TabMenu::Turn, cx)
    }

    /// Every block the run could be turned into — the editor's own vocabulary,
    /// which is what the slash menu and the gutter handle both offer.
    fn turn_rows(current: Option<SharedString>) -> Vec<(Item, menu::Act<Self>)> {
        editor::turns()
            .into_iter()
            .map(|(label, kind)| {
                let checked = current.as_ref() == Some(&label);
                menu::row::<Self>(Item::action(label).checked(checked), move |this, _, cx| {
                    this.turn_blocks(kind.clone(), cx);
                })
            })
            .collect()
    }
}

impl Cydonia {
    /// Note on the focused pane that the pointer is choosing a run — see
    /// [`Tab::set_selecting`].
    pub(crate) fn set_selecting(&mut self, selecting: bool, cx: &mut Context<Self>) {
        self.leaf()
            .clone()
            .update(cx, |pane, cx| pane.set_selecting(selecting, cx));
    }

    /// Put the focused pane's bar back to rest — see [`Tab::rest_ribbon`].
    pub(crate) fn rest_ribbon(&mut self, cx: &mut Context<Self>) {
        self.leaf()
            .clone()
            .update(cx, |pane, cx| pane.rest_ribbon(cx));
    }
}

/// One of a selection bar's actions, as a word beside its mark — the
/// ribbon's and the transcript's selection bar's.
pub(crate) fn action<V: 'static>(
    label: &'static str,
    glyph: &'static [u8],
    theme: &Theme,
    cx: &Context<V>,
    act: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> AnyElement {
    theme
        .ghost(SharedString::from(format!("selection-{label}")))
        .px(px(8.))
        .py(px(4.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.))
        .text_style(TextStyle::Callout)
        .text_color(theme.text)
        .child(
            icons::icon(glyph)
                .size(px(13.))
                .flex_none()
                .text_color(theme.text_muted),
        )
        .child(label)
        .on_click(cx.listener(move |this, _, window, cx| act(this, window, cx)))
        .into_any_element()
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

/// What the URL field files for `typed`. A bare host, `example.com`, gets
/// `https://`; a relative path is kept where it names a file under `base`.
fn typed_link(typed: &str, base: Option<&Path>) -> String {
    let kept = typed.is_empty()
        || url::Url::parse(typed).is_ok()
        || ["#", "/", "./", "../", "~/"]
            .iter()
            .any(|start| typed.starts_with(start))
        || base
            .and_then(crate::model::file_url::from_dir)
            .and_then(|base| base.join(typed).ok())
            .and_then(|url| crate::model::file_url::to_path(&url))
            .is_some_and(|path| path.exists());
    let host = typed.split(['/', '?', '#']).next().unwrap_or_default();
    match kept || !host.contains('.') || typed.contains(char::is_whitespace) {
        true => typed.to_owned(),
        false => format!("https://{typed}"),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ribbon_links.rs"]
mod tests;
