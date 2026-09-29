//! The pictures in a markdown preview, each opened in another application from
//! a button over it: the remembered application, or a pick from the ones that
//! open its kind.
//!
//! A picture on the web is fetched into the project's `.cydonia/assets/` and
//! opened from there; the file keeps pointing at the web. A picture on disk is
//! watched once it is handed over, and repainted when the other application
//! saves it.

use super::{
    FileView,
    external::{self, Application, Target},
};
use anyhow::Context as _;
use artifact::project::fs::Project;
use bezel::{
    gpui::{self, AnyElement, App, Context, Task, WeakEntity, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset as _},
    ui::{icons, popover},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::RwLock,
    time::{Duration, SystemTime},
};

/// The most a picture fetched off the web may weigh.
const FETCH_LIMIT: u64 = 25 * 1024 * 1024;

/// How often a handed-over picture is checked for a save.
const POLL: Duration = Duration::from_secs(1);

/// `settings.image_app`, held for the overlay, which is built without the
/// workspace in reach.
static APP: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Seed the remembered application from the settings, at startup.
pub(crate) fn init(app: Option<PathBuf>) {
    if let Ok(mut held) = APP.write() {
        *held = app;
    }
}

fn remembered() -> Option<PathBuf> {
    APP.read().ok().and_then(|held| held.clone())
}

#[derive(Default)]
pub(super) struct Pictures {
    /// What opens a picture, by its lowercase extension. `None` while the
    /// list is being made.
    apps: HashMap<String, Option<Rc<Vec<Application>>>>,
    /// The picture whose list of applications is open, by block.
    menu: Option<usize>,
    /// Pictures handed to another application, polled for a save.
    watching: HashMap<PathBuf, Task<()>>,
}

/// A picture's kind, off the extension of its URL's path.
fn kind(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    Some(Path::new(path).extension()?.to_str()?.to_ascii_lowercase())
}

impl FileView {
    /// Start listing the applications for every kind of picture in `doc` not
    /// listed yet.
    pub(super) fn list_picture_apps(&mut self, doc: &markdown::Doc, cx: &mut Context<Self>) {
        for block in &doc.blocks {
            let markdown::BlockKind::Image { url, .. } = &block.kind else {
                continue;
            };
            let Some(kind) = kind(url) else {
                continue;
            };
            if self.pictures.apps.contains_key(&kind) {
                continue;
            }
            self.pictures.apps.insert(kind.clone(), None);
            cx.spawn(async move |this, cx| {
                let listed = cx
                    .background_executor()
                    .spawn({
                        let kind = kind.clone();
                        async move {
                            // The system answers by the extension, for a file
                            // that is there.
                            let probe =
                                std::env::temp_dir().join(format!("cydonia-picture.{kind}"));
                            std::fs::write(&probe, [])?;
                            external::image_applications(&probe)
                        }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    let apps = listed.unwrap_or_default();
                    this.pictures.apps.insert(kind, Some(Rc::new(apps)));
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// The button over each picture, for [`markdown::render::Editing::image_overlay`].
    pub(super) fn picture_overlay(&self, cx: &mut Context<Self>) -> markdown::ImageOverlay {
        let view = cx.entity().downgrade();
        let apps = self.pictures.apps.clone();
        let menu = self.pictures.menu;
        let chosen = remembered();
        Rc::new(move |ix, url, _, cx| {
            let apps = apps.get(&kind(url)?)?.clone()?;
            let app = chosen
                .as_ref()
                .and_then(|chosen| apps.iter().find(|app| app.path() == chosen))
                .or(apps.first())?
                .clone();
            Some(button(
                ix,
                url,
                app,
                apps,
                menu == Some(ix),
                view.clone(),
                cx,
            ))
        })
    }

    /// Open the picture at `url` in `target`, fetching it first when it is on
    /// the web.
    fn open_picture(&mut self, url: &str, target: Target, cx: &mut Context<Self>) {
        self.pictures.menu = None;
        let url = url.to_owned();
        let base = self
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let project = Project::new(&self.root);
        cx.spawn(async move |this, cx| {
            let opened = cx
                .background_executor()
                .spawn(async move {
                    let file = match url.contains("://") {
                        true => fetch(&url, &project)?,
                        false => base.join(&url),
                    };
                    external::open(&file, &target)?;
                    anyhow::Ok((file, !url.contains("://")))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match opened {
                    Ok((file, true)) => this.watch_picture(file, cx),
                    Ok(_) => {}
                    Err(error) => {
                        this.external_error = Some(format!("Open with failed: {error:#}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Repaint `file` each time it is saved from now on.
    fn watch_picture(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        if self.pictures.watching.contains_key(&file) {
            return;
        }
        let path = file.clone();
        let task = cx.spawn(async move |this, cx| {
            let mut seen = modified(&path);
            loop {
                cx.background_executor().timer(POLL).await;
                let now = modified(&path);
                if now == seen {
                    continue;
                }
                seen = now;
                let alive = this.update(cx, |_, cx| {
                    gpui::ImageSource::from(path.clone()).remove_asset(cx);
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        self.pictures.watching.insert(file, task);
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// Fetch the picture at `url` into the project's assets, named for its bytes.
fn fetch(url: &str, project: &Project) -> anyhow::Result<PathBuf> {
    project.init()?;
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .new_agent();
    let bytes = agent
        .get(url)
        .call()?
        .body_mut()
        .with_config()
        .limit(FETCH_LIMIT)
        .read_to_vec()?;
    let format = image::guess_format(&bytes).context("Not a picture")?;
    let extension = format.extensions_str().first().copied().unwrap_or("png");
    crate::model::media::store(&project.assets(), &bytes, extension)
        .context("Could not save the picture")
}

/// `[icon] App ▾`: the press opens the picture in the app, the chevron lists
/// the others.
fn button(
    ix: usize,
    url: &str,
    app: Application,
    apps: Rc<Vec<Application>>,
    open: bool,
    view: WeakEntity<FileView>,
    cx: &App,
) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let url: Rc<str> = url.into();
    let main = div()
        .id(("picture-open", ix))
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(6.))
        .pr(px(8.))
        .py(px(3.))
        .cursor_pointer()
        .hover(|el| el.bg(theme.element_hover))
        .child(app.icon())
        .child(app.name().to_owned())
        .on_click({
            let (view, url, target) = (view.clone(), url.clone(), app.target());
            move |_, _, cx| {
                cx.stop_propagation();
                let _ = view.update(cx, |this, cx| this.open_picture(&url, target.clone(), cx));
            }
        });
    let more = div()
        .id(("picture-apps", ix))
        .flex()
        .items_center()
        .px(px(5.))
        .self_stretch()
        .cursor_pointer()
        .border_l_1()
        .border_color(theme.border)
        .hover(|el| el.bg(theme.element_hover))
        .child(
            icons::icon(icons::arrows::ChevronDown)
                .size(px(12.))
                .text_color(theme.text_muted),
        )
        .on_click({
            let view = view.clone();
            move |_, _, cx| {
                cx.stop_propagation();
                let _ = view.update(cx, |this, cx| {
                    this.pictures.menu = (this.pictures.menu != Some(ix)).then_some(ix);
                    cx.notify();
                });
            }
        });
    let list = open.then(|| {
        let mut card = popover::popover_card(&theme)
            .min_w(px(200.))
            .on_mouse_down_out({
                let view = view.clone();
                move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.pictures.menu = None;
                        cx.notify();
                    });
                }
            });
        for (row, pick) in apps.iter().enumerate() {
            let pick = pick.clone();
            let (view, url) = (view.clone(), url.clone());
            card = card.child(
                popover::menu_row(&theme, false, None)
                    .id(("picture-app", row))
                    .hover(|el| el.bg(theme.element_hover))
                    .child(pick.icon())
                    .child(pick.name().to_owned())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        init(Some(pick.path().to_owned()));
                        let _ = crate::model::settings::set_image_app(pick.path());
                        let _ =
                            view.update(cx, |this, cx| this.open_picture(&url, pick.target(), cx));
                    }),
            );
        }
        popover::anchored_menu_below_end("picture-apps-menu", card.into_any_element(), None)
    });
    div()
        .relative()
        .flex()
        .items_stretch()
        .rounded(px(Theme::control_radius()))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_raised)
        .text_style(TextStyle::Callout)
        .text_color(theme.text)
        .child(main)
        .child(more)
        .children(list)
        .into_any_element()
}
