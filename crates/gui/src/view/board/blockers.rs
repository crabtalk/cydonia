//! What an open card waits on: the line under its drawer's head, and the
//! picker that changes it — see [`artifact::board::Card::blocked_by`].

use super::BoardView;
use crate::view::kit::{link::mention, text::CardTitle as _};
use bezel::{
    gpui::{
        AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, SharedString,
        Subscription, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        multi_select::{Check, Choice, MultiSelect, MultiSelectEvent},
        popover::{self, Popup},
        widgets::Content as _,
    },
};

/// The open blocker picker, over one card.
pub(crate) struct Picker {
    card: String,
    select: Entity<MultiSelect>,
    /// Each choice's name, with the card it names as the open card writes it.
    named: Vec<(SharedString, String)>,
    _events: Subscription,
}

/// A board view's blocker picker: closed, or open on one card.
#[derive(Default)]
pub(crate) struct Blockers(Popup<Picker>);

impl BoardView {
    /// "Blocked by" and the handles of the cards `card` waits on, which a press
    /// opens the picker from. With none, it shows while the line is hovered.
    pub(super) fn blocked_by_line(&self, card: &str, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let handles = self.blocker_handles(card, cx);
        let empty = handles.is_empty();
        let trigger = div()
            .id("card-blocked-by")
            .group("card-blocked-by")
            .flex_none()
            .px(px(16.))
            .pb(px(8.))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .cursor_pointer()
            .text_style(TextStyle::Caption)
            .text_color(theme.text_faint)
            .child(
                div()
                    .when(empty, |el| {
                        el.invisible()
                            .group_hover("card-blocked-by", |el| el.visible())
                    })
                    .child(match empty {
                        true => "Add a blocker",
                        false => "Blocked by",
                    }),
            )
            .children(
                handles
                    .into_iter()
                    .map(|handle| theme.chip(handle, theme.danger)),
            )
            .on_click(cx.listener({
                let card = card.to_owned();
                move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.toggle_blockers(card.clone(), window, cx);
                }
            }))
            .children(self.blocker_picker(card));
        let this = cx.entity().downgrade();
        let owner = card.to_owned();
        // Capture, as the label picker's trigger does, so a press on the line
        // that opened the picker shuts it rather than reopening it.
        trigger
            .capture_any_mouse_down(move |_, _, cx| {
                let owner = owner.clone();
                this.update(cx, |this, _| {
                    this.blockers
                        .0
                        .note_trigger_press_matching(|picker| picker.card == owner)
                })
                .ok();
            })
            .into_any_element()
    }

    /// What the card `card` waits on, as written, leaving out what names no
    /// card.
    fn blocker_handles(&self, card: &str, cx: &App) -> Vec<String> {
        let workspace = self.workspace.read(cx);
        let Some(board) = workspace.board_entity(&self.board.id) else {
            return Vec::new();
        };
        let Some(card) = board.read(cx).card(card) else {
            return Vec::new();
        };
        card.blocked_by
            .iter()
            .filter(|blocker| {
                workspace
                    .card_board(&self.board.project, blocker, cx)
                    .is_some()
            })
            .cloned()
            .collect()
    }

    fn blocker_picker(&self, card: &str) -> Option<AnyElement> {
        let picker = self.blockers.0.get().filter(|picker| picker.card == card)?;
        Some(popover::anchored_menu_below(
            "blockers-picker",
            picker.select.clone().into_any_element(),
            self.blockers.0.closing_since(),
        ))
    }

    fn toggle_blockers(&mut self, card: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.blockers.0.take_press_was_open() {
            popover::close_popup(self, cx, |this: &mut Self| &mut this.blockers.0);
            return;
        }
        let (choices, named) = self.blocker_choices(&card, cx);
        let theme = Theme::of(cx).clone();
        let select = cx.new(|cx| {
            MultiSelect::new(choices, cx)
                .with_tint(move |_, _| theme.text_muted)
                .with_width(px(280.), px(420.))
        });
        select.update(cx, |select, cx| select.reset(window, cx));
        let events = cx.subscribe_in(
            &select,
            window,
            |this, _, event: &MultiSelectEvent, _, cx| this.blocker_event(event, cx),
        );
        self.blockers.0.open(Picker {
            card,
            select,
            named,
            _events: events,
        });
        cx.notify();
    }

    /// Every other card in the open projects, as the card `card` writes it,
    /// on where it waits on that card.
    fn blocker_choices(&self, card: &str, cx: &App) -> (Vec<Choice>, Vec<(SharedString, String)>) {
        let workspace = self.workspace.read(cx);
        let Some(board) = workspace.board_entity(&self.board.id) else {
            return (Vec::new(), Vec::new());
        };
        let held = board
            .read(cx)
            .card(card)
            .map(|card| card.blocked_by.clone())
            .unwrap_or_default();
        let home = workspace
            .project_at(&self.board.project)
            .and_then(|at| workspace.projects.get(at))
            .map(|project| project.path.as_path());
        let mut choices = Vec::new();
        let mut named = Vec::new();
        for carded in mention::carded(workspace, cx) {
            let own = Some(carded.path) == home;
            if own && carded.card.id == card {
                continue;
            }
            let written = carded.reference().relative(own).to_string();
            let name: SharedString = format!("{written} {}", carded.card.short_title()).into();
            choices.push(Choice {
                name: name.clone(),
                check: match held.contains(&written) {
                    true => Check::On,
                    false => Check::Off,
                },
                count: None,
            });
            named.push((name, written));
        }
        (choices, named)
    }

    fn blocker_event(&mut self, event: &MultiSelectEvent, cx: &mut Context<Self>) {
        let Some(picker) = self.blockers.0.get() else {
            return;
        };
        let card = picker.card.clone();
        let MultiSelectEvent::Toggled { name, on } = event else {
            if *event == MultiSelectEvent::Dismissed {
                popover::close_popup(self, cx, |this: &mut Self| &mut this.blockers.0);
            }
            return;
        };
        let Some(blocker) = picker
            .named
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, written)| written.clone())
        else {
            return;
        };
        let on = *on;
        let board = self.board.id.clone();
        self.workspace.update(cx, |workspace, cx| {
            workspace.write_board(&board, cx, |board| {
                let Some(held) = board.card(&card) else {
                    return false;
                };
                let mut blockers: Vec<String> = held
                    .blocked_by
                    .iter()
                    .filter(|held| **held != blocker)
                    .cloned()
                    .collect();
                if on {
                    blockers.push(blocker.clone());
                }
                board.set_card_blockers(&card, blockers)
            });
        });
        let (choices, named) = self.blocker_choices(&card, cx);
        if let Some(picker) = self.blockers.0.open_mut() {
            picker.named = named;
            picker
                .select
                .update(cx, |select, cx| select.set_choices(choices, cx));
        }
        cx.notify();
    }
}
