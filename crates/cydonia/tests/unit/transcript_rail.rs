#[test]
fn rail_requires_clear_space_in_the_conversation_pane() {
    let required = bezel::ui::scroll::RAIL_ROOM + 2. * super::MARK_PAD;
    assert_eq!(super::rail_room(400.), 0.);
    assert_eq!(super::rail_room(super::CONTENT_MAX_WIDTH), 0.);
    assert!(super::rail_room(super::CONTENT_MAX_WIDTH + 2. * required - 1.) < required);
    assert_eq!(
        super::rail_room(super::CONTENT_MAX_WIDTH + 2. * required),
        required
    );
    // A wide window cannot supply gutter space occupied by the right panel.
    assert!(super::rail_room(1200.) >= required);
    assert!(super::rail_room(1200. - 440.) < required);
}
