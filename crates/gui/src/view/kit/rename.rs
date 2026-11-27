//! A name typed over what it names, in its place: `enter` keeps it, `escape`
//! drops it, and a press anywhere else keeps it too — the name typed is the
//! name meant.

use crate::{
    model::{
        session::ChatSession,
        workspace::{Entry, Showing, Workspace},
    },
    view::sidebar::{Group, Row},
};
use artifact::kind::Kind;
use bezel::{
    gpui::{
        self, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding, MouseButton,
        Subscription, Window, actions, div, prelude::*,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::input::{Case, TextField},
};

/// Claimed on the field so `enter` keeps the name and `escape` drops it.
const RENAME_CONTEXT: &str = "CydoniaSessionName";

actions!(cydonia_rename, [CommitName, DismissName]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", CommitName, Some(RENAME_CONTEXT)),
        KeyBinding::new("escape", DismissName, Some(RENAME_CONTEXT)),
    ]
}

/// What became of a name being typed.
pub(crate) enum NameEvent {
    /// Kept, as typed.
    Named(String),
    /// Dropped.
    Dismissed,
}

/// What a name field is attached to. Each entry is held by what identifies it
/// — a session, a table's key — never by an index: that moves the moment a
/// neighbour is made or dropped, and the field would follow it onto whichever
/// entry slid underneath.
///
/// No article here: its title is the first line of its own page, which is
/// where it is written — see [`crate::view::article`]. No board either: a
/// board is named by its identity panel — see
/// [`crate::view::pane::tab::Tab::open_info`].
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Renaming {
    Session(u64),
    Canvas(Entry),
    Design(Entry),
    /// One artboard of a design, by its id.
    Artboard(Entry, String),
    Table(Entry),
    /// A space, by its id — auto-named `space-1` until someone gives it a
    /// name of their own.
    Space(String),
    /// An agent identity's display name, by its id. Its handle stays.
    Agent(String),
}

impl Renaming {
    /// How `row` is renamed in a field, for the kinds named that way.
    pub(crate) fn of(row: &Row) -> Option<Self> {
        match row {
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => Some(Self::Session(*id)),
            Row::Entry {
                project,
                showing: Showing::Table(key),
            } => Some(Self::Table(Entry {
                project: *project,
                kind: Kind::Table,
                id: key.clone(),
            })),
            Row::Entry {
                project,
                showing: Showing::Canvas(id),
            } => Some(Self::Canvas(Entry {
                project: *project,
                kind: Kind::Canvas,
                id: id.clone(),
            })),
            Row::Entry {
                project,
                showing: Showing::Design(id),
            } => Some(Self::Design(Entry {
                project: *project,
                kind: Kind::Design,
                id: id.clone(),
            })),
            Row::Group(Group::Space(id)) => Some(Self::Space(id.clone())),
            Row::Group(Group::Agent(id)) => Some(Self::Agent(id.clone())),
            Row::Entry {
                showing: Showing::Article(_) | Showing::Board(_),
                ..
            }
            | Row::App(_)
            | Row::Group(Group::Project(_))
            | Row::Heading(_) => None,
        }
    }

    /// The name it has now, which the field opens holding.
    pub(crate) fn label(&self, workspace: &Workspace, cx: &App) -> String {
        match self {
            Self::Session(id) => workspace
                .session(*id, cx)
                .map(ChatSession::label)
                .unwrap_or_default(),
            Self::Canvas(entry) => workspace
                .canvas_entity(entry)
                .map(|canvas| canvas.read(cx).record.label().to_owned())
                .unwrap_or_default(),
            Self::Design(entry) => workspace
                .design_entity(entry)
                .map(|design| design.read(cx).label().to_owned())
                .unwrap_or_default(),
            Self::Artboard(entry, id) => workspace
                .design_entity(entry)
                .and_then(|design| {
                    let design = design.read(cx);
                    let artboard = design.artboards.iter().find(|held| &held.id == id)?;
                    Some(artboard.title.clone())
                })
                .unwrap_or_default(),
            Self::Table(entry) => workspace
                .table_entity(entry)
                .map(|table| table.read(cx).record.name.clone())
                .unwrap_or_default(),
            Self::Space(id) => workspace
                .space_by_id(id)
                .map(|space| space.name.clone())
                .unwrap_or_default(),
            Self::Agent(id) => workspace
                .identity(id)
                .map(|identity| identity.name.clone())
                .unwrap_or_default(),
        }
    }

    /// Give it `name`.
    pub(crate) fn commit(
        self,
        name: String,
        workspace: &mut Workspace,
        cx: &mut Context<Workspace>,
    ) -> Result<(), String> {
        match self {
            Self::Session(id) => workspace.rename_session(id, name, cx),
            Self::Canvas(entry) => workspace.rename_canvas(&entry, name, cx),
            Self::Design(entry) => {
                let _ = workspace.edit_design(
                    &entry,
                    |design| {
                        design.title = name;
                        Ok(())
                    },
                    cx,
                );
            }
            Self::Artboard(entry, id) => {
                let name = name.trim().to_owned();
                if name.is_empty() {
                    return Ok(());
                }
                workspace
                    .edit_design(
                        &entry,
                        |design| {
                            if let Some(artboard) =
                                design.artboards.iter_mut().find(|held| held.id == id)
                            {
                                artboard.title = name;
                            }
                            Ok(())
                        },
                        cx,
                    )
                    .map_err(|error| error.to_string())?;
            }
            Self::Table(entry) => return workspace.rename_table(&entry, name, cx),
            Self::Space(id) => workspace.rename_space(&id, name, cx),
            Self::Agent(id) => workspace.rename_identity(&id, name, cx),
        }
        Ok(())
    }
}

/// A view's name field and what it renames while it is up.
pub(crate) struct Rename {
    pub(crate) field: Entity<NameField>,
    pub(crate) what: Option<Renaming>,
    workspace: Entity<Workspace>,
    _named: Subscription,
}

impl Rename {
    /// The field for view `V`, which holds it where `held` finds it.
    pub(crate) fn new<V: 'static>(
        workspace: Entity<Workspace>,
        held: fn(&mut V) -> &mut Rename,
        cx: &mut Context<V>,
    ) -> Self {
        let field = cx.new(NameField::new);
        let named = cx.subscribe(&field, move |this, _, event: &NameEvent, cx| {
            let rename = held(this);
            let Some(what) = rename.what.clone() else {
                return;
            };
            match event {
                NameEvent::Named(name) => {
                    let result = rename
                        .workspace
                        .clone()
                        .update(cx, |workspace, cx| what.commit(name.clone(), workspace, cx));
                    match result {
                        Ok(()) => rename.what = None,
                        Err(error) => rename.field.update(cx, |field, cx| {
                            field.error = Some(error);
                            cx.notify();
                        }),
                    }
                }
                NameEvent::Dismissed => rename.what = None,
            }
            cx.notify();
        });
        Self {
            field,
            what: None,
            workspace,
            _named: named,
        }
    }

    /// Put the field on `what`, holding the name it has now.
    pub(crate) fn start(&mut self, what: Renaming, window: &mut Window, cx: &mut App) {
        let label = what.label(self.workspace.read(cx), cx);
        self.field
            .update(cx, |field, cx| field.start(label, Case::Mixed, window, cx));
        self.what = Some(what);
    }
}

/// The field a name is typed in.
pub(crate) struct NameField {
    error: Option<String>,
    field: Entity<TextField>,
}

impl EventEmitter<NameEvent> for NameField {}

impl Focusable for NameField {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

impl NameField {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            error: None,
            field: cx.new(|cx| {
                TextField::new(cx)
                    .with_frame(false)
                    .with_key_context(RENAME_CONTEXT)
                    .with_placeholder("name this session…")
            }),
        }
    }

    /// Put `label` in the field, in `case`, and take the caret to it.
    pub(crate) fn start(
        &mut self,
        label: String,
        case: Case,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        self.field.update(cx, |field, cx| {
            field.set_case(case);
            field.set_content(label, cx);
        });
        window.focus(&self.field.focus_handle(cx), cx);
    }

    fn named(&mut self, cx: &mut Context<Self>) {
        let name = self.field.read(cx).content().to_string();
        cx.emit(NameEvent::Named(name));
    }
}

impl Render for NameField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // It carries its own press: `TextField` does not focus itself, and a
        // press that reached the row would open what is being named out from
        // under the name.
        div()
            .flex_1()
            .min_w_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.field.focus_handle(cx), cx);
                }),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.named(cx)))
            .on_action(cx.listener(|this, _: &CommitName, _, cx| this.named(cx)))
            .on_action(cx.listener(|_, _: &DismissName, _, cx| cx.emit(NameEvent::Dismissed)))
            .child(self.field.clone())
            .when_some(self.error.clone(), |element, error| {
                element.child(
                    div()
                        .text_style(TextStyle::Caption)
                        .text_color(Theme::of(cx).danger)
                        .child(error),
                )
            })
    }
}
