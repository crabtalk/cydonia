use super::*;
use anyhow::{Context as _, Result, bail};
#[cfg(target_os = "macos")]
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bezel::ui::{icons, popover, tooltip::Tooltip};
use std::{process::Command, sync::Arc};

#[cfg(target_os = "macos")]
const APPLICATIONS: &str = r#"
ObjC.import('AppKit');
function run(argv) {
    const workspace = $.NSWorkspace.sharedWorkspace;
    const urls = workspace.URLsForApplicationsToOpenURL($.NSURL.fileURLWithPath(argv[0]));
    const apps = [];
    const seen = new Set();
    function add(path, name, kind) {
        if (!path || seen.has(path)) return;
        seen.add(path);
        let icon = '';
        try {
            const image = workspace.iconForFile(path);
            const bitmap = $.NSBitmapImageRep.imageRepWithData(image.TIFFRepresentation);
            icon = ObjC.unwrap(bitmap.representationUsingTypeProperties($.NSBitmapImageFileTypePNG, $({})).base64EncodedStringWithOptions(0));
        } catch (_) {}
        apps.push({name: name, path: path, kind: kind || '', icon: icon});
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
    add('/System/Library/CoreServices/Finder.app', 'Default app', 'default');
    add('/System/Applications/Utilities/Terminal.app', 'Terminal', 'terminal');
    return JSON.stringify(apps);
}
"#;

#[derive(Clone, serde::Deserialize)]
pub(super) struct Application {
    name: String,
    path: PathBuf,
    /// `default` or `terminal` for the entries that stand for a way of
    /// opening rather than an application; empty for an application.
    #[serde(default)]
    kind: String,
    /// Base64 PNG from the macOS lister, decoded into `image`.
    #[cfg(target_os = "macos")]
    #[serde(default)]
    icon: String,
    #[serde(skip)]
    image: Option<Arc<gpui::Image>>,
}

impl Application {
    fn target(&self) -> Target {
        match self.kind.as_str() {
            "default" => Target::Default,
            "terminal" => Target::Terminal,
            _ => Target::Application(self.path.clone()),
        }
    }

    fn icon(&self) -> gpui::AnyElement {
        if let Some(image) = &self.image {
            gpui::img(image.clone()).size(px(18.)).into_any_element()
        } else {
            icons::icon(icons::files::File)
                .size(px(18.))
                .into_any_element()
        }
    }
}

#[derive(Default)]
pub(super) struct Menu {
    open: bool,
    pressed: bool,
    loaded: bool,
    loading: bool,
    apps: Vec<Application>,
}

#[derive(Clone)]
pub(super) enum Target {
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
    for app in &mut apps {
        app.image = STANDARD
            .decode(std::mem::take(&mut app.icon))
            .ok()
            .filter(|bytes| !bytes.is_empty())
            .map(|bytes| Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes)));
    }
    apps.dedup_by(|a, b| a.path == b.path);
    Ok(apps)
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
    let pathext = std::env::var("PATHEXT").unwrap_or_default();
    let find = |program: &str| match cfg!(windows) {
        true => crate::agent::path::resolve(program, &path, &pathext, |p| p.is_file()),
        false => std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file()),
    };
    let application = |name: &str, path: PathBuf, kind: &str| Application {
        name: name.to_owned(),
        path,
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
        if !self.external_menu.loaded && !self.external_menu.loading {
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
        let popup = self.external_menu.open.then(|| {
            let mut card = popover::popover_card(&theme)
                .min_w(px(210.))
                // Spent on the dismissal, reaching nothing behind the card —
                // the rule [`popover::dismiss_on_out`] states.
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.external_menu.open = false;
                    cx.notify();
                    cx.stop_propagation();
                }));
            for (index, app) in self.external_menu.apps.iter().enumerate() {
                let app = app.clone();
                card = card.child(
                    popover::menu_row(&theme, false, None)
                        .id(("external-app", index))
                        .hover(|row| row.bg(theme.element_hover))
                        .child(app.icon())
                        .child(app.name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.external_menu.open = false;
                            this.open_external(app.target(), cx);
                            cx.notify();
                        })),
                );
            }
            if self.external_menu.loading {
                card = card.child(div().p(px(8.)).child("Loading apps…"));
            }
            card = card.child(popover::divider()).child(
                popover::menu_row(&theme, false, None)
                    .id("external-folder")
                    .hover(|row| row.bg(theme.element_hover))
                    .child("Open in folder")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.external_menu.open = false;
                        this.open_external(Target::Folder, cx);
                        cx.notify();
                    })),
            );
            popover::anchored_menu_above_end("file-open-menu", card.into_any_element(), None)
        });
        div()
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .child(
                div()
                    .id("file-open-with")
                    .debug_selector(|| "file-open-with".into())
                    .w(px(20.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|button| button.bg(theme.element_hover))
                    .when(self.opening_external, |button| button.opacity(0.5))
                    .tooltip(move |window, cx| Tooltip::text(tooltip, window, cx))
                    .child(
                        icons::icon(icons::glyph::Dock)
                            .size(px(14.))
                            .text_color(theme.text_muted),
                    )
                    .capture_any_mouse_down(cx.listener(|this, _, _, _| {
                        this.external_menu.pressed = this.external_menu.open
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.external_menu.open =
                            !(std::mem::take(&mut this.external_menu.pressed)
                                || this.external_menu.open);
                        cx.notify();
                    })),
            )
            .children(popup)
            .into_any_element()
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "../../../../tests/unit/external_apps.rs"]
mod tests;
