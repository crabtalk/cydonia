//! ▶ on a card and on a lane: a menu of the sessions to send the cards to,
//! and a new one. What is sent is the cards' chips, and the session is written
//! down on each card as the one that last held it — see
//! [`artifact::board::Card::session`].

use super::{BoardMenu, BoardView, card_action};
use crate::{
    model::session::ChatSession,
    view::{
        component::composer,
        kit::{
            icon::session_icon,
            menu,
            new::{self, New},
        },
    },
};
use artifact::reference::{self, Reference, Target};
use bezel::{
    gpui::{AnyElement, App, Context, SharedString, Window, prelude::*},
    ui::{icons, menu::Item},
};

/// The most sessions the menu lists besides the one that last held the card.
const RECENT: usize = 8;

/// What a ▶ sends.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Sent {
    /// A card, by id.
    Card(String),
    /// The cards of a lane, by column id.
    Lane(String),
}

/// Where the cards go.
#[derive(Clone, Copy)]
enum To {
    Session(u64),
    /// A new session, on the agent [`New::Session`] names.
    New(Option<usize>),
}

impl BoardView {
    /// ▶ on card `id` of the board `on`, with its menu while it is up. `name`
    /// keeps the lane's and the list's apart.
    pub(super) fn send_card_button(
        &self,
        name: &'static str,
        on: &str,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sent = Sent::Card(id.to_owned());
        let button = card_action(name, id, icons::multimedia::Play, cx).relative();
        self.opens(
            button,
            BoardMenu::Send(sent.clone()),
            self.send_rows(on, sent.clone()),
            cx,
        )
        .children(self.menu_here(
            &BoardMenu::Send(sent),
            SharedString::from(format!("send-menu-card-{id}")),
            window,
            cx,
        ))
        .into_any_element()
    }

    /// ▶ on lane `column` of the board `on`, revealed by the hover group
    /// `group`. Nothing while sessions are off or no card in it can be sent.
    pub(super) fn send_lane_button(
        &self,
        on: &str,
        column: &str,
        group: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sent = Sent::Lane(column.to_owned());
        if !self.workspace.read(cx).settings.features.sessions
            || self.sendable(on, &sent, cx).is_empty()
        {
            return None;
        }
        let key = BoardMenu::Send(sent.clone());
        let button = menu::button(
            SharedString::from(format!("lane-send-{column}")),
            icons::multimedia::Play,
            menu::Reveal::Group(group),
            self.menu.key() == Some(&key),
            cx,
        );
        Some(
            self.opens(button, key.clone(), self.send_rows(on, sent), cx)
                .children(self.menu_here(
                    &key,
                    SharedString::from(format!("send-menu-lane-{column}")),
                    window,
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// The cards `sent` names on the board `on` that can be sent: neither
    /// busy nor waiting on another card.
    pub(super) fn sendable(&self, on: &str, sent: &Sent, cx: &App) -> Vec<String> {
        let workspace = self.workspace.read(cx);
        let Some(board) = workspace.board_at(on, cx) else {
            return Vec::new();
        };
        let cards: Vec<_> = match sent {
            Sent::Card(id) => board.card(id).into_iter().collect(),
            Sent::Lane(id) => board
                .column(id)
                .map(|column| column.cards.iter().collect())
                .unwrap_or_default(),
        };
        cards
            .into_iter()
            .filter(|card| {
                !card.busy
                    && workspace
                        .blocking_boards(&self.board.project, card, cx)
                        .next()
                        .is_none()
            })
            .map(|card| card.id.clone())
            .collect()
    }

    /// The menu's rows: the session that last held the card, the project's
    /// sessions most recently touched, then a new session.
    fn send_rows(&self, on: &str, sent: Sent) -> menu::Rows<Self> {
        let on = on.to_owned();
        Box::new(move |this, _, cx| {
            let workspace = this.workspace.read(cx);
            let held = match &sent {
                Sent::Card(card) => workspace
                    .board_at(&on, cx)
                    .and_then(|board| board.card(card))
                    .and_then(|card| card.session.as_deref())
                    .and_then(|record| workspace.session_by_record(record, cx)),
                Sent::Lane(_) => None,
            };
            let row = |chat: &ChatSession| {
                let (on, sent, to) = (on.clone(), sent.clone(), To::Session(chat.id));
                let icon = session_icon(chat, workspace);
                menu::row::<Self>(
                    Item::action(chat.label()).with_icon(icon),
                    move |this, _, cx| this.send(&on, &sent, to, cx),
                )
            };
            let recent = workspace
                .project_at(&this.board.project)
                .and_then(|ix| workspace.projects.get(ix))
                .map(|project| {
                    project
                        .recent_sessions(cx)
                        .into_iter()
                        .filter(|chat| Some(chat.id) != held.map(|held| held.id))
                        .take(RECENT)
                        .map(row)
                        .collect()
                })
                .unwrap_or_default();
            let offered = new::Offered {
                kinds: Vec::new(),
                ..new::offered(workspace)
            };
            let fresh = new::rows(offered, |kind| {
                let (on, sent) = (on.clone(), sent.clone());
                Box::new(move |this: &mut Self, _, cx| {
                    if let New::Session(at) = kind {
                        this.send(&on, &sent, To::New(at), cx);
                    }
                })
            });
            menu::sections([held.map(row).into_iter().collect(), recent, fresh])
        })
    }

    /// Send what `sent` names on the board `on` to `to`, and write `to` down
    /// on each card sent.
    fn send(&mut self, on: &str, sent: &Sent, to: To, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        let cards = self.sendable(on, sent, cx);
        let Some(text) = self.sent_text(on, sent, &cards, cx) else {
            return;
        };
        let (on, project) = (on.to_owned(), self.board.project.clone());
        self.workspace.update(cx, |workspace, cx| {
            let id = match to {
                To::Session(id) => {
                    workspace.send(id, text, cx);
                    id
                }
                To::New(at) => {
                    let (Some(ix), Some(agent)) =
                        (workspace.project_at(&project), workspace.agent_at(at, cx))
                    else {
                        return;
                    };
                    let Some(id) = workspace.new_session_in(ix, agent, Some(text), true, cx) else {
                        return;
                    };
                    id
                }
            };
            let Some(record) = workspace.mint_record(id, cx) else {
                return;
            };
            workspace.write_board(&on, cx, |board| {
                for card in &cards {
                    board.hold_card(card, record.clone());
                }
            });
        });
        cx.notify();
    }

    /// The chips of `cards`, under the lane's name where a lane is sent.
    /// Nothing where none of them has a handle to link.
    fn sent_text(&self, on: &str, sent: &Sent, cards: &[String], cx: &App) -> Option<String> {
        let board = self.workspace.read(cx).board_at(on, cx)?;
        let name = reference::project_name(&self.board.project)?;
        let chips: Vec<String> = cards
            .iter()
            .filter_map(|id| {
                let card = board.card(id)?;
                board.handle_of(card)?;
                let handle = card.handle?;
                let reference = Reference {
                    project: Some(&name),
                    target: Target::Card {
                        key: &board.key,
                        handle,
                    },
                };
                let shown = reference.clone().relative(true).to_string();
                Some(composer::mention_chip(&shown, &reference.link()))
            })
            .collect();
        if chips.is_empty() {
            return None;
        }
        Some(match sent {
            Sent::Card(_) => chips.join(" "),
            Sent::Lane(column) => {
                let listed: Vec<String> = chips.iter().map(|chip| format!("- {chip}")).collect();
                format!("{}\n\n{}", board.column(column)?.name, listed.join("\n"))
            }
        })
    }
}
