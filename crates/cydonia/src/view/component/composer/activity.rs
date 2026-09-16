use super::*;
use crate::model::session::{ChatSession, Connection};
use artifact::session::chat::{ChatItem, ToolStatus};
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq)]
pub struct Activity {
    label: String,
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
        let label = if disconnected {
            "Connection lost".into()
        } else if permission.is_some() {
            "Needs your approval".into()
        } else if connecting {
            "Connecting…".into()
        } else if running.len() == 1 {
            format!(
                "Running {}",
                running[0]
                    .1
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        } else if !running.is_empty() {
            format!("Running {} tools", running.len())
        } else {
            "Working…".into()
        };
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
            label,
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
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
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
        let label = if quiet >= Duration::from_secs(30) && activity.label == "Working…" {
            "Waiting for an update…"
        } else {
            &activity.label
        };
        let mut timing = elapsed(activity.since.elapsed());
        if quiet >= Duration::from_secs(30)
            && !activity.disconnected
            && activity.permission.is_none()
        {
            timing.push_str(&format!(" · last activity {} ago", elapsed(quiet)));
        }
        let open = self.activity_open;
        let reconnect = activity.reconnectable;
        Some(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .mb(px(6.))
                .child(
                    div()
                        .id("composer-activity")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .min_w_0()
                        .px(px(12.))
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
                        .child(if activity.disconnected || activity.permission.is_some() {
                            "!"
                        } else {
                            "◌"
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(label.to_owned()),
                        )
                        .when(!activity.disconnected, |row| {
                            row.child(div().flex_none().whitespace_nowrap().child(timing))
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
                                |details| details.child("Waiting for the agent’s next update."),
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
