//! The backups section: what migrations kept of an earlier release's files —
//! see [`artifact::backup`] — with a way to look at them, delete them, or roll
//! back to the release they belong to.
//!
//! Listed only while a backup exists.

use crate::view::settings::{self, SettingsWindow};
use artifact::backup::Backup;
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Scaffolding},
    },
};

/// What a row is asking to have agreed to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Ask {
    Delete,
    RollBack,
}

impl SettingsWindow {
    /// Read the backups again; what is on disk may have changed.
    pub(super) fn load_backups(&mut self) {
        self.backups = artifact::backup::list();
        self.backup_ask = None;
    }

    pub(super) fn backups_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let mut group = theme.group_box();
        for (ix, backup) in self.backups.iter().enumerate() {
            group = group.child(self.backup_row(ix, backup, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(settings::GROUP_GAP))
            .child(group)
            .children(self.rollback_line(cx))
            .into_any_element()
    }

    fn backup_row(&self, ix: usize, backup: &Backup, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let release = backup.release();
        let asking = self
            .backup_ask
            .as_ref()
            .filter(|(version, _)| *version == backup.version)
            .map(|(_, ask)| *ask);
        let mut meta = vec![bytes(backup.bytes)];
        if let Some(taken) = backup.taken {
            let taken: chrono::DateTime<chrono::Local> = taken.into();
            meta.insert(0, taken.format("%Y-%m-%d %H:%M").to_string());
        }
        match backup.projects.len() {
            0 => {}
            1 => meta.push("1 project".into()),
            n => meta.push(format!("{n} projects")),
        }
        if !backup.config.is_empty() {
            meta.push("settings".into());
        }
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(theme.row_title(format!("cydonia {release}")))
            .child(
                div()
                    .mt(px(4.))
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text_muted)
                    .child(match asking {
                        Some(Ask::Delete) => "Delete this backup? Rolling back to this release \
                                              will no longer be possible."
                            .to_owned(),
                        Some(Ask::RollBack) => format!(
                            "Install cydonia {release} and put these files back? Changes \
                             made since the migration are lost. cydonia quits once it is \
                             downloaded."
                        ),
                        None => meta.join(" · "),
                    }),
            );
        let version = backup.version.clone();
        let controls = match asking {
            Some(ask) => {
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .gap(px(6.))
                    .child(
                        theme
                            .button("Cancel", ButtonStyle::Ghost, None)
                            .id(("backup-cancel", ix))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.backup_ask = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        theme
                            .button(
                                match ask {
                                    Ask::Delete => "Delete",
                                    Ask::RollBack => "Roll back",
                                },
                                ButtonStyle::Destructive,
                                None,
                            )
                            .id(("backup-confirm", ix))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.answer_backup(&version, ask, cx)
                            })),
                    )
            }
            None => {
                let dir = backup.dir.clone();
                let (delete, roll) = (backup.version.clone(), backup.version.clone());
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .gap(px(2.))
                    .child(
                        theme
                            .icon_button(icons::files::FolderOpen, ButtonStyle::Ghost, None)
                            .id(("backup-reveal", ix))
                            .tooltip(|window, cx| Tooltip::text("Reveal in Finder", window, cx))
                            .on_click(move |_, _, cx| {
                                let dir = dir.clone();
                                cx.background_executor()
                                    .spawn(async move {
                                        let _ = crate::view::component::file::external::show(&dir);
                                    })
                                    .detach();
                            }),
                    )
                    .children(self.can_roll_back(cx).then(|| {
                        theme
                            .icon_button(icons::files::ArchiveRestore, ButtonStyle::Ghost, None)
                            .id(("backup-roll-back", ix))
                            .tooltip(|window, cx| Tooltip::text("Roll back", window, cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.backup_ask = Some((roll.clone(), Ask::RollBack));
                                cx.notify();
                            }))
                    }))
                    .child(
                        theme
                            .icon_button(icons::files::Trash, ButtonStyle::Ghost, None)
                            .id(("backup-delete", ix))
                            .tooltip(|window, cx| Tooltip::text("Delete", window, cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.backup_ask = Some((delete.clone(), Ask::Delete));
                                cx.notify();
                            })),
                    )
            }
        };
        theme
            .card_row(ix == 0)
            .child(text)
            .child(controls)
            .into_any_element()
    }

    fn answer_backup(&mut self, version: &str, ask: Ask, cx: &mut Context<Self>) {
        self.backup_ask = None;
        match ask {
            Ask::Delete => {
                let _ = artifact::backup::remove(version);
                self.load_backups();
                // The section is listed only while a backup exists.
                if self.backups.is_empty() {
                    self.show(super::Section::General, cx);
                }
            }
            Ask::RollBack => self.roll_back(version, cx),
        }
        cx.notify();
    }

    #[cfg(feature = "desktop")]
    fn can_roll_back(&self, cx: &Context<Self>) -> bool {
        crate::model::update::supported(cx)
    }

    #[cfg(not(feature = "desktop"))]
    fn can_roll_back(&self, _: &Context<Self>) -> bool {
        false
    }

    #[cfg(feature = "desktop")]
    fn roll_back(&mut self, version: &str, cx: &mut Context<Self>) {
        let Some(backup) = self.backups.iter().find(|backup| backup.version == version) else {
            return;
        };
        let release = backup.release();
        let version = version.to_owned();
        if let Some(updater) = crate::model::update::of(cx) {
            updater.update(cx, |updater, cx| updater.roll_back(release, version, cx));
        }
    }

    #[cfg(not(feature = "desktop"))]
    fn roll_back(&mut self, _: &str, _: &mut Context<Self>) {}

    /// How a rollback is going, while one is.
    #[cfg(feature = "desktop")]
    fn rollback_line(&self, cx: &Context<Self>) -> Option<AnyElement> {
        use crate::model::update::Rollback;
        let theme = Theme::of(cx);
        let updater = crate::model::update::of(cx)?;
        let (text, color): (bezel::gpui::SharedString, _) = match updater.read(cx).rollback()? {
            Rollback::Downloading(release) => (
                format!("Downloading cydonia {release}…").into(),
                theme.text_muted,
            ),
            Rollback::Failed(why) => (format!("Rolling back failed: {why}").into(), theme.danger),
        };
        Some(
            div()
                .text_style(TextStyle::Callout)
                .text_color(color)
                .child(text)
                .into_any_element(),
        )
    }

    #[cfg(not(feature = "desktop"))]
    fn rollback_line(&self, _: &Context<Self>) -> Option<AnyElement> {
        None
    }
}

fn bytes(n: u64) -> String {
    match n {
        0..1_000 => format!("{n} B"),
        1_000..1_000_000 => format!("{:.1} KB", n as f64 / 1e3),
        _ => format!("{:.1} MB", n as f64 / 1e6),
    }
}
