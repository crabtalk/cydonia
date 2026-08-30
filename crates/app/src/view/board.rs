//! The board pane: lanes of cards, and the one field that writes them.

use crate::{
    model::{
        board::{self, Card, Spot},
        session::ChatSession,
    },
    view::root::{Cydonia, Pane},
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, Entity, Focusable as _, FontWeight, KeyBinding,
        SharedString, Stateful, Window, actions, div, prelude::*, px,
    },
    motion::Painter,
    theme::Theme,
    ui::{
        icons,
        input::{self, Shape, TextField},
        loaders,
    },
};

actions!(cydonia_board, [CommitCard, DismissCard]);

/// Claimed on top of `TextField`, so `enter` files the card here and stays a
/// newline in every other multi-line field.
const KEY_CONTEXT: &str = "CydoniaCard";

const COLUMN_WIDTH: f32 = 272.;

pub fn init(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", CommitCard, ctx),
        KeyBinding::new("shift-enter", input::InsertNewline, ctx),
        KeyBinding::new("escape", DismissCard, ctx),
    ]);
}

/// The board's one text field — whichever card is being written or rewritten.
/// Only ever one is open, and a field per card would mint an entity for every
/// row on the board.
pub fn field(cx: &mut App) -> Entity<TextField> {
    cx.new(|cx| {
        TextField::new(cx)
            .with_shape(Shape::Grow { min: 2, max: 8 })
            .with_key_context(KEY_CONTEXT)
            .with_placeholder("what needs doing…")
    })
}

/// What the field is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Editing {
    /// A card being written, to land at the end of this column.
    New(usize),
    /// A card being rewritten.
    Card(Spot),
}

impl Cydonia {
    // ── mutations ────────────────────────────────────────────────

    pub fn show_pane(&mut self, pane: Pane, cx: &mut Context<Self>) {
        self.commit(cx);
        self.pane = pane;
        cx.notify();
    }

    /// Point the field at `at`, filing whatever was already open first — so
    /// clicking straight from one card to another never drops an edit.
    fn edit(&mut self, at: Editing, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        let text = match at {
            Editing::New(_) => String::new(),
            Editing::Card(spot) => self
                .workspace
                .read(cx)
                .active_project()
                .and_then(|project| project.board.card(spot))
                .map(|card| card.text.clone())
                .unwrap_or_default(),
        };
        self.card_field
            .update(cx, |field, cx| field.set_content(text, cx));
        self.editing = Some(at);
        window.focus(&self.card_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// File whatever is open before leaving it: the card being written, and the
    /// name the article being written has settled on. Every way out of a pane
    /// goes through here.
    ///
    /// An empty card is not a card — committing nothing drops it rather than
    /// leaving a blank on the board.
    pub(crate) fn commit(&mut self, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.rename_article(cx));
        let Some(at) = self.editing.take() else {
            return;
        };
        let text = self.card_field.read(cx).content().trim().to_owned();
        self.card_field.update(cx, |field, cx| field.clear(cx));
        self.workspace.update(cx, |workspace, cx| {
            let Some(board) = workspace.active_board_mut() else {
                return;
            };
            match at {
                Editing::New(ix) => {
                    if !text.is_empty()
                        && let Some(column) = board.columns.get_mut(ix)
                    {
                        column.cards.push(Card::new(text));
                    }
                }
                Editing::Card(spot) => {
                    if text.is_empty() {
                        board.take(spot);
                    } else if let Some(card) = board.card_mut(spot) {
                        card.text = text;
                    }
                }
            }
            board::save(&workspace.projects);
            cx.notify();
        });
    }

    fn commit_card(&mut self, _: &CommitCard, _: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        cx.notify();
    }

    /// Escape abandons the edit — the one way to leave a card as it was.
    fn dismiss_card(&mut self, _: &DismissCard, _: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.card_field.update(cx, |field, cx| field.clear(cx));
        cx.notify();
    }

    /// Carry a card one column over, its session with it.
    fn move_card(&mut self, at: Spot, delta: isize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            let Some(board) = workspace.active_board_mut() else {
                return;
            };
            let Some(to) = at.column.checked_add_signed(delta) else {
                return;
            };
            if to >= board.columns.len() {
                return;
            }
            if let Some(card) = board.take(at) {
                board.columns[to].cards.push(card);
            }
            board::save(&workspace.projects);
            cx.notify();
        });
        cx.notify();
    }

    fn delete_card(&mut self, at: Spot, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            if let Some(board) = workspace.active_board_mut() {
                board.take(at);
            }
            board::save(&workspace.projects);
            cx.notify();
        });
        cx.notify();
    }

    /// Hand the card to an agent: a session of its own, opened in the project
    /// with the card's text as its first prompt.
    ///
    /// A session of its own rather than the one in front, so the card's dot
    /// reports its own run — and so dispatching a second card doesn't queue
    /// behind the first. The board stays up: the card goes live where you are
    /// looking, and clicking it is what follows the work into the transcript.
    fn dispatch_card(&mut self, at: Spot, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            let text = workspace
                .active_project()
                .and_then(|project| project.board.card(at))
                .map(|card| card.text.clone());
            let (Some(text), Some(entry)) = (text, workspace.preferred_agent()) else {
                return;
            };
            let id = workspace.new_session(entry, Some(text), cx);
            if let Some(card) = workspace
                .active_board_mut()
                .and_then(|board| board.card_mut(at))
            {
                card.session = id;
            }
        });
        cx.notify();
    }

    /// The session a card was dispatched to, while it is still open — a card
    /// whose session has been closed is a card you can run again.
    fn card_session<'a>(&self, card: &Card, cx: &'a App) -> Option<&'a ChatSession> {
        card.session
            .and_then(|id| self.workspace.read(cx).session(id))
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The lanes. Same frame as [`Cydonia::transcript`]: the body of the
    /// content card, with the composer stack still pinned under it.
    pub fn board(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(project) = self.workspace.read(cx).active_project() else {
            return div().flex_1().into_any_element();
        };
        let count = project.board.columns.len();
        let columns: Vec<AnyElement> = (0..count).map(|ix| self.column(ix, cx)).collect();
        div()
            .flex_1()
            .min_h_0()
            .on_action(cx.listener(Self::commit_card))
            .on_action(cx.listener(Self::dismiss_card))
            .child(
                div()
                    .id("board")
                    .size_full()
                    .overflow_x_scroll()
                    .flex()
                    .flex_row()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(16.))
                    .children(columns),
            )
            .into_any_element()
    }

    fn column(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some((name, count)) = self
            .workspace
            .read(cx)
            .active_project()
            .and_then(|project| project.board.columns.get(ix))
            .map(|column| (column.name.clone(), column.cards.len()))
        else {
            return div().into_any_element();
        };
        let mut cards: Vec<AnyElement> = (0..count)
            .map(|n| self.card(Spot::new(ix, n), cx))
            .collect();
        if self.editing == Some(Editing::New(ix)) {
            cards.push(self.card_editor(cx));
        }

        div()
            .flex_none()
            .w(px(COLUMN_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex_none()
                    .px(px(4.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_muted)
                            .child(name),
                    )
                    .child(div().text_color(theme.text_faint).child(count.to_string())),
            )
            .child(
                div()
                    .id(("column", ix))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(cards)
                    .child(
                        div()
                            .id(("add-card", ix))
                            .flex_none()
                            .px(px(8.))
                            .py(px(6.))
                            .rounded(px(Theme::control_radius()))
                            .cursor_pointer()
                            .hover(|el| el.bg(theme.glass_hover()))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                icons::icon(icons::PLUS)
                                    .size(px(12.))
                                    .text_color(theme.text_faint),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(theme.text_faint)
                                    .child("Add a card"),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.edit(Editing::New(ix), window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn card(&self, at: Spot, cx: &mut Context<Self>) -> AnyElement {
        if self.editing == Some(Editing::Card(at)) {
            return self.card_editor(cx);
        }
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let Some(card) = self
            .workspace
            .read(cx)
            .active_project()
            .and_then(|project| project.board.card(at))
        else {
            return div().into_any_element();
        };
        let text = card.text.clone();
        let chat = self.card_session(card, cx);
        let live = chat.map(|chat| chat.id);
        // The same reading as the rail's session row: the card and the row are
        // reporting the same process.
        let running = chat.is_some_and(|chat| chat.streaming);
        let orb = running.then(|| {
            loaders::orb(
                loaders::Orb::Cluster,
                SharedString::from(format!("card-orb-{}-{}", at.column, at.card)),
                12.,
                &theme,
                painter,
                cx,
            )
            .into_any_element()
        });
        div()
            .id(SharedString::from(format!(
                "card-{}-{}",
                at.column, at.card
            )))
            .group("card")
            .flex_none()
            .p(px(10.))
            .rounded(px(Theme::control_radius()))
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface_raised)
            .cursor_pointer()
            .hover(|el| el.border_color(theme.text_faint))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .max_h(px(140.))
                    .overflow_hidden()
                    .text_size(px(12.5))
                    .text_color(theme.text)
                    .child(text),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(2.))
                    // On show, not behind a hover — a card's run is what you
                    // look at the board to see, and hiding it would mean
                    // hunting for the one that is working.
                    .children(orb)
                    .child(div().flex_1())
                    .child(
                        div()
                            .invisible()
                            .group_hover("card", |el| el.visible())
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(2.))
                            .children((at.column > 0).then(|| {
                                self.card_action("left", at, icons::ALT_ARROW_LEFT, cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.move_card(at, -1, cx);
                                    }))
                            }))
                            .children((!self.last_column(at, cx)).then(|| {
                                self.card_action("right", at, icons::ALT_ARROW_RIGHT, cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.move_card(at, 1, cx);
                                    }))
                            }))
                            .child(match live {
                                Some(id) => self
                                    .card_action("open", at, icons::CHAT_ROUND_LINE, cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.select_session(id, cx);
                                        this.show_pane(Pane::Chat, cx);
                                    })),
                                None => self.card_action("run", at, icons::PLAY, cx).on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.dispatch_card(at, cx);
                                    }),
                                ),
                            })
                            .child(
                                self.card_action("delete", at, icons::TRASH_BIN_MINIMALISTIC, cx)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.delete_card(at, cx);
                                    })),
                            ),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.edit(Editing::Card(at), window, cx);
            }))
            .into_any_element()
    }

    /// One glyph on a card's hover row.
    fn card_action(
        &self,
        name: &'static str,
        at: Spot,
        glyph: &'static str,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        ui::ghost(
            &theme,
            SharedString::from(format!("card-{name}-{}-{}", at.column, at.card)),
        )
        .p(px(3.))
        .child(
            icons::icon(glyph)
                .size(px(12.))
                .text_color(theme.text_faint),
        )
    }

    /// Whether `at` sits in the rightmost column — nowhere further to carry it.
    fn last_column(&self, at: Spot, cx: &App) -> bool {
        self.workspace
            .read(cx)
            .active_project()
            .is_none_or(|project| at.column + 1 >= project.board.columns.len())
    }

    fn card_editor(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex_none()
            .p(px(8.))
            .rounded(px(Theme::control_radius()))
            .border_1()
            .border_color(theme.accent)
            .bg(theme.surface_raised)
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(self.card_field.clone())
            .child(
                div()
                    .text_size(px(11.))
                    .font_family(theme.font_mono.clone())
                    .text_color(theme.text_faint)
                    .child("enter file · esc cancel"),
            )
            .into_any_element()
    }
}
