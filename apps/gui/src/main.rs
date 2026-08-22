mod app;
mod composer;
mod session;
mod transcript;

use anyhow::Result;
use bezel::{
    gpui::{
        App, AppContext as _, Bounds, Menu, MenuItem, TitlebarOptions, WindowBounds, WindowOptions,
        point, px, size,
    },
    theme::{Theme, appearance},
    ui::{self, focus, icons, input},
};
use cydonia_core::settings;
use gpui::actions;

actions!(cydonia, [Quit]);

fn main() -> Result<()> {
    let settings = settings::load()?;
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx: &mut App| {
            if let Err(err) = ui::register_fonts(cx) {
                eprintln!("font registration failed: {err:?}");
            }
            appearance::init(appearance::AppearanceMode::System, cx);
            markdown::set_highlighter(
                cx,
                |language, code| syntax::highlight(code, language),
                syntax::lang::LANGS.iter().map(|lang| lang.name),
            );
            input::init(cx);
            focus::init(cx);
            composer::init(cx);
            app::init(cx);
            set_menus(cx);

            let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("cydonia".into()),
                        appears_transparent: cfg!(target_os = "macos"),
                        traffic_light_position: Some(point(px(14.), px(14.))),
                    }),
                    // Glass needs a blurred window background to blur into.
                    window_background: Theme::of(cx).window_background_appearance(),
                    window_min_size: Some(size(px(900.), px(600.))),
                    app_id: Some("cydonia".into()),
                    ..Default::default()
                },
                |window, cx| {
                    appearance::observe_window(window, cx).detach();
                    let app = cx.new(|cx| app::Cydonia::new(settings, cx));
                    let focus = app.read(cx).composer_focus_handle(cx);
                    window.focus(&focus, cx);
                    app
                },
            )
            .expect("failed to open window");
            cx.activate(true);
        });
    Ok(())
}

/// Without a menu item `cmd-q` does nothing — a gpui app gets no menu for
/// free, the standard ones come from a nib and there is no nib here.
fn set_menus(cx: &mut App) {
    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
    cx.set_menus(vec![
        Menu::new("cydonia").items([MenuItem::action("Quit", Quit)]),
    ]);
}
