//! The question asked before an app is installed: the reference it is pulled
//! by. The app is applied to the project in front, and opened once it lands.

use crate::{
    model::apps::{Http, oci::Reference},
    view::{
        kit::{
            dialog::{answers, dialog},
            form,
        },
        window::root::Cydonia,
    },
};
use bezel::{
    gpui::{
        self, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
        SharedString, Task, WeakEntity, Window, actions, div, prelude::*,
    },
    theme::Theme,
    ui::{input::TextField, popover},
};
use std::path::PathBuf;

/// Claimed on the dialog's field, so `enter` installs.
const INSTALL_CONTEXT: &str = "CydoniaInstallApp";

actions!(cydonia_install, [CommitInstall]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new(
        "enter",
        CommitInstall,
        Some(INSTALL_CONTEXT),
    )]
}

/// An app asked for and not yet installed, as a modal.
pub(crate) struct InstallApp {
    /// The project it is applied to.
    project: PathBuf,
    reference: Entity<TextField>,
    /// What went wrong last time. The dialog stays open while there is one.
    error: Option<SharedString>,
    /// The pull under way.
    installing: Option<Task<()>>,
    root: WeakEntity<Cydonia>,
}

impl Cydonia {
    pub(crate) fn install_app_action(
        &mut self,
        _: &crate::view::window::root::InstallApp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self
            .workspace
            .read(cx)
            .active_project()
            .map(|open| open.path.clone())
        else {
            return;
        };
        self.commit(cx);
        self.close_menu(cx);
        let root = cx.entity().downgrade();
        self.toggle_modal(window, cx, |_, cx| InstallApp::new(project, root, cx));
    }
}

impl InstallApp {
    fn new(project: PathBuf, root: WeakEntity<Cydonia>, cx: &mut Context<Self>) -> Self {
        Self {
            project,
            reference: form::field(
                INSTALL_CONTEXT,
                SharedString::default(),
                "ghcr.io/owner/app:tag",
                cx,
            ),
            error: None,
            installing: None,
            root,
        }
    }

    fn install(&mut self, _: &CommitInstall, window: &mut Window, cx: &mut Context<Self>) {
        if self.installing.is_some() {
            return;
        }
        let reference = match Reference::parse(self.reference.read(cx).content()) {
            Ok(reference) => reference,
            Err(why) => {
                self.error = Some(format!("{why:#}").into());
                cx.notify();
                return;
            }
        };
        self.error = None;
        let project = self.project.clone();
        self.installing = Some(cx.spawn_in(window, async move |this, cx| {
            let into = project.clone();
            let installed = cx
                .background_executor()
                .spawn(async move { reference.install::<Http>(&into) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.installing = None;
                match installed {
                    Ok(app) => this.landed(&app.reference, window, cx),
                    Err(why) => {
                        this.error = Some(format!("{why:#}").into());
                        cx.notify();
                    }
                }
            });
        }));
        cx.notify();
    }

    /// List the installed app in its project, open it and close the dialog.
    fn landed(&mut self, reference: &str, window: &mut Window, cx: &mut Context<Self>) {
        let project = self.project.clone();
        let _ = self.root.update(cx, |root, cx| {
            root.open_applied(&project, reference, window, cx)
        });
        cx.emit(DismissEvent);
    }
}

impl EventEmitter<DismissEvent> for InstallApp {}

impl Focusable for InstallApp {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.reference.focus_handle(cx)
    }
}

impl Render for InstallApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let answer = match self.installing {
            Some(_) => "Installing…",
            None => "Install",
        };
        let dialog = dialog(
            "install-app-dialog",
            &theme,
            window,
            [
                popover::dialog_title(&theme, "Install app").into_any_element(),
                form::row(&theme, "Reference", self.reference.clone()),
            ]
            .into_iter()
            .chain(
                self.error
                    .clone()
                    .map(|why| form::error(&theme, why).into_any_element()),
            )
            .chain([answers(
                &theme,
                "install-app",
                Some(Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
                answer,
                Box::new(
                    cx.listener(|this, _, window, cx| this.install(&CommitInstall, window, cx)),
                ),
            )
            .into_any_element()]),
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        div().on_action(cx.listener(Self::install)).child(dialog)
    }
}
