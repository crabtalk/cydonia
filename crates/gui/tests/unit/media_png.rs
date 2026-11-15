use crate::model::media::Png as _;
use bezel::gpui::RenderImage;
use image::{Frame, RgbaImage};

#[test]
fn snapshot_png_preserves_dimensions_and_converts_bgra_to_rgba() {
    let pixels = RgbaImage::from_raw(2, 1, vec![0, 0, 255, 255, 255, 0, 0, 255]).unwrap();
    let snapshot = RenderImage::new(vec![Frame::new(pixels)]);
    let png = snapshot.png().unwrap();
    let decoded = image::load_from_memory(&png).unwrap().into_rgba8();
    assert_eq!(decoded.dimensions(), (2, 1));
    assert_eq!(decoded.as_raw(), &[255, 0, 0, 255, 0, 0, 255, 255]);
}
