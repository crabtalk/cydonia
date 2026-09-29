use super::*;
use crate::model::settings::{self, OpenWith, Opener, Opens};
use anyhow::{Context as _, Result, bail};
#[cfg(target_os = "macos")]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bezel::gpui::{App, Div, ElementId, Stateful};
use bezel::{
    theme::ControlSize,
    ui::{icons, popover, tooltip::Tooltip},
};
use std::{
    process::Command,
    sync::{Arc, RwLock},
};

/// `settings.open_with`, held for the buttons, which are built without the
/// workspace in reach.
static OPEN_WITH: RwLock<OpenWith> = RwLock::new(OpenWith {
    files: Opener {
        app: None,
        hidden: Vec::new(),
    },
    pictures: Opener {
        app: None,
        hidden: Vec::new(),
    },
});

/// Seed what files and pictures open in from the settings, at startup.
pub(crate) fn init(open_with: OpenWith) {
    if let Ok(mut held) = OPEN_WITH.write() {
        *held = open_with;
    }
}

/// What `opens` opens in now.
pub(crate) fn opener(opens: Opens) -> Opener {
    OPEN_WITH
        .read()
        .map(|held| held.get(opens).clone())
        .unwrap_or_default()
}

/// Change what `opens` opens in, here and in `settings.toml`.
pub(crate) fn change(opens: Opens, f: impl FnOnce(&mut Opener)) {
    let Ok(mut held) = OPEN_WITH.write() else {
        return;
    };
    let opener = held.get_mut(opens);
    f(opener);
    let _ = settings::set_opener(opens, opener);
}

/// Make `app` the default, shown in the menu again; `None` is the system's.
pub(crate) fn choose(opens: Opens, app: Option<PathBuf>) {
    change(opens, |opener| {
        if let Some(app) = &app {
            opener.hidden.retain(|hidden| hidden != app);
        }
        opener.app = app;
    });
}

/// Leave `app` out of the menu, or put it back.
pub(crate) fn hide(opens: Opens, app: &Path, hidden: bool) {
    change(opens, |opener| {
        opener.hidden.retain(|held| held != app);
        if hidden {
            opener.hidden.push(app.to_owned());
        }
    });
}

#[cfg(target_os = "macos")]
const APPLICATIONS: &str = r#"
ObjC.import('AppKit');
function run(argv) {
    const workspace = $.NSWorkspace.sharedWorkspace;
    const urls = workspace.URLsForApplicationsToOpenURL($.NSURL.fileURLWithPath(argv[0]));
    const apps = [];
    const seen = new Set();
    function add(path, name, kind, system) {
        if (!path || seen.has(path)) return;
        seen.add(path);
        let icon = '';
        try {
            const image = workspace.iconForFile(path);
            const bitmap = $.NSBitmapImageRep.imageRepWithData(image.TIFFRepresentation);
            icon = ObjC.unwrap(bitmap.representationUsingTypeProperties($.NSBitmapImageFileTypePNG, $({})).base64EncodedStringWithOptions(0));
        } catch (_) {}
        apps.push({name: name, path: path, kind: kind || '', system: !!system, icon: icon});
    }
    const system = workspace.URLForApplicationToOpenURL($.NSURL.fileURLWithPath(argv[0]));
    if (system && !system.isNil()) {
        add(ObjC.unwrap(system.path), ObjC.unwrap(system.lastPathComponent.stringByDeletingPathExtension), '', true);
    }
    // Editors need not register for every source-file extension.
    const zed = workspace.URLForApplicationWithBundleIdentifier('dev.zed.Zed');
    if (zed && !zed.isNil()) add(ObjC.unwrap(zed.path), 'Zed');
    const roots = ['/Applications', ObjC.unwrap($.NSHomeDirectory()) + '/Applications'];
    const editors = ['Zed', 'Zed Preview', 'Visual Studio Code', 'Visual Studio Code - Insiders', 'Cursor', 'Windsurf', 'Sublime Text', 'Nova', 'BBEdit', 'Xcode'];
    for (const root of roots) {
        for (const name of editors) {
            const path = root + '/' + name + '.app';
            if ($.NSFileManager.defaultManager.fileExistsAtPath(path)) add(path, name);
        }
    }
    for (let i = 0; i < urls.count; i++) {
        const url = urls.objectAtIndex(i);
        add(ObjC.unwrap(url.path), ObjC.unwrap(url.lastPathComponent.stringByDeletingPathExtension));
    }
    add('/System/Library/CoreServices/Finder.app', 'Finder', 'folder');
    add('/System/Applications/Utilities/Terminal.app', 'Terminal', 'terminal');
    return JSON.stringify(apps);
}
"#;

/// Every application registered to open the file, the system's default first.
#[cfg(target_os = "macos")]
const IMAGE_APPLICATIONS: &str = r#"
ObjC.import('AppKit');
function run(argv) {
    const workspace = $.NSWorkspace.sharedWorkspace;
    const file = $.NSURL.fileURLWithPath(argv[0]);
    const apps = [];
    const seen = new Set();
    function add(url, system, kind) {
        if (!url || url.isNil()) return;
        const path = ObjC.unwrap(url.path);
        if (seen.has(path)) return;
        seen.add(path);
        let icon = '';
        try {
            const image = workspace.iconForFile(path);
            const bitmap = $.NSBitmapImageRep.imageRepWithData(image.TIFFRepresentation);
            icon = ObjC.unwrap(bitmap.representationUsingTypeProperties($.NSBitmapImageFileTypePNG, $({})).base64EncodedStringWithOptions(0));
        } catch (_) {}
        apps.push({name: ObjC.unwrap(url.lastPathComponent.stringByDeletingPathExtension), path: path, kind: kind || '', system: !!system, icon: icon});
    }
    add(workspace.URLForApplicationToOpenURL(file), true);
    const urls = workspace.URLsForApplicationsToOpenURL(file);
    for (let i = 0; i < urls.count; i++) add(urls.objectAtIndex(i));
    add($.NSURL.fileURLWithPath('/System/Library/CoreServices/Finder.app'), false, 'folder');
    add($.NSURL.fileURLWithPath('/System/Applications/Utilities/Terminal.app'), false, 'terminal');
    return JSON.stringify(apps);
}
"#;

#[derive(Clone, serde::Deserialize)]
pub(crate) struct Application {
    name: String,
    path: PathBuf,
    /// `default`, `folder` or `terminal` for the entries that stand for a way
    /// of opening rather than an application; empty for an application.
    #[serde(default)]
    kind: String,
    /// The one the system opens the file in when nobody picks.
    #[serde(default)]
    system: bool,
    /// Base64 PNG from the macOS lister, decoded into `image`.
    #[cfg(target_os = "macos")]
    #[serde(default)]
    icon: String,
    #[serde(skip)]
    image: Option<Arc<gpui::Image>>,
}

impl Application {
    /// Whether it can be remembered as the default and hidden from a menu:
    /// anything but the system default.
    pub(crate) fn remembered(&self) -> bool {
        self.kind != "default"
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn target(&self) -> Target {
        match self.kind.as_str() {
            "default" => Target::Default,
            "folder" => Target::Folder,
            "terminal" => Target::Terminal,
            _ => Target::Application(self.path.clone()),
        }
    }

    pub(crate) fn icon(&self, size: gpui::Pixels) -> gpui::AnyElement {
        if let Some(image) = &self.image {
            gpui::img(image.clone()).size(size).into_any_element()
        } else {
            icons::icon(icons::files::File)
                .size(size)
                .into_any_element()
        }
    }
}

#[derive(Default)]
pub(super) struct Menu {
    popup: popover::Popup<()>,
    loaded: bool,
    loading: bool,
    apps: Vec<Application>,
}

#[derive(Clone)]
pub(crate) enum Target {
    Application(PathBuf),
    Default,
    Terminal,
    Folder,
}

#[cfg(target_os = "macos")]
fn open_command(file: &Path, target: &Target) -> Command {
    let mut command = Command::new("/usr/bin/open");
    let path = match target {
        Target::Application(app) => {
            command.arg("-a").arg(app);
            file
        }
        Target::Default => file,
        Target::Terminal => {
            command.args(["-a", "Terminal"]);
            file.parent().unwrap_or(file)
        }
        Target::Folder => {
            command.arg("-R");
            file
        }
    };
    command.arg("--").arg(path);
    command
}

/// Off macOS there is no terminal to name: [`Target::Terminal`] opens the
/// file's folder the way [`Target::Default`] opens a file.
#[cfg(not(target_os = "macos"))]
fn open_command(file: &Path, target: &Target) -> Command {
    let folder = file.parent().unwrap_or(file);
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = match target {
        Target::Application(app) => {
            let mut command = Command::new(app);
            command.arg(file);
            command
        }
        #[cfg(windows)]
        Target::Folder => {
            use std::os::windows::process::CommandExt as _;
            let mut command = Command::new("explorer.exe");
            // Explorer reads `/select,` and the path as one argument, and
            // takes the path in quotes of its own.
            command.raw_arg(format!("/select,\"{}\"", file.display()));
            command
        }
        #[cfg(not(windows))]
        Target::Folder => opener(folder),
        Target::Default => opener(file),
        Target::Terminal => opener(folder),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// The desktop's own way to open `path` with whatever it is associated with.
#[cfg(not(target_os = "macos"))]
fn opener(path: &Path) -> Command {
    let mut command = Command::new(if cfg!(windows) {
        "explorer.exe"
    } else {
        "xdg-open"
    });
    command.arg(path);
    command
}

fn output(result: std::process::Output) -> Result<String> {
    if !result.status.success() {
        bail!("{}", String::from_utf8_lossy(&result.stderr).trim());
    }
    Ok(String::from_utf8(result.stdout)?
        .trim_end_matches('\n')
        .to_owned())
}

#[cfg(target_os = "macos")]
fn applications(file: &Path) -> Result<Vec<Application>> {
    let json = output(
        Command::new("/usr/bin/osascript")
            .args(["-l", "JavaScript", "-e", APPLICATIONS])
            .arg(file)
            .output()
            .context("Could not find applications")?,
    )?;
    let mut apps: Vec<Application> = serde_json::from_str(&json)?;
    apps.retain(|app| !app.name.eq_ignore_ascii_case("cydonia"));
    apps.sort_by_cached_key(|app| {
        (
            !app.kind.is_empty(),
            app.name != "Zed",
            app.name.to_lowercase(),
        )
    });
    decode_icons(&mut apps);
    apps.dedup_by(|a, b| a.path == b.path);
    Ok(apps)
}

#[cfg(target_os = "macos")]
fn decode_icons(apps: &mut [Application]) {
    for app in apps {
        app.image = STANDARD
            .decode(std::mem::take(&mut app.icon))
            .ok()
            .filter(|bytes| !bytes.is_empty())
            .map(|bytes| Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes)));
    }
}

/// The applications that open a picture like `file`, the system's default
/// first. `file` need not exist: only its extension is read.
#[cfg(target_os = "macos")]
pub(crate) fn image_applications(file: &Path) -> Result<Vec<Application>> {
    let json = output(
        Command::new("/usr/bin/osascript")
            .args(["-l", "JavaScript", "-e", IMAGE_APPLICATIONS])
            .arg(file)
            .output()
            .context("Could not find applications")?,
    )?;
    let mut apps: Vec<Application> = serde_json::from_str(&json)?;
    apps.retain(|app| !app.name.eq_ignore_ascii_case("cydonia"));
    decode_icons(&mut apps);
    Ok(apps)
}

/// Off macOS nothing lists what opens a picture: the desktop's default only.
#[cfg(not(target_os = "macos"))]
pub(crate) fn image_applications(_: &Path) -> Result<Vec<Application>> {
    Ok(vec![Application {
        name: "Open".to_owned(),
        path: PathBuf::new(),
        kind: "default".to_owned(),
        system: true,
        image: None,
    }])
}

/// Editors found on `PATH`, then the desktop's default for the file. No
/// application icons off macOS.
#[cfg(not(target_os = "macos"))]
fn applications(_: &Path) -> Result<Vec<Application>> {
    const EDITORS: [(&str, &str); 4] = [
        ("Zed", "zed"),
        ("Visual Studio Code", "code"),
        ("Cursor", "cursor"),
        ("Sublime Text", "subl"),
    ];
    let path = std::env::var_os("PATH").unwrap_or_default();
    #[cfg(windows)]
    let find = {
        let pathext = std::env::var("PATHEXT").unwrap_or_default();
        move |program: &str| crate::agent::path::resolve(program, &path, &pathext, |p| p.is_file())
    };
    #[cfg(not(windows))]
    let find = |program: &str| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    };
    let application = |name: &str, path: PathBuf, kind: &str| Application {
        name: name.to_owned(),
        path,
        system: kind == "default",
        kind: kind.to_owned(),
        image: None,
    };
    let mut apps: Vec<Application> = EDITORS
        .into_iter()
        .filter_map(|(name, program)| Some(application(name, find(program)?, "")))
        .collect();
    apps.push(application("Default app", PathBuf::new(), "default"));
    Ok(apps)
}

/// The file view's `Open with` menu.
fn menu(view: &mut FileView) -> &mut popover::Popup<()> {
    &mut view.external_menu.popup
}

/// Whether this platform lists the applications that open a file. Where it
/// does not, a file opens in the system's default or is shown in its folder,
/// and nothing is remembered or hidden.
pub(crate) const LISTS: bool = cfg!(target_os = "macos");

/// What showing a file in its folder is called here.
pub(crate) const REVEAL: &str = if cfg!(target_os = "macos") {
    "Show in Finder"
} else if cfg!(windows) {
    "Show in Explorer"
} else {
    "Open containing folder"
};

/// The menu row that shows a file in its folder.
pub(crate) fn reveal_row(theme: &Theme, id: impl Into<ElementId>) -> Stateful<Div> {
    popover::menu_row(theme, false, None)
        .id(id)
        .hover(|row| row.bg(theme.element_hover))
        .child(REVEAL)
}

/// The application `opener` opens in among `apps`: the one picked, or else
/// the system's. Where nothing is listed, always the system's.
pub(crate) fn current<'a>(apps: &'a [Application], opener: &Opener) -> Option<&'a Application> {
    match &opener.app {
        Some(app) if LISTS => apps.iter().find(|listed| &listed.path == app),
        _ => apps.iter().find(|listed| listed.system),
    }
}

/// Whether `opener` lists a single application among `apps`, which leaves its
/// `Open with` button nothing to pick.
pub(crate) fn alone(apps: &[Application], opener: &Opener) -> bool {
    LISTS && apps.iter().filter(|app| !opener.hides(&app.path)).count() <= 1
}

/// One application in an `Open with` menu: a press opens in it.
pub(crate) fn app_row(
    theme: &Theme,
    id: impl Into<ElementId>,
    app: &Application,
    open: impl Fn(&mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    popover::menu_row(theme, false, None)
        .id(id)
        .hover(|row| row.bg(theme.element_hover))
        .child(app.icon(px(18.)))
        .child(app.name.clone())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            open(window, cx);
        })
}

/// The applications that open a file of kind `extension`, and those that open
/// a picture of it, listed against an empty probe file named for it.
pub(crate) fn listed_for(extension: &str, pictures: bool) -> Result<Vec<Application>> {
    let probe = std::env::temp_dir().join(format!("cydonia-probe.{extension}"));
    std::fs::write(&probe, [])?;
    match pictures {
        true => image_applications(&probe),
        false => applications(&probe),
    }
}

/// Show a path in the file manager: a directory opened, anything else selected in the
/// folder it is in.
pub(crate) fn show(path: &Path) -> Result<()> {
    let target = match path.is_dir() {
        true => Target::Default,
        false => Target::Folder,
    };
    open(path, &target)
}

pub(super) fn open(file: &Path, target: &Target) -> Result<()> {
    // Explorer exits 1 when it has done what it was asked.
    if cfg!(windows) {
        open_command(file, target)
            .spawn()
            .context("Could not open the selected destination")?;
        return Ok(());
    }
    output(
        open_command(file, target)
            .output()
            .context("Could not open the selected destination")?,
    )?;
    Ok(())
}

impl FileView {
    pub(super) fn external_button(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        // Only macOS lists what opens a file; elsewhere the menu is the
        // folder alone.
        if LISTS && !self.external_menu.loaded && !self.external_menu.loading {
            self.external_menu.loading = true;
            let path = self.path.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { applications(&path) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.external_menu.loading = false;
                    this.external_menu.loaded = true;
                    match result {
                        Ok(apps) => {
                            this.external_menu.apps = apps;
                        }
                        Err(error) => {
                            this.external_error =
                                Some(format!("Could not list applications: {error:#}"))
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        let theme = Theme::of(cx).clone();
        let tooltip = if self.opening_external {
            "Opening…"
        } else if self.dirty(cx) {
            "Open externally (saves edits first)"
        } else {
            "Open externally"
        };
        let popup = self.external_menu.popup.as_open().map(|_| {
            let mut card =
                popover::dismiss_on_out(popover::popover_card(&theme).min_w(px(210.)), menu, cx);
            let opener = opener(Opens::Files);
            let view = cx.entity().downgrade();
            for (index, app) in self.external_menu.apps.iter().enumerate() {
                if opener.hides(&app.path) {
                    continue;
                }
                let (view, pick) = (view.clone(), app.clone());
                card = card.child(app_row(
                    &theme,
                    ("external-app", index),
                    app,
                    move |_, cx| {
                        if pick.remembered() {
                            choose(Opens::Files, Some(pick.path.clone()));
                        }
                        let _ = view.update(cx, |this, cx| {
                            popover::close_popup(this, cx, menu);
                            this.open_external(pick.target(), cx);
                        });
                    },
                ));
            }
            if self.external_menu.loading {
                card = card.child(div().p(px(8.)).child("Loading apps…"));
            }
            // Where apps are listed, Finder is one of them.
            if !LISTS {
                card = card.child(reveal_row(&theme, "external-folder").on_click(cx.listener(
                    move |this, _, _, cx| {
                        popover::close_popup(this, cx, menu);
                        this.open_external(Target::Folder, cx);
                    },
                )));
            }
            popover::anchored_menu_above_end("file-open-menu", card.into_any_element(), None)
        });
        let opener = opener(Opens::Files);
        let current = current(&self.external_menu.apps, &opener).cloned();
        let target = match (&current, opener.app.clone()) {
            (Some(app), _) => app.target(),
            (None, Some(chosen)) if LISTS => Target::Application(chosen),
            _ => Target::Default,
        };
        let (icon, name) = match &current {
            Some(app) if LISTS => (app.icon(px(12.)), app.name.clone()),
            _ => (
                icons::icon(icons::glyph::Dock)
                    .size(px(12.))
                    .text_color(theme.text_muted)
                    .into_any_element(),
                "Open".to_owned(),
            ),
        };
        let mut button = theme.split_button(
            "file-open",
            "file-open-with",
            icon,
            name,
            ControlSize::Small,
        );
        button.main = button
            .main
            .tooltip(move |window, cx| Tooltip::text(tooltip, window, cx))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_external(target.clone(), cx);
            }));
        if self.external_menu.loaded && alone(&self.external_menu.apps, &opener) {
            return button
                .build_alone()
                .when(self.opening_external, |button| button.opacity(0.5))
                .into_any_element();
        }
        button.more = popover::menu_trigger(
            button.more.debug_selector(|| "file-open-with".into()),
            menu,
            |_| (),
            cx,
        );
        button
            .build(popup)
            .when(self.opening_external, |button| button.opacity(0.5))
            .into_any_element()
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "../../../../tests/unit/external_apps.rs"]
mod tests;
