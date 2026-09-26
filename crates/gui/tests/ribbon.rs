//! What the article's formatting bar shows, and where it stands.

use bezel::gpui::{Bounds, Pixels, point, px, size};
use cydonia_gui::view::component::ribbon::{formats, keystroke, link, perch};
use editor::{Formatting, Mode};
use markdown::Mark;

fn formatting(marks: Vec<Mark>) -> Formatting {
    Formatting {
        mode: Mode::Blocks,
        marks,
        block: Some("Text".into()),
        fenceable: false,
    }
}

/// The scroll box, and a line inside it.
fn port() -> Bounds<Pixels> {
    Bounds::new(point(px(100.), px(200.)), size(px(600.), px(400.)))
}

fn line(y: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(160.), px(y)), size(px(0.), px(20.)))
}

#[test]
fn the_bar_offers_the_four_marks_markdown_spells() {
    let offered: Vec<Mark> = formats(&formatting(Vec::new()))
        .into_iter()
        .map(|format| format.mark)
        .collect();
    assert_eq!(
        offered,
        vec![Mark::Bold, Mark::Italic, Mark::Strike, Mark::Code]
    );
}

#[test]
fn only_the_marks_the_run_carries_are_lit() {
    let lit: Vec<&str> = formats(&formatting(vec![Mark::Bold, Mark::Code]))
        .into_iter()
        .filter(|format| format.lit)
        .map(|format| format.label)
        .collect();
    assert_eq!(lit, vec!["Bold", "Code"]);
}

/// A link is a mark, and it is not one of the four buttons — it has a URL, and
/// no button can ask for one.
#[test]
fn a_link_over_the_run_lights_none_of_the_marks() {
    let held = formatting(vec![Mark::Link("https://cydonia.sh".into())]);
    assert!(formats(&held).iter().all(|format| !format.lit));
    assert_eq!(link(&held), Some("https://cydonia.sh"));
}

#[test]
fn a_run_with_no_link_has_none_to_edit() {
    assert_eq!(link(&formatting(vec![Mark::Bold])), None);
}

/// Over lines the editor makes a fence rather than an inline span, and the
/// tooltip is the only place that can say so before the click.
#[test]
fn code_is_named_for_what_it_would_make() {
    let inline = formats(&formatting(Vec::new()));
    assert_eq!(inline[3].label, "Code");

    let fenceable = Formatting {
        fenceable: true,
        ..formatting(Vec::new())
    };
    assert_eq!(formats(&fenceable)[3].label, "Code block");
}

/// `cmd-b` is the sidebar's in this app and `cmd-e` is Plain text's, so the
/// editor's bold and code are never reached and the buttons must not claim
/// otherwise.
#[test]
fn the_two_chords_this_app_spent_elsewhere_are_not_advertised() {
    assert_eq!(keystroke(&Mark::Bold), None);
    assert_eq!(keystroke(&Mark::Code), None);
    let (italic, strike) = if cfg!(target_os = "macos") {
        ("⌘I", "⇧⌘X")
    } else {
        ("Ctrl+I", "Ctrl+Shift+X")
    };
    assert_eq!(keystroke(&Mark::Italic).as_deref(), Some(italic));
    assert_eq!(keystroke(&Mark::Strike).as_deref(), Some(strike));
}

#[test]
fn the_bar_stands_above_the_line_it_is_about() {
    let at = perch(port(), line(300.)).expect("a line in view");
    assert_eq!(at, point(px(160.), px(292.)));
}

/// The editor answers where the run last painted whether or not it is still on
/// screen. Scrolled past, the bar would float over words it is not about.
#[test]
fn a_line_scrolled_out_of_the_port_takes_the_bar_with_it() {
    assert_eq!(perch(port(), line(150.)), None);
    assert_eq!(perch(port(), line(700.)), None);
}

/// Half a line showing is not showing: the bar would sit on the pane's edge
/// pointing at a row cut off by it.
#[test]
fn a_line_straddling_the_edge_does_not_carry_the_bar() {
    assert_eq!(perch(port(), line(590.)), None);
}
