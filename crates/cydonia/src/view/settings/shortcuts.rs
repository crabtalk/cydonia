//! The shortcuts section: the chords the app answers to, and the one the
//! system holds for it.
//!
//! Two kinds of key on one page, which is the only reason they are on one
//! page. A command's chord is the keymap's and only means anything while
//! cydonia is in front; Activate is registered with macOS and is the chord
//! that puts cydonia in front from inside something else. The rows read alike;
//! what they fail at does not, so each says so for itself.
//!
//! A chord is recorded rather than typed, so what lands in the file is what
//! the keyboard actually sends. While a row is recording the app answers to
//! nothing at all — see [`SettingsWindow::record`].

use crate::{
    model::{settings::Shortcuts, workspace::Workspace},
    view::{
        hotkey,
        keymap::{self, Command, Menu},
        settings::{self, SettingsWindow, Switch},
    },
};
use bezel::{
    gpui::{
        AnyElement, App, Context, Entity, FocusHandle, KeyDownEvent, SharedString, Subscription,
        Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{Buttons, Scaffolding},
};

/// Which row is taking keys.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Activate,
    Command(Command),
}

impl Target {
    /// The key it is written under in `[shortcuts]`.
    fn key(self) -> &'static str {
        match self {
            Self::Activate => Shortcuts::ACTIVATE,
            Self::Command(command) => command.key(),
        }
    }
}

/// A row waiting for a chord.
pub(super) struct Recording {
    target: Target,
    /// Where the keys arrive. Focused on the way in, and the reason the row
    /// hears anything at all.
    focus: FocusHandle,
    /// Why the last press was not taken, when it was not. Held so the row can
    /// say it rather than appearing to have missed the key.
    refused: Option<SharedString>,
    /// Listening for the focus to go elsewhere, which is a recording nobody is
    /// looking at any more. Held for as long as this is.
    _blur: Subscription,
}

/// What a row shows in place of a chord when it has none.
const UNBOUND: &str = "None";

impl SettingsWindow {
    pub(super) fn shortcuts_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let emacs = self.workspace.read(cx).settings.shortcuts.emacs;
        div()
            .flex()
            .flex_col()
            .gap(px(settings::GROUP_GAP))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(settings::LABEL_GAP))
                    .child(theme.field_label("System"))
                    .child(theme.group_box().child(self.activate_row(cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(settings::LABEL_GAP))
                    .child(theme.field_label("Text editing"))
                    .child(theme.group_box().child(self.switch_row(
                        Switch::new(
                            "emacs-shortcuts",
                            "Emacs word shortcuts",
                            "Option+B/F moves by word; add Shift to select. Option+D deletes the next word.",
                            emacs,
                        ).first(true),
                        cx,
                        move |this, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_emacs_shortcuts(!emacs, cx);
                            });
                            this.restore(cx);
                        },
                    ))),
            )
            .children(Menu::ALL.into_iter().map(|menu| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(settings::LABEL_GAP))
                    .child(theme.field_label(menu.title()))
                    .child(
                        theme.group_box().children(
                            Command::ALL
                                .into_iter()
                                .filter(|command| command.menu() == menu)
                                .enumerate()
                                .map(|(ix, command)| self.command_row(ix, command, cx)),
                        ),
                    )
            }))
            .into_any_element()
    }

    /// The chord macOS holds on the app's behalf.
    ///
    /// Off in a fresh install, and the blurb is where the cost is said: this is
    /// a key taken away from every other app on the desktop, including the
    /// ones already using it. ⌃Space is the obvious ask and the contested one —
    /// macOS gives it to the input-source switch, and a system shortcut wins
    /// silently — so the row reports what registration actually did rather than
    /// what was asked for.
    fn activate_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // Taken out of the workspace before the row is built: what follows
        // needs the context mutably, and a borrow held across it would be one
        // the settings entity was still lending out.
        let held = self
            .workspace
            .read(cx)
            .settings
            .shortcuts
            .activate()
            .map(str::to_owned);
        let refused = hotkey::refused(cx);
        let blurb = match refused.clone() {
            Some(why) => why,
            None if held.is_some() => SharedString::from("Brings cydonia forward from anywhere."),
            None => {
                SharedString::from("Off. A key held across the whole desktop is one to ask for.")
            }
        };
        theme
            .card_row(true)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Activate cydonia"))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(if refused.is_some() {
                                theme.danger
                            } else {
                                theme.text_muted
                            })
                            .child(blurb),
                    ),
            )
            .children(
                held.is_some()
                    .then(|| self.clear_button(Target::Activate, cx)),
            )
            .child(self.chord_button(Target::Activate, held.as_deref(), cx))
            .into_any_element()
    }

    fn command_row(&self, ix: usize, command: Command, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let shortcuts = &self.workspace.read(cx).settings.shortcuts;
        let held = keymap::chord(command, shortcuts).map(str::to_owned);
        // Only where the file has something to say about this command: a row
        // sitting on its default has nothing to put back.
        let moved = shortcuts.get(command.key()).is_some();
        let note = self.row_note(command, cx);
        theme
            .card_row(ix == 0)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(command.title()))
                    .children(note),
            )
            .children(moved.then(|| self.clear_button(Target::Command(command), cx)))
            .child(self.chord_button(Target::Command(command), held.as_deref(), cx))
            .into_any_element()
    }

    /// The line under a command's name, where there is one to draw: what it
    /// would go back to, or who else is already answering to its chord.
    ///
    /// The conflict is worth reporting rather than refusing. Two commands on
    /// one chord is not an error the keymap raises — the later binding wins and
    /// the other goes quiet — and a file edited by hand can hold one whatever
    /// this window does about it.
    fn row_note(&self, command: Command, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let shortcuts = &self.workspace.read(cx).settings.shortcuts;
        let held = keymap::chord(command, shortcuts);
        let clash = held
            .map(|chord| keymap::claimed(chord, command, shortcuts))
            .unwrap_or_default();
        let (copy, colour) = match (clash.first(), shortcuts.get(command.key())) {
            (Some(other), _) => (
                format!("Also {}, which wins it.", other.title()),
                theme.danger,
            ),
            (None, Some(_)) => (
                match command.default() {
                    Some(default) => format!(
                        "Moved from {}.",
                        keymap::glyphs(default).unwrap_or_else(|| default.into())
                    ),
                    None => "Moved from nothing.".to_owned(),
                },
                theme.text_muted,
            ),
            (None, None) => return None,
        };
        Some(
            div()
                .mt(px(4.))
                .text_style(TextStyle::Subheadline)
                .text_color(colour)
                .child(copy)
                .into_any_element(),
        )
    }

    /// The chord itself, which is also the control: pressing it starts
    /// recording, and the next key lands in the file.
    fn chord_button(
        &self,
        target: Target,
        held: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let recording = self
            .recording
            .as_ref()
            .filter(|recording| recording.target == target);
        let label: SharedString = match (recording, held) {
            (Some(recording), _) => recording
                .refused
                .clone()
                .unwrap_or_else(|| "Press a key…".into()),
            (None, Some(chord)) => keymap::glyphs(chord).unwrap_or_else(|| chord.into()),
            (None, None) => UNBOUND.into(),
        };
        let button = theme
            .ghost(SharedString::from(format!("chord-{}", target.key())))
            .px(px(10.))
            .py(px(3.))
            .border_1()
            .text_style(TextStyle::Callout)
            .child(label);
        match recording {
            Some(recording) => button
                .track_focus(&recording.focus)
                .border_color(theme.accent)
                .text_color(theme.accent)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.captured(event, window, cx)
                }))
                .into_any_element(),
            None => button
                .border_color(theme.border)
                .bg(theme.input_bg)
                .text_color(if held.is_some() {
                    theme.text
                } else {
                    theme.text_muted
                })
                .on_click(cx.listener(move |this, _, window, cx| this.record(target, window, cx)))
                .into_any_element(),
        }
    }

    /// Put a row back where it started — its default for a command, off for
    /// Activate. Absent from the file is what "default" is, so this removes the
    /// key rather than writing one.
    fn clear_button(&self, target: Target, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .ghost(SharedString::from(format!("revert-{}", target.key())))
            .px(px(8.))
            .py(px(3.))
            .text_style(TextStyle::Subheadline)
            .text_color(theme.text_muted)
            .child(match target {
                Target::Activate => "Off",
                Target::Command(_) => "Reset",
            })
            .on_click(cx.listener(move |this, _, _, cx| this.put(target, None, cx)))
            .into_any_element()
    }

    /// Start listening for a chord.
    ///
    /// The keymap is emptied and the menu bar taken down for as long as this
    /// lasts, and neither is optional. AppKit runs a menu's key equivalent
    /// before the window is ever offered the keystroke, and a global handler
    /// answers whatever the focused window is — so recording ⌘O over a live
    /// app opens a project picker instead of writing down ⌘O, and ⌘Q quits.
    /// [`Self::put`] and [`Self::stop`] both put it back, as does clicking
    /// away from the row and closing the window outright.
    fn record(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        cx.clear_key_bindings();
        cx.set_menus(Vec::new());
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // Clicking elsewhere is the third way out, and the one that has to be
        // caught rather than offered: keys go to whatever took the focus, and
        // with the keymap emptied that is an app which appears to have died.
        //
        // Deferred, and only for the row that was listening: arming a second
        // row blurs the first, and an immediate stop would undo the `record`
        // that is still running.
        let weak = cx.entity().downgrade();
        let blur = window.on_focus_out(&focus, cx, move |_, _, cx| {
            let weak = weak.clone();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this: &mut Self, cx| {
                    if this.recording.as_ref().map(|held| held.target) == Some(target) {
                        this.stop(cx);
                    }
                });
            });
        });
        self.recording = Some(Recording {
            target,
            focus,
            refused: None,
            _blur: blur,
        });
        cx.notify();
    }

    /// What the keyboard sent while a row was listening.
    ///
    /// `escape` leaves the chord alone and `delete` takes it away — the two
    /// things a recorder must not record, because a person reaching for either
    /// is reaching for the recorder and not for a shortcut.
    fn captured(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(recording) = self.recording.as_mut() else {
            return;
        };
        // A held key repeats, and the first press already answered.
        if event.is_held {
            return;
        }
        let target = recording.target;
        let stroke = &event.keystroke;
        let modified = stroke.modifiers.control
            || stroke.modifiers.alt
            || stroke.modifiers.platform
            || stroke.modifiers.function;
        match stroke.key.as_str() {
            "escape" if !modified => self.stop(cx),
            "delete" | "backspace" if !modified => self.put(target, Some(""), cx),
            // Shift alone is not a modifier a shortcut can be built on: bound
            // app-wide, ⇧N is what happens every time a capital N is typed.
            _ if !modified => {
                recording.refused = Some(
                    match cfg!(target_os = "macos") {
                        true => "Needs ⌘, ⌃ or ⌥",
                        false => "Needs Ctrl, Alt or Super",
                    }
                    .into(),
                );
                cx.notify();
            }
            _ => {
                let chord = stroke.unparse();
                // Only Activate has a second gate: the keymap will take any
                // chord gpui can parse, and the system will not.
                if target == Target::Activate && !hotkey::holdable(&chord) {
                    recording.refused = Some("The system cannot hold that".into());
                    cx.notify();
                    return;
                }
                self.put(target, Some(&chord), cx);
            }
        }
    }

    /// Stop listening and leave the file as it was.
    fn stop(&mut self, cx: &mut Context<Self>) {
        self.recording = None;
        self.restore(cx);
        cx.notify();
    }

    /// Write a chord and take up what it changed. `None` removes the key, and
    /// `Some("")` is a command that answers to nothing — which absence cannot
    /// say, since absence is the default.
    fn put(&mut self, target: Target, chord: Option<&str>, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.set_shortcut(target.key(), chord, cx)
        });
        self.recording = None;
        self.restore(cx);
        cx.notify();
    }

    fn restore(&self, cx: &mut Context<Self>) {
        restore(&self.workspace, cx);
    }
}

/// Hand the keymap, the menu bar and the system back what the file now says.
///
/// One place, because a rebind that did two of the three leaves a chord firing
/// the command it used to, or nothing at all. Free of the window because the
/// window closing is one of the things that has to call it: shutting Settings
/// while a row was recording would otherwise leave the app with no keymap and
/// no menu bar.
pub(super) fn restore(workspace: &Entity<Workspace>, cx: &mut App) {
    let shortcuts = workspace.read(cx).settings.shortcuts.clone();
    keymap::rebind(&shortcuts, cx);
    hotkey::apply(shortcuts.activate(), cx);
}
