//! The composer: a growing field on a glass card, and the agent's slash
//! commands behind `/`.

mod activity;
pub use activity::Activity;

use super::image_preview::{self, disc};
use crate::{
    model::{
        media::Attachment,
        session::{Command, Usage},
    },
    view::root,
};
use bezel::ui::scroll as scrollbars;
use bezel::{
    gpui::{
        self, AnyElement, App, ClipboardEntry, Context, Entity, EventEmitter, ExternalPaths,
        FocusHandle, Focusable, KeyBinding, ObjectFit, Render, ScrollHandle, SharedString, Window,
        actions, div, img, prelude::*, px,
    },
    theme::{Glass, SurfaceStyle, TextStyle, Theme, Typeset},
    ui::{
        icons::{self, Icon},
        input::{self, FieldEvent, Shape, TextField},
        menu::{self, Cursor, Hit, Item},
        popover,
        surface::{self, Surfaced as _},
        tooltip::Tooltip,
        widgets::{Buttons as _, Controls as _},
    },
};
use std::{collections::HashMap, sync::Arc};

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

/// The most options a switch drops as a submenu. A menu panel has no height
/// cap of its own, so a longer list — a catalog of models — opens as a flat
/// list clamped at [`PICKER_HEIGHT`] instead.
const SUBMENU_ROWS: usize = 12;

/// The side of a picture waiting in the composer.
const THUMB: f32 = 64.;
const THUMB_RADIUS: f32 = 8.;

/// The side of a thumb's remove button, which sits centred on its corner.
const REMOVE: f32 = 14.;

/// How much of the window an opened picture may take, either way.
const PREVIEW_SHARE: f32 = 0.8;

fn picture(attachment: &Attachment) -> gpui::Img {
    match attachment {
        Attachment::Bytes(image) => img(image.clone()),
        Attachment::File(path) => img(path.clone()),
    }
}

/// The picture's pixel size, read from its header.
fn dimensions(attachment: &Attachment) -> Option<(u32, u32)> {
    match attachment {
        Attachment::Bytes(image) => image::ImageReader::new(std::io::Cursor::new(&image.bytes))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok(),
        Attachment::File(path) => image::image_dimensions(path).ok(),
    }
}

pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(KEY_CONTEXT);
    let mut bindings = vec![
        KeyBinding::new("enter", Send, ctx),
        // Bound explicitly: the field's own `enter` is what usually inserts a
        // newline, and the composer has just taken it.
        KeyBinding::new("shift-enter", input::InsertNewline, ctx),
        KeyBinding::new("down", CommandNext, ctx),
        KeyBinding::new("up", CommandPrevious, ctx),
        KeyBinding::new("escape", CommandDismiss, ctx),
    ];
    if cfg!(target_os = "macos") {
        bindings.extend([
            KeyBinding::new("alt-left", input::WordLeft, ctx),
            KeyBinding::new("alt-right", input::WordRight, ctx),
            KeyBinding::new("alt-shift-left", input::SelectWordLeft, ctx),
            KeyBinding::new("alt-shift-right", input::SelectWordRight, ctx),
            KeyBinding::new("alt-backspace", input::DeleteWordLeft, ctx),
            KeyBinding::new("alt-delete", input::DeleteWordRight, ctx),
        ]);
    }
    bindings
}

/// One agent on offer: what to call it, and the registry's mark for it when
/// the catalog knows it. The mark is downloaded rather than compiled in — see
/// [`crate::agent::icons`] — but reaches here as an [`Icon`] like any glyph,
/// because that is what a menu row takes.
#[derive(Clone, PartialEq)]
pub struct Agent {
    pub name: SharedString,
    pub icon: Option<Icon>,
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
    Draft(u64, String),
    /// The message, and the pictures going with it.
    Submit(String, Vec<Attachment>),
    Cancel,
    Reconnect,
    Terminal,
    Changes,
    Files,
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

/// What goes to the agent: the quote as markdown above the message, or the
/// message alone.
///
/// Every line marked, blank ones included — a blockquote broken by an unmarked
/// blank line is two blockquotes with the rest of the passage between them.
fn quoted(quote: Option<String>, message: &str) -> String {
    let Some(quote) = quote else {
        return message.to_owned();
    };
    let quoted: String = quote
        .lines()
        .map(|line| match line.trim().is_empty() {
            true => ">".to_owned(),
            false => format!("> {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{quoted}\n\n{message}")
}

pub struct Composer {
    field: Entity<TextField>,
    session: Option<u64>,
    local_draft: String,
    saved_attachments: HashMap<Option<u64>, Vec<Attachment>>,
    /// Pictures pasted or dropped, sent with the next message.
    attachments: Vec<Attachment>,
    /// What was picked out of the transcript to answer, sent as a blockquote
    /// above the message. Kept beside the draft rather than written into the
    /// field: a quote is a thing to take back off in one press, and text in the
    /// field is text to delete by hand.
    quote: Option<String>,
    saved_quotes: HashMap<Option<u64>, String>,
    /// Which of them is open in the lightbox.
    preview: Option<usize>,
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
    /// Whether the session tools are on offer. Off beside a space: all three
    /// of them open the window's own panels, which a space divides the room
    /// for — see [`crate::view::arrangement`].
    tools: bool,
    activity: Option<Activity>,
    activity_open: bool,
    activity_frame: std::rc::Rc<std::cell::RefCell<bezel::agent::orbs::engine::Frame>>,
    activity_tick: Option<gpui::Task<()>>,
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
    menu_pressed: bool,
    tools_menu: bool,
    tools_cursor: Cursor,
    /// Where that menu is being worked: which of its rows is live, and which
    /// of them has its own panel down. One cursor for both devices, so a
    /// submenu can only ever hang off the row the pointer is on.
    cursor: Cursor,
    /// The switch whose options are open as a flat list, in place of the menu
    /// — see [`SUBMENU_ROWS`].
    picking: Option<usize>,
    picking_cursor: Cursor,
    picking_scroll: ScrollHandle,
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
        // Subscribed rather than observed. A field notifies on its own caret
        // blink, and `reread` refilters — which re-enters the list at the top,
        // so the highlight walked back to the first row twice a second while
        // the pointer sat on another one. `Changed` and `Moved` between them
        // are every reason the picker has to be re-read, and neither of them
        // is a blink.
        cx.subscribe(
            &field,
            |composer: &mut Self, _, event: &FieldEvent, cx| match event {
                FieldEvent::Changed => {
                    composer.reread(cx);
                    if let Some(id) = composer.session {
                        cx.emit(ComposerEvent::Draft(
                            id,
                            composer.field.read(cx).content().to_string(),
                        ));
                    }
                }
                FieldEvent::Moved => composer.reread(cx),
            },
        )
        .detach();
        Self {
            field,
            session: None,
            local_draft: String::new(),
            saved_attachments: HashMap::new(),
            attachments: Vec::new(),
            quote: None,
            saved_quotes: HashMap::new(),
            preview: None,
            command: None,
            filter: popover::Filter::new(Vec::new()),
            commands: Vec::new(),
            scroll: ScrollHandle::new(),
            streaming: false,
            tools: true,
            activity: None,
            activity_open: false,
            activity_frame: Default::default(),
            activity_tick: None,
            agents: Vec::new(),
            agent: None,
            switches: Vec::new(),
            usage: None,
            menu: false,
            menu_pressed: false,
            tools_menu: false,
            tools_cursor: Cursor::default(),
            cursor: Cursor::default(),
            picking: None,
            picking_cursor: Cursor::default(),
            picking_scroll: ScrollHandle::new(),
        }
    }

    pub fn session(&self) -> Option<u64> {
        self.session
    }

    pub fn set_session(&mut self, id: Option<u64>, draft: &str, cx: &mut Context<Self>) {
        if self.session == id {
            return;
        }
        if self.session.is_none() {
            self.local_draft = self.field.read(cx).content().to_string();
        }
        self.saved_attachments
            .insert(self.session, std::mem::take(&mut self.attachments));
        if let Some(quote) = self.quote.take() {
            self.saved_quotes.insert(self.session, quote);
        }
        self.session = id;
        self.activity_open = false;
        self.attachments = self.saved_attachments.remove(&id).unwrap_or_default();
        self.quote = self.saved_quotes.remove(&id);
        self.preview = None;
        let draft = if id.is_none() {
            self.local_draft.clone()
        } else {
            draft.to_owned()
        };
        self.field
            .update(cx, |field, cx| field.set_content(draft, cx));
        self.reread(cx);
        cx.notify();
    }

    pub fn restore_queued(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let (text, attachments) = crate::model::media::detach(&text);
        self.attachments
            .splice(0..0, attachments.into_iter().map(Attachment::File));
        self.preview = None;
        let draft = self.field.read(cx).content().to_string();
        let content = if text.is_empty() {
            draft
        } else if draft.is_empty() {
            text
        } else {
            format!("{text}\n\n{draft}")
        };
        self.field
            .update(cx, |field, cx| field.set_content(content, cx));
        self.reread(cx);
        window.focus(&self.focus_handle(cx), cx);
        cx.notify();
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

    pub fn set_tools(&mut self, tools: bool, cx: &mut Context<Self>) {
        if self.tools != tools {
            self.tools = tools;
            cx.notify();
        }
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
        self.attachments.is_empty() && self.field.read(cx).content().trim().is_empty()
    }

    /// Take pictures off the clipboard ahead of the field, which would paste
    /// only the text beside them — a file copied in Finder is its name.
    /// A clipboard with no picture on it is left to the field.
    fn paste(&mut self, _: &input::Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let before = self.attachments.len();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::Image(image) => self
                    .attachments
                    .push(Attachment::Bytes(Arc::new(image.clone()))),
                ClipboardEntry::ExternalPaths(paths) => self.attach(paths),
                _ => {}
            }
        }
        if self.attachments.len() > before {
            cx.stop_propagation();
            cx.notify();
        }
    }

    /// Take the pictures among dropped files. Called by the session pane, whose
    /// whole area is the drop target — the composer alone is a narrow strip to
    /// aim a drag at.
    pub fn drop_paths(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        self.attach(paths);
        cx.notify();
    }

    fn attach(&mut self, paths: &ExternalPaths) {
        self.attachments.extend(
            paths
                .paths()
                .iter()
                .filter(|path| markdown::is_image(&path.to_string_lossy()))
                .cloned()
                .map(Attachment::File),
        );
    }

    /// Answer this. The transcript hands over what was picked out of it — see
    /// [`crate::view::component::transcript`] — and the composer holds it until
    /// the message it belongs to is sent.
    ///
    /// One at a time: a second quote replaces the first. A message answering
    /// two places at once is one nobody writes, and a stack of them is a stack
    /// to manage before a word is typed.
    pub fn quote(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.quote = Some(text);
        cx.notify();
    }

    /// The quote waiting above the message: one line of what is being answered,
    /// and the way to drop it.
    fn quote_row(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let quote = self.quote.as_ref()?;
        let line = quote.lines().next().unwrap_or_default().trim().to_owned();
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .pl(px(8.))
                .pr(px(4.))
                .py(px(4.))
                .rounded(px(Theme::control_radius()))
                .bg(theme.element_hover)
                // The mark a blockquote is drawn with, so the row says what it
                // will become rather than naming it.
                .child(div().w(px(2.)).h(px(14.)).flex_none().bg(theme.text_faint))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_muted)
                        .child(line),
                )
                .child(
                    theme
                        .ghost("composer-quote-drop")
                        .flex_none()
                        .p(px(2.))
                        .tooltip(|window, cx| Tooltip::text("Drop the quote", window, cx))
                        .child(
                            icons::icon(icons::notifications::X)
                                .size(px(12.))
                                .text_color(theme.text_faint),
                        )
                        .on_click(cx.listener(|composer, _, _, cx| {
                            composer.quote = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// The pictures waiting to go with the message, each with a way to take it
    /// back off.
    fn tray(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.attachments.is_empty() {
            return None;
        }
        let thumbs = self.attachments.iter().enumerate().map(|(ix, attachment)| {
            // Unclipped, so the remove button can sit on the corner: half on
            // the picture, half off it.
            div()
                .relative()
                .size(px(THUMB))
                .child(
                    div()
                        .id(("composer-attachment", ix))
                        .size_full()
                        .rounded(px(THUMB_RADIUS))
                        .overflow_hidden()
                        .cursor_pointer()
                        .on_click(cx.listener(move |composer, _, _, cx| {
                            composer.preview = Some(ix);
                            cx.notify();
                        }))
                        .child(
                            picture(attachment)
                                .size_full()
                                .rounded(px(THUMB_RADIUS))
                                .object_fit(ObjectFit::Cover),
                        ),
                )
                // Paint the border above the image on the glass surface.
                .child(surface::layered(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .rounded(px(THUMB_RADIUS))
                        .border_1()
                        .border_color(theme.border),
                ))
                // Its own layer: inside the glass card every primitive shares
                // one draw order, and a picture paints over quads and icons.
                .child(surface::layered(
                    disc(theme, ("composer-attachment-remove", ix), REMOVE)
                        .absolute()
                        .top(px(-REMOVE / 2.))
                        .right(px(-REMOVE / 2.))
                        .tooltip(|window, cx| Tooltip::text("Remove image", window, cx))
                        .on_click(cx.listener(move |composer, _, _, cx| {
                            if ix < composer.attachments.len() {
                                composer.attachments.remove(ix);
                            }
                            composer.preview = None;
                            cx.notify();
                        })),
                ))
        });
        Some(
            div()
                // Room above for the half of each remove button past its
                // corner, and between the pictures and the line under them.
                .pt(px(REMOVE / 2. + 4.))
                .pr(px(REMOVE / 2.))
                .pb(px(8.))
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(12.))
                .children(thumbs)
                .into_any_element(),
        )
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
        if content.trim().is_empty() && self.attachments.is_empty() {
            return;
        }
        self.field.update(cx, |field, cx| field.clear(cx));
        self.command = None;
        cx.emit(ComposerEvent::Submit(
            quoted(self.quote.take(), &content),
            std::mem::take(&mut self.attachments),
        ));
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
        if self.preview.take().is_some() {
            // The open picture is the outermost thing there is.
        } else if self.tools_menu {
            self.tools_menu = false;
        } else if self.cursor.ascend() {
            // A submenu shuts before the menu holding it — one press, one level.
        } else if self.menu || self.picking.is_some() {
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
            div()
                .relative()
                .child(
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
                    // The card sizes to its widest row, and a row is a sentence — so
                    // without this it opens as wide as the window lets it. See
                    // [`root::composer_width`].
                    .max_w(px(root::composer_width()))
                    .max_h(px(PICKER_HEIGHT))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll),
                )
                .child(scrollbars::Overlay::new(
                    "composer-commands-bar",
                    &self.scroll,
                    bezel::gpui::Axis::Vertical,
                ))
                .into_any_element(),
            None,
        ))
    }

    /// Shut the menu, and whatever it had a panel down over.
    fn close_menu(&mut self) {
        self.menu = false;
        self.cursor.clear();
        self.picking = None;
        self.picking_cursor.clear();
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
            .debug_selector(|| "composer-agent".into())
            .size(px(root::composer_height()))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|button| button.bg(theme.element_hover))
            .child(match agent.icon {
                Some(icon) => icons::icon(icon)
                    .size(mark)
                    .text_color(theme.text_muted)
                    .into_any_element(),
                // A slot the catalog has no mark for still has to open the menu.
                None => icons::icon(icons::development::Bot)
                    .size(mark)
                    .text_color(theme.text_muted)
                    .into_any_element(),
            })
            // Outside-click dismissal runs before the trigger's click handler.
            .capture_any_mouse_down(cx.listener(|composer, _, _, _| {
                composer.menu_pressed = composer.menu || composer.picking.is_some();
            }))
            .on_click(cx.listener(|composer, _, _, cx| {
                composer.tools_menu = false;
                composer.menu = !(std::mem::take(&mut composer.menu_pressed) || composer.menu);
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
        if let Some(row) = self.picking {
            return self.options_card(row, theme, cx);
        }
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
            move |composer, hit, window, cx| composer.hit(&rows, hit, window, cx),
        )
        .child(popover::divider())
        .child(self.usage_row(theme));
        Some(popover::anchored_menu_above(
            "composer-menu",
            card.into_any_element(),
            None,
        ))
    }

    /// A long switch's options, one level, clamped at [`PICKER_HEIGHT`] and
    /// scrolled — the command picker's shape.
    fn options_card(
        &self,
        row: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let switch = self.switches.get(row)?;
        let items: Vec<Item> = switch
            .options
            .iter()
            .map(|option| {
                Item::action(option.name.clone())
                    .checked(switch.current.as_ref() == Some(&option.id))
            })
            .collect();
        let rows = items.clone();
        let card = menu::card(
            theme,
            "composer-options",
            &items,
            &self.picking_cursor,
            cx,
            move |composer, hit, window, cx| match hit {
                Hit::Point(path) => {
                    if composer.picking_cursor.point_at(&rows, &path) {
                        cx.notify();
                    }
                }
                Hit::Choose(path) => {
                    let Some(&at) = path.first() else { return };
                    composer.close_menu();
                    window.focus(&composer.focus_handle(cx), cx);
                    if let Some(switch) = composer.switches.get(row)
                        && let Some(option) = switch.options.get(at)
                    {
                        cx.emit(ComposerEvent::Switch(switch.id.clone(), option.id.clone()));
                    }
                    cx.notify();
                }
                Hit::Dismiss => {
                    composer.close_menu();
                    window.focus(&composer.focus_handle(cx), cx);
                    cx.notify();
                }
            },
        )
        .id("composer-options-list")
        .max_h(px(PICKER_HEIGHT))
        .overflow_y_scroll()
        .track_scroll(&self.picking_scroll);
        Some(popover::anchored_menu_above(
            "composer-options",
            div()
                .relative()
                .child(card)
                .child(scrollbars::Overlay::new(
                    "composer-options-bar",
                    &self.picking_scroll,
                    bezel::gpui::Axis::Vertical,
                ))
                .into_any_element(),
            None,
        ))
    }

    /// The menu's rows: whatever the live session offers, one submenu each.
    /// Each switch carries the value it is on in its own name — the reason to
    /// open one of these is as often to read what it is set to as to change it,
    /// and a submenu row has one line to say both on.
    ///
    /// No agents here. A session is bound to the process serving it, so picking
    /// one would open a session beside this one rather than change this one —
    /// which is the sidebar's `+`, and belongs where sessions are made.
    ///
    /// No leading glyphs either: these rows are words, and one icon among them
    /// would open an empty gutter down the menu's left.
    fn menu_items(&self) -> Vec<Item> {
        self.switches
            .iter()
            .map(|switch| {
                if switch.options.len() > SUBMENU_ROWS {
                    return Item::action(set_to(&switch.name, self.value_of(switch)))
                        .with_keystroke("›");
                }
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
            })
            .collect()
    }

    /// What the pointer did to that menu. A path is one row per level — the
    /// top-level row, then which of its alternatives — so reading one is
    /// [`Self::menu_items`] taken backwards.
    fn hit(&mut self, items: &[Item], hit: Hit, window: &mut Window, cx: &mut Context<Self>) {
        match hit {
            Hit::Point(path) => {
                if self.cursor.point_at(items, &path) {
                    cx.notify();
                }
            }
            Hit::Choose(path) => {
                if let [row] = path[..] {
                    // A long switch: its options open as their own list.
                    self.close_menu();
                    self.picking = Some(row);
                    if let Some(at) = self.switches.get(row).and_then(|switch| {
                        switch
                            .options
                            .iter()
                            .position(|option| switch.current.as_ref() == Some(&option.id))
                    }) {
                        self.picking_scroll.scroll_to_item(at);
                    }
                    cx.notify();
                    return;
                }
                let [row, at] = path[..] else { return };
                // Picking anything shuts the menu: every choice here is the
                // session's, and none of them is made twice in a row.
                self.close_menu();
                // Opening a menu takes the caret off what its rows act on —
                // see bezel's `menu`. Nothing else puts it back, and a
                // composer without the caret takes neither typing nor Enter.
                window.focus(&self.focus_handle(cx), cx);
                if let Some(switch) = self.switches.get(row)
                    && let Some(option) = switch.options.get(at)
                {
                    cx.emit(ComposerEvent::Switch(switch.id.clone(), option.id.clone()));
                }
                cx.notify();
            }
            Hit::Dismiss => {
                self.close_menu();
                window.focus(&self.focus_handle(cx), cx);
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

    /// Show Send only for a draft; keep Stop available throughout a turn.
    fn button(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let streaming = self.streaming;
        if !streaming && self.is_empty(cx) {
            return None;
        }
        let glyph = if streaming {
            div()
                .size(px(root::composer_disc() / 3.))
                .rounded(px(2.))
                .bg(theme.on_solid)
                .into_any_element()
        } else {
            icons::icon(icons::arrows::ArrowUp)
                .size(px(root::composer_disc() / 2.))
                .text_color(theme.on_solid)
                .into_any_element()
        };
        let disc = div()
            .flex_none()
            .size(px(root::composer_disc()))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.solid)
            .cursor_pointer()
            .hover(|s| s.opacity(0.9))
            .child(glyph);
        Some(
            div()
                .id("composer-send")
                .flex_none()
                .tooltip(move |window, cx| {
                    Tooltip::text(
                        if streaming {
                            "Stop response"
                        } else {
                            "Send message (Enter)"
                        },
                        window,
                        cx,
                    )
                })
                .on_click(cx.listener(|composer, _, _, cx| {
                    if composer.streaming {
                        cx.emit(ComposerEvent::Cancel);
                    } else {
                        composer.submit(cx);
                    }
                }))
                .child(disc)
                .into_any_element(),
        )
    }

    fn tools(&self, theme: &Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let button = div()
            .id("composer-tools")
            .size(px(root::composer_height()))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(theme.element_hover))
            .tooltip(|window, cx| Tooltip::text("Session tools", window, cx))
            .on_click(cx.listener(|this, _, _, cx| {
                this.tools_menu = !this.tools_menu;
                this.close_menu();
                this.tools_cursor.clear();
                cx.notify();
            }))
            .child(
                icons::icon(icons::math::Plus)
                    .size(px(18.))
                    .text_color(theme.text_muted),
            )
            .surface(theme, SURFACE);
        let items = vec![
            Item::action("Terminal")
                .with_icon(icons::development::Terminal)
                .with_shortcut(&root::ToggleTerminal, window),
            Item::action("Review")
                .with_icon(icons::development::GitCompare)
                .with_shortcut(&root::OpenReview, window),
            Item::action("Files")
                .with_icon(icons::files::Folder)
                .with_shortcut(&root::OpenFiles, window),
        ];
        let rows = items.clone();
        let popup = self.tools_menu.then(|| {
            menu::card(
                theme,
                "composer-tools-menu",
                &items,
                &self.tools_cursor,
                cx,
                move |this, hit, window, cx| {
                    match hit {
                        Hit::Point(path) => {
                            this.tools_cursor.point_at(&rows, &path);
                        }
                        Hit::Choose(path) => {
                            this.tools_menu = false;
                            match path.as_slice() {
                                [0] => cx.emit(ComposerEvent::Terminal),
                                [1] => cx.emit(ComposerEvent::Changes),
                                [2] => cx.emit(ComposerEvent::Files),
                                _ => {}
                            }
                        }
                        // A menu backed out of leaves the caret where it took
                        // it from, which is the field.
                        Hit::Dismiss => {
                            this.tools_menu = false;
                            window.focus(&this.focus_handle(cx), cx);
                        }
                    }
                    cx.notify();
                },
            )
            .into_any_element()
        });
        div()
            .relative()
            .flex_none()
            .child(button)
            .children(
                popup.map(|card| {
                    popover::anchored_menu_above_end("composer-tools-menu", card, None)
                }),
            )
            .into_any_element()
    }

    fn body(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let picker = self.picker(&theme, cx);
        let tray = self.tray(&theme, cx);
        let radius = px(root::composer_height() / 2.);
        let right_inset = if self.streaming || !self.is_empty(cx) {
            root::COMPOSER_INSET
        } else {
            12.
        };

        div()
            // Ahead of the field's own paste, which only knows text.
            .capture_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::send))
            .on_action(cx.listener(Self::command_next))
            .on_action(cx.listener(Self::command_previous))
            .on_action(cx.listener(Self::command_dismiss))
            // The band occludes the pane behind it, so the pane's own drop
            // target never sees a picture let go over the composer — and a
            // composer holding a few lines of text is most of what is there to
            // aim at. See [`crate::view::detail::footer`].
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.drop_paths(paths, cx)))
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
                    .gap(px(10.))
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
                                    .py(px(root::COMPOSER_INSET))
                                    .pl(px(12.))
                                    .pr(px(right_inset))
                                    .flex()
                                    .flex_col()
                                    .gap(px(root::COMPOSER_INSET))
                                    .children(self.activity_row(&theme, right_inset, cx))
                                    .children(self.quote_row(&theme, cx))
                                    .children(tray)
                                    .child(
                                        div()
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
                                            .children(self.button(&theme, cx)),
                                    )
                                    .surface(&theme, SURFACE),
                            )
                            .children(picker),
                    )
                    .children(self.tools.then(|| self.tools(&theme, window, cx))),
            )
            .children(self.lightbox(window, cx))
    }

    /// The picture opened from its thumb, as large as the window allows.
    /// Closed by its button, a press off it, or escape.
    fn lightbox(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let attachment = self.attachments.get(self.preview?)?;
        let theme = Theme::of(cx).clone();
        let viewport = window.viewport_size();
        let composer = cx.entity().downgrade();
        // Sized here rather than left to `img`, which lays out at the
        // picture's own size and lets a tall one run past `max_h`.
        let (width, height) = (
            viewport.width * PREVIEW_SHARE,
            viewport.height * PREVIEW_SHARE,
        );
        let (width, height) = match dimensions(attachment) {
            Some((w, h)) if w > 0 && h > 0 => {
                let ratio = w as f32 / h as f32;
                match f32::from(width) / f32::from(height) > ratio {
                    true => (height * ratio, height),
                    false => (width, width / ratio),
                }
            }
            _ => (width, height),
        };
        let card = image_preview::frame(
            &theme,
            "composer-preview-close",
            picture(attachment)
                .w(width)
                .h(height)
                .object_fit(ObjectFit::Contain)
                .rounded(px(16.)),
            cx.listener(|composer, _, _, cx| {
                composer.preview = None;
                cx.notify();
            }),
        );
        Some(popover::modal(
            "composer-preview",
            viewport,
            card.into_any_element(),
            move |_, _, cx| {
                composer
                    .update(cx, |composer, cx| {
                        composer.preview = None;
                        cx.notify();
                    })
                    .ok();
            },
        ))
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

#[cfg(test)]
#[path = "../../../tests/unit/composer_drafts.rs"]
mod draft_tests;
