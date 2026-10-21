//! The composer: a growing field on a glass card, the agent's slash commands
//! behind `/`, and the project's entries behind `@`.

mod activity;
pub use activity::Activity;

use super::image_preview::{self, disc};
use crate::{
    model::{
        media::Attachment,
        session::{Command, Usage},
        settings::PanelTabs,
    },
    view::{kit::link::mention, window::root},
};
use bezel::{
    gpui::{
        self, AnyElement, App, ClipboardEntry, Context, Entity, EventEmitter, ExternalPaths,
        FocusHandle, Focusable, KeyBinding, ObjectFit, Render, ScrollHandle, SharedString, Window,
        actions, div, img, prelude::*, px,
    },
    theme::{Glass, SurfaceStyle, TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        icons::{self, Icon},
        input::{self, Chip, FieldEvent, Shape, TextField},
        menu::{self, Cursor, Hit, Item},
        popover, scroll as scrollbars,
        surface::{self, Surfaced as _},
        tooltip::Tooltip,
        widgets::Controls as _,
    },
};
use editor::Mention;
use std::{collections::HashMap, sync::Arc};

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

/// The side of a picture waiting in the composer.
const THUMB: f32 = 64.;

const THUMB_RADIUS: f32 = 8.;

/// The side of a thumb's remove button, which sits centred on its corner.
const REMOVE: f32 = 14.;

/// How much of the window an opened picture may take, either way.
const PREVIEW_SHARE: f32 = 0.8;

actions!(
    cydonia_composer,
    [Send, CommandNext, CommandPrevious, CommandDismiss]
);

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
    /// The agent an article's session opens on, offered before there is a
    /// session to switch. Its options are the configured agents by index.
    Agent,
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

/// What a row of the tools menu sends when it is chosen.
type Emit = fn() -> ComposerEvent;

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

/// The text as a markdown blockquote.
///
/// Every line marked, blank ones included — a blockquote broken by an unmarked
/// blank line is two blockquotes with the rest of the passage between them.
pub(crate) fn blockquote(quote: &str) -> String {
    quote
        .lines()
        .map(|line| match line.trim().is_empty() {
            true => ">".to_owned(),
            false => format!("> {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the open picker completes.
enum Completing {
    /// A slash command, from the filter over the agent's commands.
    Command,
    /// An entry link: `at` is the `@`, `rows` what [`mention::source`] gave
    /// for the text after it.
    Mention {
        at: usize,
        rows: Vec<Mention>,
        active: Option<usize>,
    },
}

/// The `@` the caret is completing: one at the start of the text or after
/// whitespace, with no whitespace between it and the caret.
fn mention_at(content: &str, caret: usize) -> Option<usize> {
    let before = &content[..caret];
    let at = before.rfind('@')?;
    let opens_word = before[..at]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    (opens_word && !before[at + 1..].contains(char::is_whitespace)).then_some(at)
}

/// The chips in `text`: each `[label](cydonia://… "chip")` that names an
/// entry, painted as the entry's mark and title.
fn mention_chips(text: &str, cx: &App) -> Vec<Chip> {
    use pulldown_cmark::{Event, Parser, Tag};
    Parser::new(text)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) if &*title == "chip" => {
                let preview = mention::preview(&dest_url, cx)?;
                Some(Chip {
                    range,
                    glyph: preview.glyph?,
                    title: preview.title?,
                })
            }
            _ => None,
        })
        .collect()
}

/// `label` linking `url` as a chip, in the Markdown an article writes for one.
pub(crate) fn mention_chip(label: &str, url: &str) -> String {
    let chip = markdown::Text {
        text: label.to_owned(),
        marks: vec![markdown::MarkSpan {
            range: 0..label.len(),
            mark: markdown::Mark::Mention {
                url: url.to_owned(),
                form: markdown::Form::Chip,
            },
        }],
    };
    let doc = markdown::Doc {
        blocks: vec![markdown::Block::new(markdown::BlockKind::Paragraph(chip))],
    };
    markdown::serialize(&doc).trim_end().to_owned()
}

/// A round button beside the pill: the agent's mark, the tools button, and
/// the chevron in its place. Its glyph and its surface are the caller's.
fn side_button(id: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .size(px(root::composer_height()))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(theme.element_hover))
}

/// `row` ruled off from the field under it, the rule running through the
/// pill's padding while its content keeps the pill's inset.
fn ruled_off(row: gpui::Div, theme: &Theme, right_inset: f32) -> gpui::Div {
    row.ml(px(-12.))
        .mr(px(-right_inset))
        .pl(px(12.))
        .pr(px(right_inset))
        .pb(px(6.))
        .border_b_1()
        .border_color(theme.border.opacity(0.7))
}

pub struct Composer {
    field: Entity<TextField>,
    session: Option<u64>,
    local_draft: String,
    saved_attachments: HashMap<Option<u64>, Vec<Attachment>>,
    /// Pictures pasted or dropped, sent with the next message.
    attachments: Vec<Attachment>,
    /// Which of them is open in the lightbox.
    preview: Option<usize>,
    /// The picker open over the text being typed, or `None`. Derived from the
    /// text on every change rather than stored as a flag: a backspace over the
    /// `/` or `@` has to close the picker, and a flag would have to be told.
    completing: Option<Completing>,
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
    /// The session's last user message, which `up` in an empty field brings
    /// back.
    last_asked: Option<String>,
    /// Whether the session tools are on offer. Off beside a space: all three
    /// of them open the window's own panels, which a space divides the room
    /// for — see [`crate::view::pane::arrangement`].
    tools: bool,
    /// A conversation drawn in the pill over the field, opened by the chevron
    /// in the tools' place: an article's thread.
    thread: Option<gpui::AnyView>,
    thread_open: bool,
    /// Whether there is a field to type into. Off, the composer is the turn's
    /// activity alone, with Stop while one runs.
    input: bool,
    activity: Option<Activity>,
    activity_open: bool,
    activity_frame: std::rc::Rc<std::cell::RefCell<bezel::agent::orbs::engine::Frame>>,
    activity_tick: Option<gpui::Task<()>>,
    /// The configured agents, and which one the session runs on.
    agents: Vec<Agent>,
    agent: Option<usize>,
    /// What the session can be switched between — its mode, its model.
    /// Empty for a session with no agent behind it.
    switches: Vec<Switch>,
    /// Context spent, when the agent counts it.
    usage: Option<Usage>,
    /// The agent mark's menu or the tools menu, whichever is up.
    menu: ContextMenu<Self, ComposerMenu>,
    /// Which of the right panel's tabs the tools menu offers.
    panel_tabs: PanelTabs,
}

/// The composer's two menus.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ComposerMenu {
    /// The agent mark's: what the session can be switched between.
    Agent,
    /// The `+` beside the field: the session's tools.
    Tools,
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
                FieldEvent::Changed(_) => {
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
            preview: None,
            completing: None,
            filter: popover::Filter::new(Vec::new()),
            commands: Vec::new(),
            scroll: ScrollHandle::new(),
            streaming: false,
            last_asked: None,
            tools: true,
            thread: None,
            thread_open: false,
            input: true,
            activity: None,
            activity_open: false,
            activity_frame: Default::default(),
            activity_tick: None,
            agents: Vec::new(),
            agent: None,
            switches: Vec::new(),
            usage: None,
            menu: ContextMenu::default(),
            panel_tabs: PanelTabs::default(),
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
        self.session = id;
        self.activity_open = false;
        self.attachments = self.saved_attachments.remove(&id).unwrap_or_default();
        self.preview = None;
        let draft = if id.is_none() {
            self.local_draft.clone()
        } else {
            draft.to_owned()
        };
        self.set_text(draft, cx);
        self.reread(cx);
        cx.notify();
    }

    /// Put a message that was sent back in the field, ahead of whatever is
    /// typed there.
    pub fn restore(&mut self, text: String, cx: &mut Context<Self>) {
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
        self.set_text(content, cx);
        self.reread(cx);
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
        self.completing = None;
        cx.notify();
    }

    pub fn set_panel_tabs(&mut self, tabs: PanelTabs, cx: &mut Context<Self>) {
        if self.panel_tabs != tabs {
            self.panel_tabs = tabs;
            cx.notify();
        }
    }

    pub fn set_tools(&mut self, tools: bool, cx: &mut Context<Self>) {
        if self.tools != tools {
            self.tools = tools;
            cx.notify();
        }
    }

    /// The conversation the chevron opens over the field. Another one, or
    /// none, closes it.
    pub fn set_thread(&mut self, thread: Option<gpui::AnyView>, cx: &mut Context<Self>) {
        let id = |thread: &Option<gpui::AnyView>| thread.as_ref().map(gpui::AnyView::entity_id);
        if id(&self.thread) == id(&thread) {
            return;
        }
        self.thread = thread;
        self.thread_open = false;
        cx.notify();
    }

    pub fn set_input(&mut self, input: bool, cx: &mut Context<Self>) {
        if self.input != input {
            self.input = input;
            cx.notify();
        }
    }

    pub fn set_streaming(&mut self, streaming: bool, cx: &mut Context<Self>) {
        if self.streaming != streaming {
            self.streaming = streaming;
            cx.notify();
        }
    }

    pub fn set_last_asked(&mut self, last_asked: Option<String>) {
        self.last_asked = last_asked;
    }

    /// The agents on offer, and the one the session is talking to.
    pub fn set_agents(&mut self, agents: &[Agent], current: Option<usize>, cx: &mut Context<Self>) {
        if self.agents == agents && self.agent == current {
            return;
        }
        self.agents = agents.to_vec();
        self.agent = current;
        self.close_menu(cx);
        cx.notify();
    }

    /// What the session can be switched between: the agent's offers, live or
    /// cached.
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
    /// [`crate::view::component::transcript`] — and it lands in the field as a
    /// blockquote paragraph at the caret, which lands after it.
    pub fn quote(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let content = self.field.read(cx).content().clone();
        let caret = self.field.read(cx).cursor().min(content.len());
        let before = content[..caret].trim_end();
        let after = content[caret..].trim_start();
        let lead = match before.is_empty() {
            true => String::new(),
            false => format!("{before}\n\n"),
        };
        let quoted = format!("{lead}{}\n\n", blockquote(text));
        let end = quoted.len();
        self.set_text(quoted + after, cx);
        self.field
            .update(cx, |field, cx| field.select(end..end, cx));
        cx.notify();
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
    /// handler: a `/` opening the first line with no whitespace since, or an
    /// `@` opening a word with no whitespace between it and the caret. Typing,
    /// pasting, arrowing back into the word and deleting the `/` or `@` all
    /// agree without any of them being special-cased.
    fn reread(&mut self, cx: &mut Context<Self>) {
        let content = self.field.read(cx).content().clone();
        let caret = self.field.read(cx).cursor().min(content.len());
        let command = content.starts_with('/')
            && !self.filter.items().is_empty()
            && !content[1..caret].contains(char::is_whitespace);
        self.completing = if command {
            self.filter.refilter(&content[1..caret]);
            Some(Completing::Command)
        } else {
            mention_at(&content, caret).map(|at| {
                let rows = mention::source(&content[at + 1..caret], cx);
                Completing::Mention {
                    at,
                    active: (!rows.is_empty()).then_some(0),
                    rows,
                }
            })
        };
        // Narrowing re-enters the list at the top, and the view it is read
        // through has to go back with it.
        self.reveal();
        cx.notify();
    }

    /// Set the field's text, with its mention chips drawn as chips.
    fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        let text = text.into();
        let chips = mention_chips(&text, cx);
        self.field.update(cx, |field, cx| {
            field.set_content(text, cx);
            field.set_chips(chips, cx);
        });
    }

    /// Replace the typed `/query` with the picked command.
    fn accept(&mut self, item: usize, cx: &mut Context<Self>) {
        let content = self.field.read(cx).content().clone();
        let caret = self.field.read(cx).cursor().min(content.len());
        let picked = format!("{} ", self.filter.items()[item]);
        let rest = content[caret..].to_string();
        self.set_text(picked + &rest, cx);
        self.completing = None;
        cx.notify();
    }

    /// Replace the typed `@query` with a chip linking the picked entry.
    fn accept_mention(&mut self, at: usize, row: &Mention, cx: &mut Context<Self>) {
        let content = self.field.read(cx).content().clone();
        let caret = self.field.read(cx).cursor().min(content.len());
        let link = format!("{} ", mention_chip(&row.label, &row.url));
        let end = at + link.len();
        let text = format!("{}{link}{}", &content[..at], &content[caret..]);
        self.set_text(text, cx);
        self.field
            .update(cx, |field, cx| field.select(end..end, cx));
        self.completing = None;
        cx.notify();
    }

    /// The highlighted row's position in the open picker.
    fn active(&self) -> Option<usize> {
        match self.completing.as_ref()? {
            Completing::Command => self.filter.active(),
            Completing::Mention { active, .. } => *active,
        }
    }

    /// Take the row at `position` in the open picker.
    fn choose(&mut self, position: usize, cx: &mut Context<Self>) {
        match &self.completing {
            Some(Completing::Command) => {
                if let Some(&item) = self.filter.filtered().get(position) {
                    self.accept(item, cx);
                }
            }
            Some(Completing::Mention { at, rows, .. }) => {
                if let Some(row) = rows.get(position).cloned() {
                    self.accept_mention(*at, &row, cx);
                }
            }
            None => {}
        }
    }

    fn point(&mut self, position: usize) {
        match &mut self.completing {
            Some(Completing::Command) => self.filter.set_active(position),
            Some(Completing::Mention { rows, active, .. }) if position < rows.len() => {
                *active = Some(position);
            }
            _ => {}
        }
    }

    /// With no picker open the key is the field's caret motion, so it is
    /// passed on rather than swallowed — except `up` in an empty field, which
    /// brings back the last message sent.
    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        match &mut self.completing {
            Some(Completing::Command) => self.filter.step(delta),
            Some(Completing::Mention { rows, active, .. }) => {
                *active = popover::menu_step(*active, rows.len(), delta);
            }
            None => {
                if delta < 0
                    && self.field.read(cx).content().is_empty()
                    && let Some(text) = self.last_asked.clone()
                {
                    self.set_text(text, cx);
                    cx.notify();
                } else {
                    cx.propagate();
                }
                return;
            }
        }
        self.reveal();
        cx.notify();
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) {
        // `enter` is one key doing two jobs: while the picker is up it takes
        // the highlighted row, exactly as the combobox's does.
        if let Some(position) = self.active() {
            self.choose(position, cx);
            return;
        }
        let content = self.field.read(cx).content().clone();
        if content.trim().is_empty() && self.attachments.is_empty() {
            return;
        }
        self.field.update(cx, |field, cx| field.clear(cx));
        self.completing = None;
        // What was sent is answered in the thread.
        self.thread_open |= self.thread.is_some();
        cx.emit(ComposerEvent::Submit(
            content.to_string(),
            std::mem::take(&mut self.attachments),
        ));
        cx.notify();
    }

    fn send(&mut self, _: &Send, _: &mut Window, cx: &mut Context<Self>) {
        self.submit(cx);
    }

    fn command_next(&mut self, _: &CommandNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn command_previous(&mut self, _: &CommandPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    /// Scroll the highlighted row back inside the clamped card. The card's
    /// children are the picker's rows one for one, so the active position is
    /// the child gpui indexes.
    fn reveal(&self) {
        if let Some(active) = self.active() {
            self.scroll.scroll_to_item(active);
        }
    }

    /// Escape backs out of whatever is happening, outermost first: the agent
    /// menu, then the picker, then the thread, and the turn in flight once
    /// there is nothing left to close.
    fn command_dismiss(&mut self, _: &CommandDismiss, _: &mut Window, cx: &mut Context<Self>) {
        if self.preview.take().is_some() {
            // The open picture is the outermost thing there is.
        } else if self.completing.take().is_some() {
            // The picker shuts.
        } else if self.thread_open {
            self.thread_open = false;
        } else {
            cx.emit(ComposerEvent::Cancel);
        }
        cx.notify();
    }

    /// The picker, resting on the pill's top edge and growing upward from it.
    /// Not dropped from the caret: the composer is already at the bottom of
    /// the window, so a card hanging below the `/` covers the very text being
    /// typed into it. Anchored to the pill rather than the caret, it also
    /// rides up with the box as the field grows.
    fn picker(
        &self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let items: Vec<Item> = match self.completing.as_ref()? {
            Completing::Command => self
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
                .collect(),
            Completing::Mention { rows, .. } => rows
                .iter()
                .map(|row| {
                    let mut item = Item::action(row.label.clone());
                    if let Some(icon) = &row.icon {
                        item = item.with_icon(icon.clone());
                    }
                    match &row.description {
                        Some(description) => item.with_description(description.clone()),
                        None => item,
                    }
                })
                .collect(),
        };
        if items.is_empty() {
            return None;
        }
        // The filter or the open mention is what the highlight lives in — it
        // survives a repaint and the card does not — so the cursor is made
        // from it each frame rather than kept beside it, where the two could
        // disagree.
        let mut cursor = Cursor::default();
        if let Some(active) = self.active() {
            cursor.point_at(&items, &[active]);
        }
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
                        None,
                        window,
                        cx,
                        move |composer, hit, _, cx| match hit {
                            // The pointer moves the same highlight the arrows do, so
                            // Enter always takes the row that is lit.
                            Hit::Point(path) => {
                                if let [row] = path[..] {
                                    composer.point(row);
                                    cx.notify();
                                }
                            }
                            Hit::Choose(path) => {
                                if let [row] = path[..] {
                                    composer.choose(row, cx);
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
                    .map(|el| scrollbars::scrolls(el, scrollbars::Axes::Vertical))
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

    /// Shut whichever menu is up.
    fn close_menu(&mut self, cx: &mut Context<Self>) {
        ContextMenu::close(self, |this| &mut this.menu, cx);
    }

    /// The agent the session runs on, as the mark that opens the rest — the
    /// placeholder already carries its name. Picking one is the app's call to
    /// act on: an ACP session is bound to the process serving it, so the
    /// composer only reports the choice.
    fn chip(
        &self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let agent = self.agent.and_then(|ix| self.agents.get(ix))?.clone();
        let mark = px(root::composer_height() / 2.);
        let button = side_button("composer-agent", theme)
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
            .on_click(cx.listener(|composer, _, window, cx| {
                ContextMenu::toggle(
                    composer,
                    |this| &mut this.menu,
                    ComposerMenu::Agent,
                    Place::Above,
                    |composer, _, _| (composer.menu_rows(), Self::switch),
                    window,
                    cx,
                );
            }));
        let button = ContextMenu::trigger(button, |this| &mut this.menu, ComposerMenu::Agent, cx);
        Some(
            div()
                .relative()
                .flex_none()
                // A surface draws its whole subtree in one layer, so the menu
                // hangs off the positioning parent beside it.
                .child(button.surface(theme, SURFACE))
                .children(self.menu_card(theme, window, cx))
                .into_any_element(),
        )
    }

    /// The agent mark's menu: a row per thing the session can be switched
    /// between, each dropping a panel of its own, and the context meter under
    /// a rule at the bottom. The meter is hung on the card rather than being a
    /// row of it, because it answers back instead of offering a choice.
    fn menu_card(
        &self,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.menu.showing(&ComposerMenu::Agent) {
            return None;
        }
        let card = self
            .menu
            .card("composer-menu", |this| &mut this.menu, window, cx)?
            .child(popover::divider(theme))
            .child(self.usage_row(theme));
        Some(self.menu.place("composer-menu", card))
    }

    /// The menu's rows: whatever the session offers, one submenu each.
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
    fn menu_rows(&self) -> Vec<Item> {
        self.switches
            .iter()
            .map(|switch| {
                Item::submenu(
                    crate::view::kit::menu::set_to(&switch.name, self.value_of(switch)),
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

    /// Pick the option at `path` in the agent menu: one row per level, the
    /// switch and then which of its options — [`Self::menu_rows`] taken
    /// backwards.
    fn switch(&mut self, path: &[usize], _: &mut Window, cx: &mut Context<Self>) {
        if let [row, at] = path[..]
            && let Some(switch) = self.switches.get(row)
            && let Some(option) = switch.options.get(at)
        {
            cx.emit(ComposerEvent::Switch(switch.id.clone(), option.id.clone()));
        }
        cx.notify();
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

    fn tools(&self, theme: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let button = side_button("composer-tools", theme)
            .tooltip(|window, cx| Tooltip::text("Session tools", window, cx))
            .on_click(cx.listener(|this, _, window, cx| {
                ContextMenu::toggle(
                    this,
                    |this| &mut this.menu,
                    ComposerMenu::Tools,
                    Place::AboveEnd,
                    Self::tools_menu,
                    window,
                    cx,
                );
            }));
        let button = ContextMenu::trigger(button, |this| &mut this.menu, ComposerMenu::Tools, cx)
            .child(
                icons::icon(icons::math::Plus)
                    .size(px(18.))
                    .text_color(theme.text_muted),
            )
            .surface(theme, SURFACE);
        div()
            .relative()
            .flex_none()
            .child(button)
            .children(self.menu.render_for(
                &ComposerMenu::Tools,
                "composer-tools-menu",
                |this| &mut this.menu,
                window,
                cx,
            ))
            .into_any_element()
    }

    /// The tools menu: the right panel's tabs this session can open, and what
    /// choosing one asks for.
    #[allow(clippy::type_complexity)]
    fn tools_menu(
        &mut self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> (
        Vec<Item>,
        impl Fn(&mut Self, &[usize], &mut Window, &mut Context<Self>) + use<>,
    ) {
        let tabs = self.panel_tabs;
        let (items, events): (Vec<Item>, Vec<Emit>) = [
            Some((
                Item::action("Terminal")
                    .with_icon(icons::development::Terminal)
                    .with_shortcut(&root::ToggleTerminal, window),
                (|| ComposerEvent::Terminal) as Emit,
            )),
            tabs.review.then(|| {
                (
                    Item::action("Review")
                        .with_icon(icons::development::GitCompare)
                        .with_shortcut(&root::OpenReview, window),
                    (|| ComposerEvent::Changes) as Emit,
                )
            }),
            tabs.files.then(|| {
                (
                    Item::action("Files")
                        .with_icon(icons::files::Folder)
                        .with_shortcut(&root::OpenFiles, window),
                    (|| ComposerEvent::Files) as Emit,
                )
            }),
        ]
        .into_iter()
        .flatten()
        .unzip();
        let choose = move |_: &mut Self, path: &[usize], _: &mut Window, cx: &mut Context<Self>| {
            if let [ix] = path
                && let Some(event) = events.get(*ix)
            {
                cx.emit(event());
            }
        };
        (items, choose)
    }

    /// The chevron opening the thread over the field, in the tools button's
    /// place and cut the same.
    fn thread_button(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let open = self.thread_open;
        side_button("composer-thread", theme)
            .tooltip(move |window, cx| {
                Tooltip::text(if open { "Hide chat" } else { "Show chat" }, window, cx)
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.thread_open = !this.thread_open;
                cx.notify();
            }))
            // Up while it is shut, where the panel opens; down to close it.
            .child(
                icons::icon(match open {
                    true => icons::arrows::ChevronDown,
                    false => icons::arrows::ChevronUp,
                })
                .size(px(14.))
                .text_color(theme.text_muted),
            )
            .surface(theme, SURFACE)
            .into_any_element()
    }

    /// The open thread, ruled off from what is under it through the capsule's
    /// padding, as the activity row is.
    fn thread_panel(&self, theme: &Theme, right_inset: f32) -> Option<AnyElement> {
        let thread = self.thread.clone().filter(|_| self.thread_open)?;
        Some(
            ruled_off(div(), theme, right_inset)
                .child(thread)
                .into_any_element(),
        )
    }

    /// The turn's activity alone, and Stop while it runs: the composer with
    /// no field. Nothing between turns.
    fn status(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(row) = self.activity_row(&theme, false, root::COMPOSER_INSET, cx) else {
            return div().into_any_element();
        };
        div()
            .w_full()
            .rounded(px(root::composer_height() / 2.))
            .py(px(root::COMPOSER_INSET))
            .pl(px(12.))
            .pr(px(root::COMPOSER_INSET))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(root::COMPOSER_INSET))
            .child(div().flex_1().min_w_0().child(row))
            .children(self.streaming.then(|| self.button(&theme, cx)).flatten())
            .surface(&theme, SURFACE)
            .into_any_element()
    }

    fn body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.input {
            return self.status(cx);
        }
        let theme = Theme::of(cx).clone();
        let picker = self.picker(&theme, window, cx);
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
                    .children(self.chip(&theme, window, cx))
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
                                    .children(self.thread_panel(&theme, right_inset))
                                    .children(self.activity_row(&theme, true, right_inset, cx))
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
                    .children(match self.thread.is_some() {
                        true => Some(self.thread_button(&theme, cx)),
                        false => self.tools.then(|| self.tools(&theme, window, cx)),
                    }),
            )
            .children(self.lightbox(window, cx))
            .into_any_element()
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
