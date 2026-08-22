//! The composer: a growing field on a frosted card, and the agent's slash
//! commands behind `/`.

use bezel::{
    gpui::{
        AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, Render,
        SharedString, Window, div, point, prelude::*, px,
    },
    theme::{self, Theme},
    ui::{
        icons,
        input::{self, Shape, TextField},
        popover,
    },
};
use gpui::actions;

actions!(
    cydonia_composer,
    [Send, CommandNext, CommandPrevious, CommandDismiss]
);

/// Claimed on top of `TextField`/`TextArea`, so `enter` sends here and stays a
/// newline in every other multi-line field.
const KEY_CONTEXT: &str = "CydoniaComposer";

pub fn init(cx: &mut App) {
    let ctx = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", Send, ctx),
        // Bound explicitly: the field's own `enter` is what usually inserts a
        // newline, and the composer has just taken it.
        KeyBinding::new("shift-enter", input::InsertNewline, ctx),
        KeyBinding::new("down", CommandNext, ctx),
        KeyBinding::new("up", CommandPrevious, ctx),
        KeyBinding::new("escape", CommandDismiss, ctx),
    ]);
}

pub enum ComposerEvent {
    Submit(String),
    Cancel,
}

pub struct Composer {
    field: Entity<TextField>,
    /// Byte offset of the `/` being typed, or `None` when no picker is open.
    /// Derived from the text on every change rather than stored as a flag: a
    /// backspace over the `/` has to close the picker, and a flag would have to
    /// be told.
    command: Option<usize>,
    filter: popover::Filter,
    /// Whether a turn is in flight — what the button does when pressed.
    streaming: bool,
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| {
            TextField::new(cx)
                .with_shape(Shape::Grow { min: 2, max: 12 })
                .with_key_context(KEY_CONTEXT)
                .with_placeholder("message the agent…")
        });
        cx.observe(&field, |composer: &mut Self, _, cx| composer.reread(cx))
            .detach();
        Self {
            field,
            command: None,
            filter: popover::Filter::new(Vec::new()),
            streaming: false,
        }
    }

    pub fn set_placeholder(&mut self, placeholder: &str, cx: &mut Context<Self>) {
        self.field
            .update(cx, |field, cx| field.set_placeholder(placeholder, cx));
    }

    /// The commands the active agent advertises — what `/` offers.
    pub fn set_commands(&mut self, commands: &[String], cx: &mut Context<Self>) {
        let items: Vec<SharedString> = commands
            .iter()
            .map(|name| SharedString::from(format!("/{name}")))
            .collect();
        if items.as_slice() == self.filter.items() {
            return;
        }
        self.filter = popover::Filter::new(items);
        self.command = None;
        cx.notify();
    }

    pub fn set_streaming(&mut self, streaming: bool, cx: &mut Context<Self>) {
        if self.streaming != streaming {
            self.streaming = streaming;
            cx.notify();
        }
    }

    pub fn is_empty(&self, cx: &App) -> bool {
        self.field.read(cx).content().trim().is_empty()
    }

    /// The picker trigger, and it is a *read* of the text rather than a key
    /// handler: a `/` opening the first line, with no whitespace since. Typing,
    /// pasting, arrowing back into the word and deleting the `/` all agree
    /// without any of them being special-cased.
    fn reread(&mut self, cx: &mut Context<Self>) {
        let content = self.field.read(cx).content().clone();
        let caret = self.field.read(cx).cursor().min(content.len());
        self.command = content
            .starts_with('/')
            .then_some(0)
            .filter(|_| !self.filter.items().is_empty())
            .filter(|_| !content[1..caret].contains(char::is_whitespace));
        if self.command.is_some() {
            self.filter.refilter(&content[1..caret]);
        }
        cx.notify();
    }

    /// Replace the typed `/query` with the picked command.
    fn accept(&mut self, item: usize, cx: &mut Context<Self>) {
        let content = self.field.read(cx).content().clone();
        let caret = self.field.read(cx).cursor().min(content.len());
        let picked = format!("{} ", self.filter.items()[item]);
        let rest = content[caret..].to_string();
        self.field
            .update(cx, |field, cx| field.set_content(picked + &rest, cx));
        self.command = None;
        cx.notify();
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) {
        // `enter` is one key doing two jobs: while the picker is up it takes
        // the highlighted row, exactly as the combobox's does.
        if self.command.is_some()
            && let Some(item) = self.filter.active_item()
        {
            self.accept(item, cx);
            return;
        }
        let content = self.field.read(cx).content().clone();
        if content.trim().is_empty() {
            return;
        }
        self.field.update(cx, |field, cx| field.clear(cx));
        self.command = None;
        cx.emit(ComposerEvent::Submit(content.to_string()));
        cx.notify();
    }

    fn send(&mut self, _: &Send, _: &mut Window, cx: &mut Context<Self>) {
        self.submit(cx);
    }

    fn command_next(&mut self, _: &CommandNext, _: &mut Window, cx: &mut Context<Self>) {
        self.filter.step(1);
        cx.notify();
    }

    fn command_previous(&mut self, _: &CommandPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.filter.step(-1);
        cx.notify();
    }

    /// Escape backs out of whatever is happening: the picker first, and the
    /// turn in flight once there is no picker left to close.
    fn command_dismiss(&mut self, _: &CommandDismiss, _: &mut Window, cx: &mut Context<Self>) {
        if self.command.take().is_none() {
            cx.emit(ComposerEvent::Cancel);
        }
        cx.notify();
    }

    /// The picker, anchored at the `/` itself — `TextField::offset_bounds` is
    /// the same measurement the IME candidate panel anchors to, so it follows
    /// the caret down as the box grows.
    fn picker(&self, theme: &Theme, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let slash = self.command?;
        let anchor = self.field.read(cx).offset_bounds(slash, window)?;
        let rows: Vec<AnyElement> = self
            .filter
            .filtered()
            .iter()
            .enumerate()
            .map(|(position, &item)| {
                popover::menu_row(
                    theme,
                    Some(position) == self.filter.active(),
                    format!("command-{item}"),
                )
                .id(SharedString::from(format!("command-{item}")))
                .on_click(cx.listener(move |composer, _, _, cx| composer.accept(item, cx)))
                .child(self.filter.items()[item].clone())
                .into_any_element()
            })
            .collect();
        if rows.is_empty() {
            return None;
        }
        Some(popover::menu_at(
            "composer-commands",
            point(anchor.left(), anchor.bottom() + px(4.)),
            popover::popover_card(theme)
                .w(px(280.))
                .child(div().flex().flex_col().children(rows))
                .into_any_element(),
            None,
        ))
    }

    /// Send, as a 24px disc — a stop square while a turn is in flight, and
    /// inert when there is nothing to send. Quietened rather than faded, so the
    /// glyph stays legible and nothing invites a press.
    fn button(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let streaming = self.streaming;
        let ready = streaming || !self.is_empty(cx);
        let glyph = if streaming {
            icons::STOP
        } else {
            icons::ARROW_UP
        };
        let disc = div()
            .size(px(24.))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center();
        let disc = if ready {
            disc.bg(if streaming { theme.danger } else { theme.solid })
                .cursor_pointer()
                .hover(|s| s.opacity(0.9))
                .child(icons::icon(glyph).size(px(12.)).text_color(if streaming {
                    theme.on_accent
                } else {
                    theme.on_solid
                }))
        } else {
            disc.bg(theme::ink(0.06)).child(
                icons::icon(glyph)
                    .size(px(12.))
                    .text_color(theme.text_faint),
            )
        };
        div()
            .id("composer-send")
            .on_click(cx.listener(|composer, _, _, cx| {
                if composer.streaming {
                    cx.emit(ComposerEvent::Cancel);
                } else {
                    composer.submit(cx);
                }
            }))
            .child(disc)
            .into_any_element()
    }

    fn body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let picker = self.picker(&theme, window, cx);
        let hint = if self.command.is_some() {
            "↑↓ pick · enter run · esc close"
        } else {
            "enter send · shift-enter newline"
        };

        div()
            .on_action(cx.listener(Self::send))
            .on_action(cx.listener(Self::command_next))
            .on_action(cx.listener(Self::command_previous))
            .on_action(cx.listener(Self::command_dismiss))
            .child(
                div()
                    .rounded(px(Theme::SURFACE_RADIUS))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.card_glass_bg())
                    .px(px(4.))
                    .pt(px(4.))
                    .pb(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(self.field.clone())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .px(px(6.))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family(theme.font_mono.clone())
                                    .text_color(theme.text_faint)
                                    .child(hint),
                            )
                            .child(self.button(&theme, cx)),
                    ),
            )
            .children(picker)
    }
}

impl Focusable for Composer {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.read(cx).focus_handle(cx)
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.body(window, cx)
    }
}
