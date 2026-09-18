//! Window-independent right-panel tabs and unsaved file buffers.

use super::*;
use bezel::gpui::App;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
enum SavedTab {
    Review,
    Terminal(PathBuf),
    File {
        path: PathBuf,
        draft: Option<(String, String)>,
    },
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct SavedPanel {
    tabs: Vec<SavedTab>,
    active: Option<usize>,
    files_open: bool,
    files_width: f32,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct SavedPanels {
    open: bool,
    /// Absent until the split is dragged: a panel nobody has sized is given a
    /// share of the window, which is not a number to write down.
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<f32>,
    projects: BTreeMap<PathBuf, BTreeMap<String, SavedPanel>>,
}

fn path() -> Option<PathBuf> {
    crate::model::settings::dir()
        .ok()
        .map(|dir| dir.join("right-panels.json"))
}

fn load() -> SavedPanels {
    path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

impl Panel {
    fn snapshot(&self, cx: &App) -> SavedPanel {
        if let Some(saved) = &self.restore_pending {
            return saved.clone();
        }
        SavedPanel {
            tabs: self
                .tabs
                .iter()
                .map(|tab| match &tab.content {
                    Content::Review(_) => SavedTab::Review,
                    Content::Terminal(terminal) => {
                        SavedTab::Terminal(terminal.read(cx).directory.clone())
                    }
                    Content::File(file) => {
                        let file = file.read(cx);
                        SavedTab::File {
                            path: file.path.clone(),
                            draft: file.draft_snapshot(cx),
                        }
                    }
                })
                .collect(),
            active: self.tabs.iter().position(|tab| Some(tab.id) == self.active),
            files_open: self.files_open,
            files_width: self.files_width,
        }
    }

    pub(super) fn restore_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(saved) = self.restore_pending.take() else {
            return;
        };
        let focus = window.focused(cx);
        let cwd = self.cwd.clone();
        let mut active = None;
        for (index, tab) in saved.tabs.into_iter().enumerate() {
            match tab {
                SavedTab::Review => self.review(cx),
                SavedTab::Terminal(directory) => {
                    self.cwd = if directory.is_dir() {
                        directory
                    } else {
                        cwd.clone()
                    };
                    self.terminal(window, cx);
                }
                SavedTab::File { path, draft } => {
                    self.open_file(path, cx);
                    if let Some(draft) = draft
                        && let Some(Content::File(file)) = self.tabs.last().map(|tab| &tab.content)
                    {
                        file.update(cx, |file, cx| file.restore_draft(draft, cx));
                    }
                }
            }
            if saved.active == Some(index) {
                active = self.active;
            }
        }
        self.cwd = cwd;
        self.active = active.or(self.active);
        self.files_width = if saved.files_width.is_finite() {
            saved.files_width.clamp(140., 600.)
        } else {
            220.
        };
        if saved.files_open {
            self.files(window, cx);
        }
        self.focus_pending = false;
        if let Some(focus) = focus {
            window.focus(&focus, cx);
        }
    }
}

impl Cydonia {
    pub(crate) fn restore_panel_layout(&mut self) {
        let saved = load();
        self.changes_open = saved.open;
        self.changes_width = saved
            .width
            .filter(|width| width.is_finite() && *width >= 200.);
    }

    pub(crate) fn save_panel_layout(&mut self, cx: &mut App) {
        let mut saved = load();
        saved.open = self.changes_open;
        saved.width = self.changes_width;
        let active = if self.showing(cx) == Some(Pane::Chat) {
            self.workspace.read(cx).active_id()
        } else {
            None
        };
        for (id, panel) in &self.right_panels {
            let panel = panel.read(cx).snapshot(cx);
            let identity = self.workspace.update(cx, |workspace, cx| {
                workspace.retain_panel_session(*id, active == Some(*id), cx)
            });
            if let Some((cwd, record)) = identity {
                saved.projects.entry(cwd).or_default().insert(record, panel);
            }
        }
        let Some(path) = path() else {
            return;
        };
        if let Ok(bytes) = serde_json::to_vec(&saved)
            && let Some(dir) = path.parent()
            && std::fs::create_dir_all(dir).is_ok()
        {
            let temporary = path.with_extension("json.tmp");
            if std::fs::write(&temporary, bytes).is_ok() {
                let _ = std::fs::rename(temporary, path);
            }
        }
    }
}

pub(super) fn saved_panel(cwd: &std::path::Path, record: &str) -> Option<SavedPanel> {
    load().projects.get(cwd)?.get(record).cloned()
}
