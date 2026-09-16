use super::*;

#[test]
fn images_keep_send_order_and_are_separate_from_selectable_text() {
    let (doc, images) =
        document("Before\n\n![](/tmp/one.png)\n\nBetween\n\n![](/tmp/two.png)\n\nAfter");
    assert_eq!(images, ["/tmp/one.png", "/tmp/two.png"]);
    assert_eq!(
        markdown::selectable::copied(&doc, markdown::Selection::all(&doc)),
        "Before\nBetween\nAfter"
    );
}

#[gpui::test]
fn clicking_opens_preview_and_horizontal_drag_switches_images(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|_, cx| {
        Gallery::new(
            vec!["one.png".into(), "two.png".into(), "three.png".into()],
            Path::new("/tmp"),
            cx,
        )
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(500.), px(500.)));
    visual.run_until_parked();
    let center = visual.debug_bounds("sent-image").unwrap().center();
    visual.simulate_click(center, gpui::Modifiers::default());
    visual.run_until_parked();
    window
        .update(&mut visual, |gallery, _, _| assert!(gallery.preview))
        .unwrap();
    let center = visual.debug_bounds("sent-image-preview").unwrap().center();
    visual.simulate_mouse_down(center, MouseButton::Left, gpui::Modifiers::default());
    let left = Point {
        x: center.x - px(120.),
        y: center.y,
    };
    visual.simulate_mouse_move(left, Some(MouseButton::Left), gpui::Modifiers::default());
    visual.simulate_mouse_up(left, MouseButton::Left, gpui::Modifiers::default());
    visual.run_until_parked();
    window
        .update(&mut visual, |gallery, _, _| assert_eq!(gallery.selected, 1))
        .unwrap();
    visual.simulate_keystrokes("right");
    visual.run_until_parked();
    window
        .update(&mut visual, |gallery, _, _| assert_eq!(gallery.selected, 2))
        .unwrap();
    visual.simulate_keystrokes("left escape");
    visual.run_until_parked();
    window
        .update(&mut visual, |gallery, _, _| assert!(!gallery.preview))
        .unwrap();
    let center = visual.debug_bounds("sent-image").unwrap().center();
    visual.simulate_mouse_down(center, MouseButton::Left, gpui::Modifiers::default());
    let right = Point {
        x: center.x + px(120.),
        y: center.y,
    };
    visual.simulate_mouse_move(right, Some(MouseButton::Left), gpui::Modifiers::default());
    visual.simulate_mouse_up(right, MouseButton::Left, gpui::Modifiers::default());
    visual.run_until_parked();
    window
        .update(&mut visual, |gallery, _, _| {
            assert_eq!(gallery.selected, 0);
            assert!(!gallery.preview, "dragging must not open the preview");
        })
        .unwrap();
}

#[gpui::test]
fn dots_select_images_and_single_images_have_no_dots(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|_, cx| {
        Gallery::new(
            vec!["one.png".into(), "two.png".into()],
            Path::new("/tmp"),
            cx,
        )
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    let dot = visual.debug_bounds("image-dot-1").unwrap().center();
    visual.simulate_click(dot, gpui::Modifiers::default());
    visual.run_until_parked();
    window
        .update(&mut visual, |gallery, _, cx| {
            assert_eq!(gallery.selected, 1);
            assert!(!gallery.preview);
            gallery.images.truncate(1);
            gallery.selected = 0;
            cx.notify();
        })
        .unwrap();
    visual.run_until_parked();
    assert!(visual.debug_bounds("image-dot-0").is_none());
}

#[gpui::test]
fn preview_close_button_is_above_the_image_and_closes_the_dialog(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(32, 32, image::Rgba([255, 255, 255, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let picture = Arc::new(gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        png.into_inner(),
    ));
    let window = cx.add_window(|_, cx| {
        let mut gallery = Gallery::new(vec![], Path::new("/tmp"), cx);
        gallery.images.push(picture.into());
        gallery.preview = true;
        gallery
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(500.), px(500.)));
    visual.run_until_parked();
    let button = visual
        .debug_bounds("sent-image-close")
        .expect("close button is painted");
    let image = visual.debug_bounds("sent-image-preview").unwrap();
    assert!(image.contains(&button.center()));
    visual.simulate_click(button.center(), gpui::Modifiers::default());
    visual.run_until_parked();
    window
        .update(&mut visual, |gallery, _, _| assert!(!gallery.preview))
        .unwrap();
    assert!(visual.debug_bounds("sent-image-close").is_none());
}
