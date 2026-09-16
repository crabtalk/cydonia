use super::*;
use crate::model::session::{ChatSession, Connection};
use crate::view::component::transcript;
use artifact::session::chat::{ChatItem, ToolStatus};
use bezel::agent::orbs::OrbState;
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq)]
pub struct Activity {
    orb: OrbState,
    word: &'static str,
    connecting: bool,
    turn_started: Instant,
    since: Instant,
    last_event: Instant,
    tools: Vec<(String, String)>,
    permission: Option<String>,
    disconnected: bool,
    reconnectable: bool,
}

impl Activity {
    pub fn of(chat: &ChatSession) -> Option<Self> {
        let disconnected = matches!(chat.connection, Connection::Lost);
        let connecting = matches!(chat.connection, Connection::Connecting);
        if !chat.streaming && !connecting && !disconnected && chat.permission.is_none() {
            return None;
        }
        let start = chat
            .items
            .iter()
            .rposition(|item| matches!(item, ChatItem::User(_)))
            .unwrap_or(0);
        let running: Vec<_> = chat.items[start..]
            .iter()
            .filter_map(|item| match item {
                ChatItem::Tool {
                    id,
                    label,
                    status: ToolStatus::Running,
                    output,
                    ..
                } if chat.streaming => Some((id, label, output)),
                _ => None,
            })
            .collect();
        let permission = chat.permission.as_ref().map(|prompt| prompt.title.clone());
        let since = if connecting {
            chat.last_activity
        } else {
            running
                .iter()
                .filter_map(|(id, _, _)| chat.tool_started.get(*id).copied())
                .min()
                .or(chat.turn_started)
                .unwrap_or(chat.last_activity)
        };
        Some(Self {
            orb: transcript::orb_of(chat),
            word: transcript::working_word(chat),
            connecting,
            turn_started: chat.turn_started.unwrap_or(chat.last_activity),
            since,
            last_event: chat.last_activity,
            tools: running
                .iter()
                .map(|(_, label, output)| {
                    let tail: String = output
                        .chars()
                        .rev()
                        .take(2000)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    ((*label).clone(), tail)
                })
                .collect(),
            permission,
            disconnected,
            reconnectable: disconnected && chat.resumable(),
        })
    }
}

fn elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!("{:02}m {:02}s", seconds / 60, seconds % 60)
}

impl Composer {
    pub fn set_activity(&mut self, activity: Option<Activity>, cx: &mut Context<Self>) {
        if self.activity == activity {
            return;
        }
        self.activity = activity;
        let ticking = self
            .activity
            .as_ref()
            .is_some_and(|activity| !activity.disconnected);
        if ticking && self.activity_tick.is_none() {
            self.activity_tick = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
        } else if !ticking {
            self.activity_tick = None;
        }
        if self.activity.is_none() {
            self.activity_open = false;
        }
        cx.notify();
    }

    pub(super) fn activity_row(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let activity = self.activity.as_ref()?;
        let quiet = activity.last_event.elapsed();
        let timing = elapsed(activity.since.elapsed());
        let open = self.activity_open;
        let reconnect = activity.reconnectable;
        Some(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .pb(px(6.))
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .id("composer-activity")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .min_w_0()
                        .pr(px(8.))
                        .py(px(4.))
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_muted)
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if reconnect {
                                cx.emit(ComposerEvent::Reconnect);
                            } else {
                                this.activity_open = !this.activity_open;
                                cx.notify();
                            }
                        }))
                        .child(transcript::orb(
                            activity.orb,
                            activity.turn_started.elapsed(),
                            &self.activity_frame,
                            cx,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .when(
                                    !activity.disconnected && activity.permission.is_none(),
                                    |label| label.child(format!("{}…", activity.word)),
                                )
                                .when(activity.disconnected, |label| {
                                    label.child("Connection lost")
                                })
                                .when(
                                    !activity.disconnected && activity.permission.is_some(),
                                    |label| label.child("Needs your approval"),
                                ),
                        )
                        .when(!activity.disconnected, |row| {
                            row.child(
                                div()
                                    .flex_none()
                                    .font_family(theme.font_mono.clone())
                                    .whitespace_nowrap()
                                    .child(timing),
                            )
                        })
                        .child(if reconnect {
                            "Reconnect"
                        } else if open {
                            "⌄"
                        } else {
                            "›"
                        }),
                )
                .when(open && !activity.disconnected, |panel| {
                    panel.child(
                        div()
                            .id("composer-activity-details")
                            .max_h(px(180.))
                            .overflow_y_scroll()
                            .px(px(12.))
                            .py(px(8.))
                            .rounded(px(8.))
                            .bg(theme.input_bg)
                            .text_style(TextStyle::Caption)
                            .text_color(theme.text_muted)
                            .when_some(activity.permission.clone(), |details, title| {
                                details.child(title)
                            })
                            .when(
                                activity.tools.is_empty() && activity.permission.is_none(),
                                |details| {
                                    details.child(if activity.connecting {
                                        "Connecting to the agent…"
                                    } else {
                                        "Waiting for the agent’s next update."
                                    })
                                },
                            )
                            .when(
                                quiet >= Duration::from_secs(30) && activity.permission.is_none(),
                                |details| {
                                    details.child(
                                        div()
                                            .mb(px(8.))
                                            .child(format!("Last activity {} ago", elapsed(quiet))),
                                    )
                                },
                            )
                            .children(activity.tools.iter().map(|(label, output)| {
                                div()
                                    .mb(px(8.))
                                    .child(
                                        div()
                                            .font_family(theme.font_mono.clone())
                                            .child(label.clone()),
                                    )
                                    .child(
                                        div()
                                            .mt(px(4.))
                                            .font_family(theme.font_mono.clone())
                                            .child(if output.is_empty() {
                                                "No output reported yet.".to_owned()
                                            } else {
                                                output.clone()
                                            }),
                                    )
                            })),
                    )
                })
                .into_any_element(),
        )
    }
}
