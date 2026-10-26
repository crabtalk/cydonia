//! The agent modal: one identity's blob, name, workspace, program and that
//! program's choices, for a new identity or one already kept. Nothing is
//! written until Save.

use crate::{
    model::{
        identity::{Avatar, Identity},
        session_preferences::{self, Choices, OfferCache},
        settings,
        workspace::Workspace,
    },
    view::{
        kit::{
            dialog::{answers, dialog},
            form,
            switch::{Switch, SwitchId},
        },
        window::{
            reach::InWindow,
            root::{Cydonia, EditAgent},
        },
    },
};
use bezel::{
    agent::{Face, mascot},
    gpui::{
        self, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
        SharedString, Subscription, WeakEntity, Window, actions, div, prelude::*, px,
    },
    theme::Theme,
    ui::{
        combobox::{Combobox, ComboboxEvent},
        input::TextField,
        popover,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons},
    },
};
use cydonia_acp::proto::SessionConfigOptionValue;
use std::path::PathBuf;

/// Claimed on the name field, so `enter` saves.
const AGENT_CONTEXT: &str = "CydoniaAgent";
/// How wide the labels run.
const LABEL_WIDTH: f32 = 72.;
/// How wide the blob is drawn.
const AVATAR_SIZE: f32 = 48.;
/// The room above and below the blob.
const AVATAR_PAD: f32 = 16.;

actions!(cydonia_agent, [SaveAgent]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("enter", SaveAgent, Some(AGENT_CONTEXT))]
}

/// An identity being made or changed, as a modal.
pub(crate) struct AgentModal {
    /// What Save writes, but for the name, which is the field's.
    draft: Identity,
    /// Whether `draft` is kept already.
    kept: bool,
    /// The session a new identity is made from, which saving makes its.
    from: Option<u64>,
    name: Entity<TextField>,
    /// The workspaces on offer, index for index with `workspace`'s rows.
    workspaces: Vec<PathBuf>,
    workspace: Entity<Combobox>,
    /// The installed programs, index for index with `program`'s rows.
    programs: Vec<settings::Agent>,
    program: Entity<Combobox>,
    /// What the program offers in the workspace, one picker each.
    switches: Vec<(Switch, Entity<Combobox>)>,
    /// What was wrong last time. The modal stays open while there is one.
    error: Option<SharedString>,
    workspace_model: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    _pickers: [Subscription; 2],
    _switches: Vec<Subscription>,
}

impl Cydonia {
    /// Raise the modal: on the identity [`EditAgent::id`] names, or on a new
    /// one where it names none.
    pub(crate) fn edit_agent_action(
        &mut self,
        edit: &EditAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.workspace.read(cx).settings.features.sessions {
            return;
        }
        self.commit(cx);
        self.close_menu(cx);
        let workspace = self.workspace.clone();
        let root = cx.entity().downgrade();
        let (id, from) = (edit.id.clone(), edit.from);
        self.toggle_modal(window, cx, |window, cx| {
            AgentModal::new(id.as_deref(), from, workspace, root, window, cx)
        });
    }
}

impl AgentModal {
    fn new(
        id: Option<&str>,
        from: Option<u64>,
        workspace_model: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = workspace_model.read(cx);
        let held = id.and_then(|id| workspace.identity(id)).cloned();
        let kept = held.is_some();
        let programs = workspace.settings.agents.clone();
        let mut workspaces: Vec<PathBuf> = workspace
            .projects
            .iter()
            .map(|project| project.path.clone())
            .collect();
        let seed = from
            .filter(|_| !kept)
            .and_then(|session| workspace.session(session, cx))
            .map(|chat| {
                let choices =
                    session_preferences::load(&chat.cwd, &chat.entry, chat.record.as_deref());
                Identity::new(
                    &chat.entry.name,
                    chat.cwd.clone(),
                    chat.entry.reference(),
                    choices,
                )
            });
        let draft = held.or(seed).unwrap_or_else(|| {
            let cwd = workspace
                .active_project()
                .map(|project| project.path.clone())
                .or_else(|| workspaces.first().cloned())
                .unwrap_or_default();
            let program = workspace
                .preferred_agent(cx)
                .map(|agent| agent.reference().to_owned())
                .unwrap_or_default();
            Identity::new("", cwd, &program, Choices::default())
        });
        // A kept identity's workspace stays on offer while its project is shut.
        if !draft.workspace.as_os_str().is_empty() && !workspaces.contains(&draft.workspace) {
            workspaces.push(draft.workspace.clone());
        }
        let name = form::field(AGENT_CONTEXT, draft.name.clone(), "name this agent…", cx);
        let workspace = picker(
            workspaces
                .iter()
                .map(|path| path.display().to_string().into())
                .collect(),
            "Workspace",
            workspaces.iter().position(|path| *path == draft.workspace),
            cx,
        );
        let program = picker(
            programs
                .iter()
                .map(|agent| agent.name.clone().into())
                .collect(),
            "Program",
            programs
                .iter()
                .position(|agent| agent.reference() == draft.acp),
            cx,
        );
        let pickers = [
            cx.subscribe_in(&workspace, window, |this, _, event, window, cx| {
                let ComboboxEvent::Selected(at) = event;
                if let Some(path) = this.workspaces.get(*at) {
                    this.draft.workspace = path.clone();
                    this.offer(window, cx);
                }
            }),
            cx.subscribe_in(&program, window, |this, _, event, window, cx| {
                let ComboboxEvent::Selected(at) = event;
                if let Some(agent) = this.programs.get(*at) {
                    this.draft.acp = agent.reference().to_owned();
                    // Another program's choices name options this one lacks.
                    this.draft.choices = Choices::default();
                    this.offer(window, cx);
                }
            }),
        ];
        let mut modal = Self {
            draft,
            kept,
            from: from.filter(|_| !kept),
            name,
            workspaces,
            workspace,
            programs,
            program,
            switches: Vec::new(),
            error: None,
            workspace_model,
            root,
            _pickers: pickers,
            _switches: Vec::new(),
        };
        modal.offer(window, cx);
        modal
    }

    /// Build a picker per switch the program offers in the workspace, on the
    /// draft's choices.
    fn offer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self
            .programs
            .iter()
            .find(|agent| agent.reference() == self.draft.acp)
        else {
            self.switches.clear();
            self._switches.clear();
            cx.notify();
            return;
        };
        let offers = OfferCache::get(cx, &self.draft.workspace, agent).with(&self.draft.choices);
        let mut events = Vec::new();
        self.switches = Switch::of(&offers)
            .into_iter()
            .map(|switch| {
                let current = switch
                    .options
                    .iter()
                    .position(|option| Some(&option.id) == switch.current.as_ref());
                let combobox = picker(
                    switch
                        .options
                        .iter()
                        .map(|option| option.name.clone())
                        .collect(),
                    switch.name.clone(),
                    current,
                    cx,
                );
                let (id, options) = (switch.id.clone(), switch.options.clone());
                events.push(
                    cx.subscribe_in(&combobox, window, move |this, _, event, _, cx| {
                        let ComboboxEvent::Selected(at) = event;
                        if let Some(option) = options.get(*at) {
                            this.choose(&id, option.id.to_string());
                            cx.notify();
                        }
                    }),
                );
                (switch, combobox)
            })
            .collect();
        self._switches = events;
        cx.notify();
    }

    fn choose(&mut self, switch: &SwitchId, value: String) {
        match switch {
            SwitchId::Mode => self.draft.choices.mode = Some(value),
            SwitchId::Config(config) => {
                self.draft.choices.config.insert(
                    config.to_string(),
                    SessionConfigOptionValue::ValueId {
                        value: value.into(),
                    },
                );
            }
            SwitchId::Agent => {}
        }
    }

    fn reroll(&mut self, cx: &mut Context<Self>) {
        self.draft.avatar = Avatar::random();
        cx.notify();
    }

    fn save(&mut self, _: &SaveAgent, _: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).content().trim().to_owned();
        let refused = match () {
            _ if name.is_empty() => Some("Name the agent."),
            _ if self.draft.workspace.as_os_str().is_empty() => {
                Some("Open a project for it to work in.")
            }
            _ if self.draft.acp.is_empty() => Some("Install an agent program first."),
            _ => None,
        };
        if let Some(why) = refused {
            self.error = Some(why.into());
            cx.notify();
            return;
        }
        let identity = Identity {
            name,
            ..self.draft.clone()
        };
        let (kept, from) = (self.kept, self.from);
        let saved = self.workspace_model.update(cx, |workspace, cx| match kept {
            true => {
                workspace.update_identity(identity, cx);
                true
            }
            false => {
                let Some(made) = workspace.create_identity(identity, cx) else {
                    return false;
                };
                if let Some(session) = from {
                    workspace.give_session(session, &made, cx);
                }
                true
            }
        });
        match saved {
            true => cx.emit(DismissEvent),
            false => {
                self.error = Some("The agent could not be written.".into());
                cx.notify();
            }
        }
    }

    /// Ask before removing the kept identity: the delete question takes this
    /// modal's place.
    fn remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (id, name) = (self.draft.id.clone(), self.draft.name.clone());
        self.defer_window(window, cx, move |root, window, cx| {
            root.ask_delete_identity(id, name, window, cx)
        });
    }
}

impl InWindow for AgentModal {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

/// A select over `rows`, on the row at `chosen`.
fn picker(
    rows: Vec<SharedString>,
    placeholder: impl Into<SharedString>,
    chosen: Option<usize>,
    cx: &mut App,
) -> Entity<Combobox> {
    cx.new(|cx| {
        let combobox = Combobox::new(rows, placeholder, cx);
        match chosen {
            Some(at) => combobox.with_selection(at),
            None => combobox,
        }
    })
}

impl EventEmitter<DismissEvent> for AgentModal {}

impl Focusable for AgentModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.name.focus_handle(cx)
    }
}

impl Render for AgentModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let title = match self.kept {
            true => format!("Edit {}", self.draft.name),
            false => "New agent".to_owned(),
        };
        let avatar = div().flex().justify_center().py(px(AVATAR_PAD)).child(
            div()
                .id("agent-avatar")
                .size(px(AVATAR_SIZE))
                .cursor_pointer()
                .tooltip(|window, cx| Tooltip::text("Reroll", window, cx))
                .on_click(cx.listener(|this, _, _, cx| this.reroll(cx)))
                .child(mascot(&Face::from(self.draft.avatar.0), 0.)),
        );
        let fields = [
            form::labelled(&theme, LABEL_WIDTH, "Name", self.name.clone()),
            form::labelled(&theme, LABEL_WIDTH, "Workspace", self.workspace.clone()),
            form::labelled(&theme, LABEL_WIDTH, "Program", self.program.clone()),
        ];
        let switches = self.switches.iter().map(|(switch, combobox)| {
            form::labelled(&theme, LABEL_WIDTH, &switch.name, combobox.clone())
        });
        let remove = self.kept.then(|| {
            theme
                .button("Remove", ButtonStyle::Destructive, None)
                .id("agent-remove")
                .on_click(cx.listener(|this, _, window, cx| this.remove(window, cx)))
        });
        let answers = div()
            .flex()
            .flex_row()
            .items_end()
            .children(remove)
            .child(div().flex_1())
            .child(answers(
                &theme,
                "agent",
                Some(Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
                "Save",
                Box::new(cx.listener(|this, _, window, cx| this.save(&SaveAgent, window, cx))),
            ));
        let dialog = dialog(
            "agent-dialog",
            &theme,
            true,
            window,
            [
                popover::dialog_title(&theme, title).into_any_element(),
                avatar.into_any_element(),
            ]
            .into_iter()
            .chain(fields)
            .chain((!self.switches.is_empty()).then(|| popover::divider(&theme).into_any_element()))
            .chain(switches)
            .chain(
                self.error
                    .clone()
                    .map(|why| form::error(&theme, why).into_any_element()),
            )
            .chain([answers.into_any_element()]),
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        div().on_action(cx.listener(Self::save)).child(dialog)
    }
}
