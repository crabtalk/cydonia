mod app;
mod composer;
mod markdown;
mod session;
mod theme;
mod transcript;

use anyhow::Result;
use cydonia_core::settings;
use gpui::{
    App, AppContext as _, Application, Bounds, KeyBinding, TitlebarOptions, WindowBounds,
    WindowOptions, point, px, size,
};

fn main() -> Result<()> {
    let settings = settings::load()?;
    Application::new().run(move |cx: &mut App| {
        theme::init(cx);
        bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("cydonia".into()),
                    appears_transparent: cfg!(target_os = "macos"),
                    traffic_light_position: Some(point(px(14.), px(14.))),
                }),
                window_min_size: Some(size(px(900.), px(600.))),
                app_id: Some("cydonia".into()),
                ..Default::default()
            },
            |window, cx| {
                let app = cx.new(|cx| app::Cydonia::new(settings, cx));
                app.read(cx).composer_focus_handle(cx).focus(window);
                app
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
    Ok(())
}

fn bind_keys(cx: &mut App) {
    use composer::*;
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("Composer")),
        KeyBinding::new("delete", Delete, Some("Composer")),
        KeyBinding::new("left", Left, Some("Composer")),
        KeyBinding::new("right", Right, Some("Composer")),
        KeyBinding::new("shift-left", SelectLeft, Some("Composer")),
        KeyBinding::new("shift-right", SelectRight, Some("Composer")),
        KeyBinding::new("cmd-a", SelectAll, Some("Composer")),
        KeyBinding::new("cmd-v", Paste, Some("Composer")),
        KeyBinding::new("cmd-c", Copy, Some("Composer")),
        KeyBinding::new("cmd-x", Cut, Some("Composer")),
        KeyBinding::new("home", Home, Some("Composer")),
        KeyBinding::new("end", End, Some("Composer")),
        KeyBinding::new("cmd-left", Home, Some("Composer")),
        KeyBinding::new("cmd-right", End, Some("Composer")),
        KeyBinding::new("enter", Submit, Some("Composer")),
        KeyBinding::new("shift-enter", Newline, Some("Composer")),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("Composer")),
        KeyBinding::new("escape", app::CancelTurn, Some("Composer")),
        KeyBinding::new("cmd-n", app::NewSession, None),
    ]);
}
