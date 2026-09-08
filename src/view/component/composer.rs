//! The composer: a growing field on a glass card, and the agent's slash
//! commands behind `/`.

use crate::{
    model::session::Usage,
    view::{component::ext::submenu, root},
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
        KeyBinding, Render, SharedString, Window, actions, div, point, prelude::*, px, svg,
    },
    theme::{self, Glass, SurfaceStyle, TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::{self, Shape, TextField},
        menu::{self, Item},
        popover,
        surface::Surfaced as _,
        tooltip::Tooltip,
        widgets::Controls as _,
    },
};

actions!(
    cydonia_composer,
    [Send, CommandNext, CommandPrevious, CommandDismiss]
);

/// Claimed on top of `TextField`/`TextArea`, so `enter` sends here and stays a
/// newline in every other multi-line field.
const KEY_CONTEXT: &str = "CydoniaComposer";

/// What the pill and the agent mark are cut from — and every card that floats
/// in the same stack over the transcript, which is why it is not private.
pub(crate) const SURFACE: SurfaceStyle = SurfaceStyle::Glass(Glass::Regular);

/// How full the context has to be before the meter says so in amber. Late
/// enough that it is not shouting through a normal conversation, early enough
/// to leave room to compact before a turn is refused.
const WARN_AT: f32 = 0.8;

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

/// Which request a [`Switch`] is, since the agent offers two shapes of the
/// same idea.
#[derive(Clone, PartialEq, Eq)]
pub enum SwitchId {
    /// `session/set_mode`. A session has one mode, so it carries no id.
    Mode,
    /// `session/set_config_option`, by the option's own id — which is where
    /// the model lives.
    Config(SharedString),
}

/// One value a switch can be set to.
#[derive(Clone, PartialEq)]
pub struct SwitchOption {
    pub id: SharedString,
    pub name: SharedString,
}

/// One switchable thing the session offers: the agent's mode, or a config
/// option. One shape for both — the composer shows a value and reports a pick,
/// and which request that is stays the session's business.
#[derive(Clone, PartialEq)]
pub struct Switch {
    pub id: SwitchId,
    /// What the thing is called, shown when it is on a value the agent no
    /// longer offers — which happens when an update lands mid-pick.
    pub name: SharedString,
    pub current: Option<SharedString>,
    pub options: Vec<SwitchOption>,
}

pub enum ComposerEvent {
    Submit(String),
    Cancel,
    /// Talk to this agent instead — an index into the configured agents.
    Agent(usize),
    /// Nothing here to pick: open settings where agents are installed.
    Install,
    /// Set a switch to one of its values, by id.
    Switch(SwitchId, SharedString),
}

/// Which row's alternatives are flown out. The agents are one of these too —
/// the menu gives every choice the session offers the same shape.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flyout {
    Agent,
    Switch(usize),
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
    /// What the live session can be switched between — its mode, its model.
    /// Empty for a session with no agent behind it.
    switches: Vec<Switch>,
    /// Context spent, when the agent counts it.
    usage: Option<Usage>,
    /// Whether the agent mark's menu is up.
    menu: bool,
    /// Which row's alternatives are flown out beside it, while they are.
    ///
    /// Sticky rather than tied to the row's own hover: the card is a deferred
    /// layer and not geometrically inside the row, so a pointer travelling into
    /// it leaves the row and would close what it was reaching for. Another row
    /// taking the hover is what closes this one — which is what AppKit's
    /// submenus do anyway.
    submenu: Option<Flyout>,
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
            switches: Vec::new(),
            usage: None,
            menu: false,
            submenu: None,
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
        self.close_menu();
        cx.notify();
    }

    /// What the session can be switched between, and how much context it has
    /// spent. Both belong to a live connection, so both go empty with one.
    pub fn set_switches(&mut self, switches: &[Switch], cx: &mut Context<Self>) {
        if self.switches == switches {
            return;
        }
        self.switches = switches.to_vec();
        // A flown-out switch is an index into the list that just changed. The
        // agents are not in that list and keep whatever they had open.
        self.submenu = self.submenu.filter(|open| *open == Flyout::Agent);
        cx.notify();
    }

    pub fn set_usage(&mut self, usage: Option<Usage>, cx: &mut Context<Self>) {
        let same = match (self.usage, usage) {
            (Some(held), Some(next)) => held.used == next.used && held.size == next.size,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.usage = usage;
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
        if self.submenu.take().is_some() {
            // A submenu shuts before the menu holding it — one press, one level.
        } else if self.menu {
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
    /// Shut the menu and whatever was flown out of it.
    fn close_menu(&mut self) {
        self.menu = false;
        self.submenu = None;
    }

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
                None => icons::icon(icons::system::WIDGET)
                    .size(mark)
                    .text_color(theme.text_muted)
                    .into_any_element(),
            })
            .on_click(cx.listener(|composer, _, _, cx| {
                composer.menu = !composer.menu;
                composer.submenu = None;
                cx.notify();
            }));
        Some(
            div()
                .relative()
                .flex_none()
                // A surface draws its whole subtree in one layer, so the menu
                // hangs off the positioning parent beside it.
                .child(button.surface(theme, SURFACE))
                .children(self.menu_card(theme, cx))
                .into_any_element(),
        )
    }

    /// The agent mark's menu. Every choice the session offers has the same
    /// shape: a row naming it, the value it is on, and a card of alternatives
    /// off its trailing edge. See [`submenu`] for why that is not `menu::card`.
    fn menu_card(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.menu {
            return None;
        }
        let card = submenu::card(theme)
            .child(self.agent_row(theme, cx))
            .children(
                self.switches
                    .iter()
                    .enumerate()
                    .map(|(ix, switch)| self.switch_row(ix, switch, theme, cx)),
            )
            // Under a rule, and last: the rows above are what the session can
            // be *switched* to, and this is the one line that answers back.
            .child(popover::divider())
            .child(self.usage_row(theme))
            .on_mouse_down_out(cx.listener(|composer, _, _, cx| {
                composer.close_menu();
                cx.notify();
            }));
        Some(popover::anchored_menu_above(
            "composer-menu",
            card.into_any_element(),
            None,
        ))
    }

    /// Which agent the session talks to, carrying the mark of whoever it is on
    /// as its own glyph.
    fn agent_row(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let agent = self.agent.and_then(|ix| self.agents.get(ix));
        let card = submenu::card(theme)
            .children(self.agents.iter().enumerate().map(|(ix, agent)| {
                submenu::row(
                    ("composer-agent-value", ix),
                    agent.name.clone(),
                    agent.icon.clone(),
                    Some(ix) == self.agent,
                    theme,
                    cx,
                )
                .on_click(cx.listener(move |composer, _, _, cx| {
                    composer.close_menu();
                    cx.emit(ComposerEvent::Agent(ix));
                    cx.notify();
                }))
                .into_any_element()
            }))
            .child(popover::divider())
            .child(
                submenu::row(
                    "composer-install",
                    "Install an agent…".into(),
                    Some(icons::files::DOWNLOAD.into()),
                    false,
                    theme,
                    cx,
                )
                .on_click(cx.listener(|composer, _, _, cx| {
                    composer.close_menu();
                    cx.emit(ComposerEvent::Install);
                    cx.notify();
                })),
            );
        // The mark rides with the name it belongs to rather than sitting in
        // the leading column: the column is for what a row *is*, and every row
        // here is a word.
        let value = agent.map(|agent| {
            let value = submenu::Value::new(agent.name.clone());
            match agent.icon.clone() {
                Some(mark) => value.with_icon(mark),
                None => value,
            }
        });
        self.opener(
            Flyout::Agent,
            "composer-agent-row",
            "Agent".into(),
            value,
            card,
            theme,
            cx,
        )
    }

    /// One switch — a mode, or a config option like the model.
    fn switch_row(
        &self,
        ix: usize,
        switch: &Switch,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = switch.id.clone();
        let card =
            submenu::card(theme).children(switch.options.iter().enumerate().map(|(at, option)| {
                let value = option.id.clone();
                let id = id.clone();
                submenu::row(
                    ("composer-switch-value", at),
                    option.name.clone(),
                    None,
                    switch.current.as_ref() == Some(&option.id),
                    theme,
                    cx,
                )
                .on_click(cx.listener(move |composer, _, _, cx| {
                    composer.close_menu();
                    cx.emit(ComposerEvent::Switch(id.clone(), value.clone()));
                    cx.notify();
                }))
                .into_any_element()
            }));
        self.opener(
            Flyout::Switch(ix),
            ("composer-switch-row", ix),
            switch.name.clone(),
            self.value_of(switch).map(submenu::Value::new),
            card,
            theme,
            cx,
        )
    }

    /// A row that opens `card` beside itself, with which one is out kept here —
    /// one menu has at most one submenu, so the state belongs to the menu.
    #[expect(
        clippy::too_many_arguments,
        reason = "one row, and it has that many parts"
    )]
    fn opener(
        &self,
        flyout: Flyout,
        id: impl Into<ElementId>,
        label: SharedString,
        value: Option<submenu::Value>,
        card: gpui::Div,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        submenu::opener(
            id,
            label,
            value,
            // No leading glyph: these rows are words, and the column would open
            // an empty gutter down the menu's left.
            None,
            self.submenu == Some(flyout),
            card,
            theme,
            cx,
            move |composer, open, cx| {
                composer.submenu = open.then_some(flyout);
                cx.notify();
            },
        )
    }

    /// The name of the value a switch is on, when it is on one the agent still
    /// offers. An update landing mid-pick is what leaves it on one that is gone.
    fn value_of(&self, switch: &Switch) -> Option<SharedString> {
        let current = switch.current.as_ref()?;
        switch
            .options
            .iter()
            .find(|option| &option.id == current)
            .map(|option| option.name.clone())
    }

    /// Context spent, as the menu carries it: a name, and whatever the agent
    /// has counted. The number carries the warning rather than the track,
    /// because bezel's bar paints its fill from the theme and recolouring it
    /// would mean reimplementing it.
    ///
    /// A dash until it counts, and for an agent that never does — the two are
    /// one state here, since nothing in ACP asks and only an update tells. Not
    /// a 0%: a session with nothing said is not empty, its prompt and its
    /// tools being in the window before you type.
    fn usage_row(&self, theme: &Theme) -> AnyElement {
        let row = div()
            .id("composer-usage")
            // The metrics `popover::menu_row` gives a row, without the hover
            // and the pointer: nothing here is pressable.
            .px(px(8.))
            .py(px(6.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.))
            .text_style(TextStyle::Body)
            .text_color(theme.text_muted)
            .child(div().flex_1().min_w_0().child("Context"));
        let Some((usage, fraction)) = self
            .usage
            .and_then(|usage| Some((usage, usage.fraction()?)))
        else {
            return row
                .child(
                    div()
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_faint)
                        .child("—"),
                )
                .into_any_element();
        };
        let percent = (fraction * 100.).round() as u32;
        let (used, size) = (usage.used, usage.size);
        row
            // The raw counts would be noise in a row of words, and the tooltip
            // has them for whoever wants them.
            .tooltip(move |window, cx| {
                Tooltip::text(format!("{used} of {size} tokens"), window, cx)
            })
            .child(
                div()
                    .w(px(44.))
                    .flex_none()
                    .child(theme.progress_bar(fraction)),
            )
            .child(
                div()
                    .text_style(TextStyle::Caption)
                    .text_color(match fraction >= WARN_AT {
                        true => theme.warning,
                        false => theme.text_faint,
                    })
                    .child(format!("{percent}%")),
            )
            .into_any_element()
    }

    /// Send, as the disc inside the pill's trailing end — a stop square while a
    /// turn is in flight, and inert when there is nothing to send. Quietened so
    /// the glyph stays legible and nothing invites a press.
    fn button(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let streaming = self.streaming;
        let ready = streaming || !self.is_empty(cx);
        let glyph = if streaming {
            icons::media::STOP
        } else {
            icons::arrows::ARROW_UP
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
            .flex()
            .flex_col()
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
