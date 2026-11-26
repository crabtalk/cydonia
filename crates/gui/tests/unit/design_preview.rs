use crate::view::design::preview::Preview;
use artifact::web::Viewport;
use bezel::gpui::RenderImage;
use std::sync::Arc;

#[test]
fn artboard_detail_follows_zoom_and_display_scale_without_upsampling() {
    let preview = Preview {
        height: 1600,
        image: Arc::new(RenderImage::new(vec![])),
    };
    assert_eq!(preview.detail_width(Viewport::DESKTOP, 0.25, 2.), 768);
    assert_eq!(preview.detail_width(Viewport::DESKTOP, 1., 1.), 1440);
    assert_eq!(preview.detail_width(Viewport::DESKTOP, 1., 2.), 2880);
    assert_eq!(preview.detail_width(Viewport::DESKTOP, 2., 2.), 2880);
}

#[test]
fn artboard_detail_respects_image_budget_for_tall_artboards() {
    let preview = Preview {
        height: 16_000,
        image: Arc::new(RenderImage::new(vec![])),
    };
    assert_eq!(preview.detail_width(Viewport::DESKTOP, 1., 2.), 737);
}
