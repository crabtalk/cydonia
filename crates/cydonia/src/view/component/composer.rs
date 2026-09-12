//! The composer: a growing field on a glass card, and the agent's slash
//! commands behind `/`.

use crate::{
    model::session::{Command, Usage},
    view::root,
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
        Render, ScrollHandle, SharedString, Window, actions, div, prelude::*, px,
    },
    theme::{self, Glass, SurfaceStyle, TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::{self, Shape, TextField},
        menu::{self, Cursor, Hit, Item},
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

/// How tall the `/` picker gets before it scrolls rather than grows — around
/// half a dozen rows, each of them two lines deep. An agent advertising a long
/// catalog would otherwise open a card taller than the window and off the top
/// of it.
const PICKER_HEIGHT: f32 = 320.;

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
/// the catalog knows it. The mark is downloaded rather than compiled in — see
/// [`crate::agent::icons`] — but reaches here as the same bytes a bezel glyph
/// is, because that is what a menu row takes.
#[derive(Clone, PartialEq)]
pub struct Agent {
    pub name: SharedString,
    pub icon: Option<&'static [u8]>,
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

/// A switchable row's name with the value it is on. A submenu row is one line
/// and has no column for a value the way the old flyout did, so what a thing
/// is set to is written into what it is called.
fn set_to(name: &str, value: Option<SharedString>) -> SharedString {
    match value {
        Some(value) => format!("{name}: {value}").into(),
        None => name.into(),
    }
}

pub struct Composer {
    field: Entity<TextField>,
    /// Byte offset of the `/` being typed, or `None` when no picker is open.
    /// Derived from the text on every change rather than stored as a flag: a
    /// backspace over the `/` has to close the picker, and a flag would have to
    /// be told.
    command: Option<usize>,
    filter: popover::Filter,
    /// The commands as the agent described them, in the filter's own order —
    /// a row's second line is `commands[item].description`.
    commands: Vec<Command>,
    /// Where the picker's list sits, since the card clamps at
    /// [`PICKER_HEIGHT`]: arrowing past the last visible row has to bring
    /// the row it landed on back into view.
    scroll: ScrollHandle,
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
    /// Where that menu is being worked: which of its rows is live, and which
    /// of them has its own panel down. One cursor for both devices, so a
    /// submenu can only ever hang off the row the pointer is on.
    cursor: Cursor,
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
            commands: Vec::new(),
            scroll: ScrollHandle::new(),
            streaming: false,
            agents: Vec::new(),
            agent: None,
            switches: Vec::new(),
            usage: None,
            menu: false,
            cursor: Cursor::default(),
        }
    }

    pub fn set_placeholder(&mut self, placeholder: &str, cx: &mut Context<Self>) {
        self.field
            .update(cx, |field, cx| field.set_placeholder(placeholder, cx));
    }

    /// The commands the active agent advertises — what `/` offers. The
    /// descriptions are held beside the filter rather than in it: the filter
    /// ranks names, and a picker row is the name with its sentence under it.
    pub fn set_commands(&mut self, commands: &[Command], cx: &mut Context<Self>) {
        if self.commands == commands {
            return;
        }
        self.commands = commands.to_vec();
        self.filter = popover::Filter::new(
            commands
                .iter()
                .map(|command| SharedString::from(format!("/{}", command.name)))
                .collect(),
        );
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
        // The open path is left alone: a chain that no longer names a submenu
        // simply stops painting, and the agent row — which is what a pick is
        // usually in the middle of — keeps its place at the top whatever the
        // switches below it did.
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
            // Narrowing re-enters the list at the top, and the view it is read
            // through has to go back with it.
            self.reveal();
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
        self.reveal();
        cx.notify();
    }

    fn command_previous(&mut self, _: &CommandPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.filter.step(-1);
        self.reveal();
        cx.notify();
    }

    /// Scroll the highlighted row back inside the clamped card. The card's
    /// children are the filtered rows one for one, so the position the filter
    /// reports is the child gpui indexes.
    fn reveal(&self) {
        if let Some(active) = self.filter.active() {
            self.scroll.scroll_to_item(active);
        }
    }

    /// Escape backs out of whatever is happening, outermost first: the agent
    /// menu, then the command picker, and the turn in flight once there is
    /// nothing left to close.
    fn command_dismiss(&mut self, _: &CommandDismiss, _: &mut Window, cx: &mut Context<Self>) {
        if self.cursor.ascend() {
            // A submenu shuts before the menu holding it — one press, one level.
        } else if self.menu {
            self.close_menu();
        } else if self.command.take().is_none() {
            cx.emit(ComposerEvent::Cancel);
        }
        cx.notify();
    }

    /// The picker, resting on the pill's top edge and growing upward from it.
    /// Not dropped from the caret: the composer is already at the bottom of
    /// the window, so a card hanging below the `/` covers the very text being
    /// typed into it. Anchored to the pill rather than the caret, it also
    /// rides up with the box as the field grows.
    fn picker(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.command?;
        let items: Vec<Item> = self
            .filter
            .filtered()
            .iter()
            .map(|&item| {
                let row = Item::action(self.filter.items()[item].clone());
                // An agent is free to send an empty one, and a blank second
                // line would read as a gap rather than as a description.
                match self.commands[item].description.trim() {
                    "" => row,
                    description => row.with_description(description.to_string()),
                }
            })
            .collect();
        if items.is_empty() {
            return None;
        }
        // The filter is what the highlight lives in — it survives a repaint
        // and the card does not — so the cursor is made from it each frame
        // rather than kept beside it, where the two could disagree.
        let mut cursor = Cursor::default();
        if let Some(active) = self.filter.active() {
            cursor.point_at(&items, &[active]);
        }
        // The card reports the row it was on; the commands behind those rows
        // are whatever the query left standing.
        let filtered = self.filter.filtered().to_vec();
        Some(popover::anchored_menu_above(
            "composer-commands",
            menu::card(
                theme,
                "composer-commands",
                &items,
                &cursor,
                cx,
                move |composer, hit, _, cx| match hit {
                    // The pointer moves the same highlight the arrows do, so
                    // Enter always takes the row that is lit.
                    Hit::Point(path) => {
                        if let [row] = path[..] {
                            composer.filter.set_active(row);
                            cx.notify();
                        }
                    }
                    Hit::Choose(path) => {
                        if let [row] = path[..]
                            && let Some(&item) = filtered.get(row)
                        {
                            composer.accept(item, cx);
                        }
                    }
                    // Not dismissed on an out-click: `reread` reopens the
                    // picker from the text on the very next thing the field
                    // reports, cursor moves included, so a press into the
                    // field would shut it and open it again in one frame.
                    // Escape and editing the `/` away are what close it.
                    Hit::Dismiss => {}
                },
            )
            .id("composer-commands-list")
            .max_h(px(PICKER_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .into_any_element(),
            None,
        ))
    }

    /// Shut the menu, and whatever it had a panel down over.
    fn close_menu(&mut self) {
        self.menu = false;
        self.cursor.clear();
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
                Some(glyph) => icons::icon(glyph)
                    .size(mark)
                    .text_color(theme.text_muted)
                    .into_any_element(),
                // A slot the catalog has no mark for still has to open the menu.
                None => icons::icon(icons::layout::LayoutGrid)
                    .size(mark)
                    .text_color(theme.text_muted)
                    .into_any_element(),
            })
            .on_click(cx.listener(|composer, _, _, cx| {
                composer.menu = !composer.menu;
                composer.cursor.clear();
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

    /// The agent mark's menu: a row per thing the session can be switched
    /// between, each dropping a panel of its own, and the context meter under
    /// a rule at the bottom. The meter is hung on the card rather than being a
    /// row of it, because it answers back instead of offering a choice.
    fn menu_card(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.menu {
            return None;
        }
        let items = self.menu_items();
        // The card reports a row by its path; what stands behind those paths
        // is the list it was built from, so the handler is given it.
        let rows = items.clone();
        let card = menu::card(
            theme,
            "composer-menu",
            &items,
            &self.cursor,
            cx,
            move |composer, hit, _, cx| composer.hit(&rows, hit, cx),
        )
        .child(popover::divider())
        .child(self.usage_row(theme));
        Some(popover::anchored_menu_above(
            "composer-menu",
            card.into_any_element(),
            None,
        ))
    }

    /// The menu's rows: the agent first, then whatever the live session
    /// offers. Each carries the value it is on in its own name — the reason to
    /// open one of these is as often to read what it is set to as to change
    /// it, and a submenu row has one line to say both on.
    ///
    /// No leading glyphs here: these rows are words, and one icon among them
    /// would open an empty gutter down the menu's left. The agents inside the
    /// first panel keep their marks, where every row has one.
    fn menu_items(&self) -> Vec<Item> {
        let current = self.agent.and_then(|ix| self.agents.get(ix));
        let mut agents: Vec<Item> = self
            .agents
            .iter()
            .enumerate()
            .map(|(ix, agent)| {
                let row = Item::action(agent.name.clone()).checked(Some(ix) == self.agent);
                match agent.icon {
                    Some(mark) => row.with_icon(mark),
                    None => row,
                }
            })
            .collect();
        // Nothing installed is a panel of one row, and a rule above it would
        // be a rule under nothing.
        if !agents.is_empty() {
            agents.push(Item::Separator);
        }
        agents.push(Item::action("Install an agent…").with_icon(icons::files::Download));
        let mut items = vec![Item::submenu(
            set_to("Agent", current.map(|agent| agent.name.clone())),
            agents,
        )];
        items.extend(self.switches.iter().map(|switch| {
            Item::submenu(
                set_to(&switch.name, self.value_of(switch)),
                switch
                    .options
                    .iter()
                    .map(|option| {
                        Item::action(option.name.clone())
                            .checked(switch.current.as_ref() == Some(&option.id))
                    })
                    .collect(),
            )
        }));
        items
    }

    /// What the pointer did to that menu. A path is one row per level — the
    /// top-level row, then which of its alternatives — so reading one is
    /// [`Self::menu_items`] taken backwards.
    fn hit(&mut self, items: &[Item], hit: Hit, cx: &mut Context<Self>) {
        match hit {
            Hit::Point(path) => {
                if self.cursor.point_at(items, &path) {
                    cx.notify();
                }
            }
            Hit::Choose(path) => {
                let [row, at] = path[..] else { return };
                // Picking anything shuts the menu: every choice here is the
                // session's, and none of them is made twice in a row.
                self.close_menu();
                if row == 0 {
                    // Past the last agent is the install row — which is also
                    // the whole panel when nothing is installed.
                    match at < self.agents.len() {
                        true => cx.emit(ComposerEvent::Agent(at)),
                        false => cx.emit(ComposerEvent::Install),
                    }
                } else if let Some(switch) = self.switches.get(row - 1)
                    && let Some(option) = switch.options.get(at)
                {
                    cx.emit(ComposerEvent::Switch(switch.id.clone(), option.id.clone()));
                }
                cx.notify();
            }
            Hit::Dismiss => {
                self.close_menu();
                cx.notify();
            }
        }
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
            icons::multimedia::CircleStop
        } else {
            icons::arrows::ArrowUp
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

    fn body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let picker = self.picker(&theme, cx);
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
                        // The pill's positioning parent, as with the agent
                        // mark: a surface draws its whole subtree in one
                        // layer, so the picker hangs off the box around the
                        // pill rather than out of it.
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .w_full()
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
                            )
                            .children(picker),
                    ),
            )
    }
}

impl Focusable for Composer {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.read(cx).focus_handle(cx)
    }
}

impl Render for Composer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.body(cx)
    }
}
