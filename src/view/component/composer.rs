//! The composer: a growing field on a glass card, and the agent's slash
//! commands behind `/`.

use crate::view::root;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
        Render, SharedString, Window, actions, div, point, prelude::*, px, svg,
    },
    theme::{self, Glass, SurfaceStyle, Theme},
    ui::{
        icons,
        input::{self, Shape, TextField},
        menu::{self, Item},
        popover,
        surface::Surfaced as _,
    },
};

actions!(
    cydonia_composer,
    [Send, CommandNext, CommandPrevious, CommandDismiss]
);

/// Claimed on top of `TextField`/`TextArea`, so `enter` sends here and stays a
/// newline in every other multi-line field.
const KEY_CONTEXT: &str = "CydoniaComposer";

/// What the pill and the agent mark are cut from.
const SURFACE: SurfaceStyle = SurfaceStyle::Glass(Glass::Regular);

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

/// One agent on offer: what to call it, and the registry's mark for it when
/// the catalog knows it.
#[derive(Clone, PartialEq)]
pub struct Agent {
    pub name: SharedString,
    pub icon: Option<SharedString>,
}

pub enum ComposerEvent {
    Submit(String),
    Cancel,
    /// Talk to this agent instead — an index into the configured agents.
    Agent(usize),
    /// Nothing here to pick: open settings where agents are installed.
    Install,
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
    /// The configured agents, and which one the session runs on.
    agents: Vec<Agent>,
    agent: Option<usize>,
    menu: bool,
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| {
            TextField::new(cx)
                .with_shape(Shape::Grow { min: 1, max: 12 })
                // The pill is the field's frame.
                .with_frame(false)
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
            agents: Vec::new(),
            agent: None,
            menu: false,
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

    /// The agents on offer, and the one the session is talking to.
    pub fn set_agents(&mut self, agents: &[Agent], current: Option<usize>, cx: &mut Context<Self>) {
        if self.agents == agents && self.agent == current {
            return;
        }
        self.agents = agents.to_vec();
        self.agent = current;
        self.menu = false;
        cx.notify();
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

    /// Escape backs out of whatever is happening, outermost first: the agent
    /// menu, then the command picker, and the turn in flight once there is
    /// nothing left to close.
    fn command_dismiss(&mut self, _: &CommandDismiss, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu {
            self.menu = false;
        } else if self.command.take().is_none() {
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
        let items: Vec<Item> = self
            .filter
            .filtered()
            .iter()
            .map(|&item| Item::action(self.filter.items()[item].clone()))
            .collect();
        if items.is_empty() {
            return None;
        }
        // The card reports the row it was on; the commands behind those rows
        // are whatever the query left standing.
        let filtered = self.filter.filtered().to_vec();
        Some(popover::menu_at(
            "composer-commands",
            point(anchor.left(), anchor.bottom() + px(4.)),
            menu::card(
                theme,
                "composer-commands",
                &items,
                self.filter.active(),
                cx,
                move |composer, row, _, cx| composer.accept(filtered[row], cx),
            )
            .into_any_element(),
            None,
        ))
    }

    /// The agent the session runs on, as the mark that opens the rest — the
    /// placeholder already carries its name. Picking one is the app's call to
    /// act on: an ACP session is bound to the process serving it, so the
    /// composer only reports the choice.
    fn chip(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let agent = self.agent.and_then(|ix| self.agents.get(ix))?.clone();
        let mark = px(root::composer_height() / 2.);
        let button = div()
            .id("composer-agent")
            .size(px(root::composer_height()))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|button| button.bg(theme.element_hover))
            .child(match agent.icon {
                Some(path) => svg()
                    .path(path)
                    .size(mark)
                    .flex_none()
                    .text_color(theme.text_muted)
                    .into_any_element(),
                // A slot the catalog has no mark for still has to open the menu.
                None => icons::icon(icons::WIDGET)
                    .size(mark)
                    .text_color(theme.text_muted)
                    .into_any_element(),
            })
            .on_click(cx.listener(|composer, _, _, cx| {
                composer.menu = !composer.menu;
                cx.notify();
            }));
        Some(
            div()
                .relative()
                .flex_none()
                // A surface draws its whole subtree in one layer, so the menu
                // hangs off the positioning parent beside it.
                .child(button.surface(theme, SURFACE))
                .children(self.agent_menu(theme, cx))
                .into_any_element(),
        )
    }

    /// Opens upward from the chip: `anchored_menu_above` pins to the trigger's
    /// top-left, which is why the chip carries the `relative`.
    fn agent_menu(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.menu {
            return None;
        }
        let mut items: Vec<Item> = self
            .agents
            .iter()
            .enumerate()
            .map(|(ix, agent)| {
                let item = Item::action(agent.name.clone()).checked(Some(ix) == self.agent);
                match agent.icon.clone() {
                    Some(mark) => item.with_icon(mark),
                    None => item,
                }
            })
            .collect();
        items.push(Item::action("Install an agent…").with_icon(icons::DOWNLOAD));
        // The install row sits past the last agent, so the row it reports is an
        // agent exactly while it is in range.
        let agents = self.agents.len();
        Some(popover::anchored_menu_above(
            "composer-agents",
            menu::card(
                theme,
                "composer-agents",
                &items,
                None,
                cx,
                move |composer, row, _, cx| {
                    composer.menu = false;
                    match row < agents {
                        true => cx.emit(ComposerEvent::Agent(row)),
                        false => cx.emit(ComposerEvent::Install),
                    }
                    cx.notify();
                },
            )
            .on_mouse_down_out(cx.listener(|composer, _, _, cx| {
                composer.menu = false;
                cx.notify();
            }))
            .into_any_element(),
            None,
        ))
    }

    /// Send, as the disc inside the pill's trailing end — a stop square while a
    /// turn is in flight, and inert when there is nothing to send. Quietened so
    /// the glyph stays legible and nothing invites a press.
    fn button(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let streaming = self.streaming;
        let ready = streaming || !self.is_empty(cx);
        let glyph = if streaming {
            icons::STOP
        } else {
            icons::ARROW_UP
        };
        let glyph_size = px(root::composer_disc() / 2.);
        let disc = div()
            .flex_none()
            .size(px(root::composer_disc()))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center();
        let disc = if ready {
            disc.bg(if streaming { theme.danger } else { theme.solid })
                .cursor_pointer()
                .hover(|s| s.opacity(0.9))
                .child(
                    icons::icon(glyph)
                        .size(glyph_size)
                        .text_color(if streaming {
                            theme.on_accent
                        } else {
                            theme.on_solid
                        }),
                )
        } else {
            disc.bg(theme::ink(0.06)).child(
                icons::icon(glyph)
                    .size(glyph_size)
                    .text_color(theme.text_faint),
            )
        };
        div()
            .id("composer-send")
            .flex_none()
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
        let radius = px(root::composer_height() / 2.);

        div()
            .on_action(cx.listener(Self::send))
            .on_action(cx.listener(Self::command_next))
            .on_action(cx.listener(Self::command_previous))
            .on_action(cx.listener(Self::command_dismiss))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    // The capsule and the disc hold the last line as the field
                    // grows up past them.
                    .items_end()
                    .gap(px(root::COMPOSER_INSET))
                    .children(self.chip(&theme, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .rounded(radius)
                            .p(px(root::COMPOSER_INSET))
                            .flex()
                            .flex_row()
                            .items_end()
                            .gap(px(root::COMPOSER_INSET))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h(px(root::composer_disc()))
                                    .flex()
                                    .items_center()
                                    .child(self.field.clone()),
                            )
                            .child(self.button(&theme, cx))
                            .surface(&theme, SURFACE),
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
