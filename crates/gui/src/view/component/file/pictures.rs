//! The pictures in a rendered document — a markdown file's preview or an
//! article — each opened in another application from a button over it: the
//! remembered application, or a pick from the ones that open its kind.
//!
//! A picture on the web is fetched into [`Assets`] and opened from there; the
//! document keeps pointing at the web. A picture on disk is watched once it is
//! handed over, and repainted when the other application saves it.

use super::external::{self, Application, Target};
use crate::model::{settings::Opens, watch, workspace::Workspace};
use bezel::{
    gpui::{self, AnyElement, App, Context, Entity, Task, WeakEntity, prelude::*, px},
    theme::{ControlSize, Theme},
    ui::{popover, widgets::Buttons as _},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::SystemTime,
};

/// The most a picture fetched off the web may weigh.
#[cfg(feature = "desktop")]
const FETCH_LIMIT: u64 = 25 * 1024 * 1024;

/// Where a picture fetched off the web is kept.
#[derive(Clone, PartialEq)]
pub(crate) enum Assets {
    /// The project's `.cydonia/assets/`, by the project's root.
    Project(PathBuf),
    /// A directory of its own, such as an article's `assets/`.
    Dir(PathBuf),
}

/// Each picture on disk as gpui's asset cache last decoded it, by path. App
/// wide, like that cache: a document drawn after a save finds the picture
/// cached as it was before it.
#[derive(Default)]
struct Loaded(HashMap<PathBuf, Option<Stamp>>);

impl gpui::Global for Loaded {}

pub(crate) struct Pictures {
    /// The directory a relative picture path is joined onto.
    base: PathBuf,
    assets: Assets,
    /// What went wrong opening the last picture, for the host to show.
    pub(crate) error: Option<String>,
    /// What opens a picture, by its lowercase extension. `None` while the
    /// list is being made.
    apps: HashMap<String, Option<Rc<Vec<Application>>>>,
    /// The picture whose list of applications is open, by block.
    menu: popover::Popup<usize>,
    /// Every picture on disk the document has drawn, polled for a save, and
    /// its file as the poll last saw it.
    watching: HashMap<PathBuf, Option<Stamp>>,
    _poll: Task<()>,
}

/// A picture's kind, off the extension of its URL's path.
fn kind(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    Some(Path::new(path).extension()?.to_str()?.to_ascii_lowercase())
}

impl Pictures {
    /// Pictures are checked for a save every `workspace`'s watch delay.
    pub(crate) fn new(
        base: PathBuf,
        assets: Assets,
        workspace: WeakEntity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                // Read per pass: moving the setting takes effect on the next.
                let every = workspace
                    .read_with(cx, |workspace, _| {
                        watch::bounce(workspace.settings.watch_bounce)
                    })
                    .unwrap_or_else(|_| watch::bounce(watch::BOUNCE));
                cx.background_executor().timer(every).await;
                if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            base,
            assets,
            error: None,
            apps: HashMap::new(),
            menu: popover::Popup::default(),
            watching: HashMap::new(),
            _poll: poll,
        }
    }

    /// Reload each watched picture whose file changed and has since held
    /// still for a poll — a save can take several writes.
    fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for (path, last) in &mut self.watching {
            let now = stamp(path);
            if now != *last {
                *last = now;
                continue;
            }
            let loaded = &mut cx.default_global::<Loaded>().0;
            if loaded.get(path) == Some(&now) {
                continue;
            }
            loaded.insert(path.clone(), now);
            let source = gpui::ImageSource::from(path.clone());
            source.remove_asset(cx);
            // Started now rather than at the next frame; the picture keeps
            // its old image until this lands.
            if let gpui::ImageSource::Resource(resource) = &source {
                cx.fetch_asset::<gpui::ImgResourceLoader>(resource);
            }
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// Start watching the picture at `url` if it is on disk.
    fn watch(&mut self, url: &str, cx: &mut App) {
        if url.contains("://") {
            return;
        }
        let path = self.base.join(url);
        if self.watching.contains_key(&path) {
            return;
        }
        let now = stamp(&path);
        cx.default_global::<Loaded>()
            .0
            .entry(path.clone())
            .or_insert(now);
        self.watching.insert(path, now);
    }

    /// Point relative paths at `base` and fetches at `assets`.
    pub(crate) fn place(&mut self, base: PathBuf, assets: Assets) {
        self.base = base;
        self.assets = assets;
    }

    /// The button over each picture, for an editor's or a preview's
    /// `image_overlay`. Built once: it reads `this` each time it is drawn,
    /// and lists what opens a kind of picture the first time one is drawn.
    pub(crate) fn overlay(this: &Entity<Self>) -> markdown::ImageOverlay {
        let view = this.downgrade();
        Rc::new(move |ix, url, _, cx| {
            let pictures = view.upgrade()?;
            if !url.contains("://")
                && !pictures
                    .read(cx)
                    .watching
                    .contains_key(&pictures.read(cx).base.join(url))
            {
                pictures.update(cx, |this, cx| this.watch(url, cx));
            }
            let kind = kind(url)?;
            if !pictures.read(cx).apps.contains_key(&kind) {
                pictures.update(cx, |this, cx| this.list(kind.clone(), cx));
            }
            let apps = pictures.read(cx).apps.get(&kind)?.clone()?;
            pictures.update(cx, |this, cx| this.button(ix, url, apps, cx))
        })
    }

    /// List what opens pictures of `kind`.
    fn list(&mut self, kind: String, cx: &mut Context<Self>) {
        self.apps.insert(kind.clone(), None);
        cx.spawn(async move |this, cx| {
            let listed = cx
                .background_executor()
                .spawn({
                    let kind = kind.clone();
                    async move { external::listed_for(&kind, true) }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let apps = listed.unwrap_or_default();
                this.apps.insert(kind, Some(Rc::new(apps)));
                cx.notify();
            });
        })
        .detach();
    }

    /// Open the picture at `url` in `target`, fetching it first when it is on
    /// the web.
    fn open_picture(&mut self, url: &str, target: Target, cx: &mut Context<Self>) {
        let url = url.to_owned();
        let base = self.base.clone();
        let assets = self.assets.clone();
        cx.spawn(async move |this, cx| {
            let opened = cx
                .background_executor()
                .spawn(async move {
                    let file = match url.contains("://") {
                        true => fetch(&url, &assets)?,
                        false => base.join(&url),
                    };
                    external::open(&file, &target)?;
                    anyhow::Ok(file)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match opened {
                    Ok(_) => {}
                    Err(error) => this.error = Some(format!("Open with failed: {error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

/// What a save changes: when the file was written, and how long it is.
type Stamp = (SystemTime, u64);

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Fetch the picture at `url` into `assets`, named for its bytes.
#[cfg(feature = "desktop")]
fn fetch(url: &str, assets: &Assets) -> anyhow::Result<PathBuf> {
    use anyhow::Context as _;
    use artifact::project::fs::Project;
    use std::time::Duration;

    let dir = match assets {
        Assets::Project(root) => {
            let project = Project::new(root);
            project.init()?;
            project.assets()
        }
        Assets::Dir(dir) => dir.clone(),
    };
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
    crate::model::media::store(&dir, &bytes, extension).context("Could not save the picture")
}

#[cfg(not(feature = "desktop"))]
fn fetch(_: &str, _: &Assets) -> anyhow::Result<PathBuf> {
    anyhow::bail!("Pictures on the web cannot be fetched here")
}

/// The picture button's menu.
fn menu(pictures: &mut Pictures) -> &mut popover::Popup<usize> {
    &mut pictures.menu
}

impl Pictures {
    /// `[icon] App ▾`: the press opens the picture in the app, the chevron
    /// lists the others and the folder.
    fn button(
        &mut self,
        ix: usize,
        url: &str,
        apps: Rc<Vec<Application>>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let opener = external::opener(Opens::Pictures);
        let app = external::current(&apps, &opener)?.clone();
        let url: Rc<str> = url.into();
        let mut split = theme.split_button(
            ("picture-open", ix),
            ("picture-apps", ix),
            app.icon(px(16.)),
            app.name().to_owned(),
            ControlSize::Regular,
        );
        split.main = split.main.on_click(cx.listener({
            let (url, target) = (url.clone(), app.target());
            move |this, _, _, cx| {
                cx.stop_propagation();
                this.open_picture(&url, target.clone(), cx);
            }
        }));
        if external::alone(&apps, &opener) {
            return Some(split.build_alone().into_any_element());
        }
        split.more = popover::menu_trigger_matching(
            split.more,
            menu,
            move |open| *open == ix,
            move |_| ix,
            cx,
        );
        let list = (self.menu.as_open() == Some(&ix)).then(|| {
            let mut card =
                popover::dismiss_on_out(popover::popover_card(&theme).min_w(px(200.)), menu, cx);
            let view = cx.entity().downgrade();
            if external::LISTS {
                for (row, pick) in apps.iter().enumerate() {
                    if opener.hides(pick.path()) {
                        continue;
                    }
                    let (view, url, chosen) = (view.clone(), url.clone(), pick.clone());
                    card = card.child(external::app_row(
                        &theme,
                        ("picture-app", row),
                        pick,
                        move |_, cx| {
                            external::choose(Opens::Pictures, Some(chosen.path().to_owned()));
                            let _ = view.update(cx, |this, cx| {
                                popover::close_popup(this, cx, menu);
                                this.open_picture(&url, chosen.target(), cx)
                            });
                        },
                    ));
                }
            } else {
                card = card.child(
                    external::reveal_row(&theme, ("picture-reveal", ix)).on_click(cx.listener(
                        move |this, _, _, cx| {
                            cx.stop_propagation();
                            popover::close_popup(this, cx, menu);
                            this.open_picture(&url, Target::Folder, cx);
                        },
                    )),
                );
            }
            popover::anchored_menu_below_end("picture-apps-menu", card.into_any_element(), None)
        });
        Some(split.build(list).into_any_element())
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/pictures.rs"]
mod tests;
