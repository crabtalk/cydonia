//! An article's cover: a picture named after the document it sits above.
//!
//! The file being there is the whole of the state. Nothing records that an
//! article has a cover, so the listing cannot disagree with the screen, and a
//! cover deleted in Finder is a cover gone.
//!
//! The seed is in the file name because gpui caches a decoded image against its
//! path — a shuffle that rewrote the same name would repaint the picture it had
//! just replaced.

use anyhow::Result;
use image::imageops::FilterType;
use std::{
    f64::consts::SQRT_2,
    path::{Path, PathBuf},
};

/// What a cover's name begins with. The article's own directory says which
/// document it belongs to, so this only has to tell it from the content and
/// the properties beside it.
const MARK: &str = "cover-";

/// How many pictures there are to land on.
const SEEDS: u64 = 1_000_000;

/// The widest a cover is kept at, generated or imported: what Notion asks for a
/// cover that runs a page's full width, which is what this one does.
///
/// It is the generated picture's declared size as well as the import cap, and
/// both want the same thing. gpui rasterises an SVG once, at twice its declared
/// size and never again — so a picture declared at the band's own width would
/// be resampled up on any window wide enough to stretch it, and a field of
/// squares resampled up is a field of squares with soft edges.
const WIDTH: u32 = 1500;

/// The 5:2 a cover is cut at.
const HEIGHT: u32 = WIDTH * 2 / 5;

/// What a generated cover costs in memory once it is on screen: gpui rasterises
/// an SVG at `SMOOTH_SVG_SCALE_FACTOR` — two — in each direction and keeps the
/// frame as BGRA. An imported picture keeps its own proportions, so it is this
/// only in the width.
pub const RASTER_BYTES: u64 = (WIDTH as u64 * 2) * (HEIGHT as u64 * 2) * 4;

/// How wide a cut square lands on screen, in the pane's own pixels. Measured
/// off paxel.ycombinator.com, whose dither runs a 4px cell and no gutter — the
/// white lattice in it is the squares that were not cut, not gaps between the
/// ones that were.
const CELL: i64 = 4;

/// The band's width at the window's default size: 1100, less the sidebar and
/// the card's insets and border. The grid is frozen into the file, so it is cut
/// for one width and drifts either side of it.
const BAND: i64 = 882;

/// The grid that gives, at the 5:2 a cover is cut at.
const COLS: i64 = BAND / CELL;
const ROWS: i64 = COLS * 2 / 5;

/// How much of the threshold is the ordered matrix rather than per-cell noise.
/// All matrix is a printed halftone; all noise is television static.
const ORDERED: f64 = 0.75;

/// Where the field ends up once it is as far from the anchor as it gets, and
/// the curve it takes getting there. The far end thins out but never clears.
const FLOOR: f64 = 0.55;
const FALLOFF: f64 = 1.6;

/// How far the low-frequency noise pulls the density either way, and how coarse
/// it is. This is what puts streaks and holes through the field instead of an
/// even wash.
const WOBBLE: f64 = 0.30;
const WOBBLE_SCALE: f64 = 5.0;

/// The accent's share of the squares at the anchor, how fast it gives out, and
/// what is left of it further off — the stray dark square out in the field.
const ACCENT: f64 = 0.85;
const ACCENT_FALLOFF: f64 = 6.0;
const ACCENT_STRAY: f64 = 0.015;

/// The 4×4 ordered-dither matrix. Its diagonal structure is the whole reason
/// to use one: a threshold this regular leaves the woven look the picture is
/// after, where noise alone would leave a rash.
const BAYER: [f64; 16] = [
    0., 8., 2., 10., 12., 4., 14., 6., 3., 11., 1., 9., 15., 7., 13., 5.,
];

/// The ground the squares are cut from.
const GROUND: &str = "#ffffff";

/// Paper comes in a fixed set of colours, and so does this. A pair chosen from
/// a list rather than a hue taken off a wheel is what stops a seed landing on a
/// colour nobody would have picked. Fill first, accent second.
const PAPER: [(&str, &str); 8] = [
    ("#ffb280", "#ff6600"),
    ("#a9c8ee", "#2c6fbb"),
    ("#b3d8b0", "#3e8e5a"),
    ("#f3b7c2", "#d6436e"),
    ("#c9b8e8", "#6b4fa8"),
    ("#f6d9a0", "#d9982b"),
    ("#a6d8d4", "#2e8b84"),
    ("#c2cbd4", "#5a6b7c"),
];

/// Which corner the squares mass at. Both are top corners: the weight belongs
/// above the document, and a cover that piles up along its bottom edge closes
/// on the first line of the article.
const ANCHORS: [(f64, f64); 2] = [(0., 0.), (1., 0.)];

/// The cover in this article's directory, if it has been given one.
pub fn of(article: &Path) -> Option<PathBuf> {
    std::fs::read_dir(article.parent()?)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(MARK))
        })
}

/// Where this document's next cover goes.
pub fn path(article: &Path, seed: u64, ext: &str) -> Option<PathBuf> {
    Some(article.with_file_name(format!("{MARK}{seed}.{ext}")))
}

/// The seed to cut the next one from: the document's own path the first time,
/// and a step on from the last one after that, so shuffling moves.
///
/// The path rather than the title, because a document is `untitled.md` at the
/// moment it is most likely to be given a cover — seeding off what it is called
/// would hand every untitled article in every project the same picture.
pub fn seed(article: &Path, current: Option<&Path>) -> u64 {
    let seed = match current.and_then(seed_of) {
        Some(seed) => hash([seed, 1]),
        None => hash(article.to_string_lossy().bytes().map(u64::from)),
    };
    // Short enough to read: these names sit in a directory people open, and a
    // twenty-digit one buries the document's own name.
    seed % SEEDS
}

/// Bring a picked image in, no wider than [`WIDTH`]. Written back in the format
/// its extension names, so a photo stays a JPEG instead of becoming a PNG
/// twenty times its size.
pub fn import(source: &Path, to: &Path) -> Result<()> {
    let picture = image::open(source)?;
    let picture = match picture.width() > WIDTH {
        true => picture.resize(WIDTH, u32::MAX, FilterType::Lanczos3),
        false => picture,
    };
    Ok(picture.save(to)?)
}

/// The picture: squares cut on a grid, massed at one corner and thinning out
/// across the field.
pub fn svg(seed: u64) -> String {
    let (fill, accent) = PAPER[(seed % PAPER.len() as u64) as usize];
    let anchor = ANCHORS[(seed / PAPER.len() as u64 % ANCHORS.len() as u64) as usize];
    let (mut plain, mut hot) = (String::new(), String::new());

    for row in 0..ROWS {
        // The squares touch, so a run of them in one colour is one wide
        // rectangle rather than a rectangle each. At this grid that is the
        // difference between twenty thousand nodes in the file and a few
        // thousand. One past the last column closes whatever run is open.
        let mut run: Option<(i64, bool)> = None;
        for col in 0..=COLS {
            let here = (col < COLS).then(|| cell(col, row, seed, anchor)).flatten();
            if let Some((start, was)) = run
                && here != Some(was)
            {
                let strip = match was {
                    true => &mut hot,
                    false => &mut plain,
                };
                strip.push_str(&format!(
                    "<rect x=\"{start}\" y=\"{row}\" width=\"{}\" height=\"1\"/>",
                    col - start
                ));
                run = None;
            }
            if run.is_none()
                && let Some(is_accent) = here
            {
                run = Some((col, is_accent));
            }
        }
    }

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{HEIGHT}\" \
         viewBox=\"0 0 {COLS} {ROWS}\" shape-rendering=\"crispEdges\">\
         <rect width=\"{COLS}\" height=\"{ROWS}\" fill=\"{GROUND}\"/>\
         <g fill=\"{fill}\">{plain}</g><g fill=\"{accent}\">{hot}</g></svg>"
    )
}

/// Whether a square is cut here, and whether it is an accent one.
///
/// Three thresholds stacked: an ordered matrix for the weave, per-cell noise to
/// rough it up, and a coarse noise across the whole field for the streaks and
/// holes that keep it from reading as a ramp.
fn cell(col: i64, row: i64, seed: u64, anchor: (f64, f64)) -> Option<bool> {
    let u = (col as f64 + 0.5) / COLS as f64;
    let v = (row as f64 + 0.5) / ROWS as f64;
    let away = (((u - anchor.0).powi(2) + (v - anchor.1).powi(2)).sqrt() / SQRT_2).min(1.);

    let density =
        FLOOR + (1. - FLOOR) * (1. - away).powf(FALLOFF) + (wobble(u, v, seed) - 0.5) * WOBBLE;
    let threshold = ORDERED * BAYER[(row % 4 * 4 + col % 4) as usize] / 16.
        + (1. - ORDERED) * unit([col as u64, row as u64, seed]);
    if threshold >= density {
        return None;
    }

    let heat = ACCENT * (1. - away).powf(ACCENT_FALLOFF) + ACCENT_STRAY * (1. - away).powi(2);
    Some(unit([col as u64, row as u64, seed, 7]) < heat)
}

/// The seed the cover at this path was cut from.
fn seed_of(cover: &Path) -> Option<u64> {
    let (_, tail) = cover.file_name()?.to_str()?.split_once(MARK)?;
    tail.split_once('.')?.0.parse().ok()
}

/// Value noise: one random number per coarse lattice point, smoothed between
/// them. Cheaper than a gradient noise and indistinguishable at this scale,
/// where the whole field is sixty squares across.
fn wobble(u: f64, v: f64, seed: u64) -> f64 {
    let (x, y) = (u * WOBBLE_SCALE, v * WOBBLE_SCALE);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (smooth(x - x0), smooth(y - y0));
    let at = |i: u64, j: u64| unit([x0 as u64 + i, y0 as u64 + j, seed]);
    let top = at(0, 0) * (1. - fx) + at(1, 0) * fx;
    let bottom = at(0, 1) * (1. - fx) + at(1, 1) * fx;
    top * (1. - fy) + bottom * fy
}

/// Smoothstep, so the lattice reads as blotches rather than as diamonds.
fn smooth(t: f64) -> f64 {
    t * t * (3. - 2. * t)
}

fn hash(keys: impl IntoIterator<Item = u64>) -> u64 {
    let mut x = 0xcbf2_9ce4_8422_2325_u64;
    for key in keys {
        x = (x ^ key).wrapping_mul(0x0000_0100_0000_01b3);
    }
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^ (x >> 29)
}

/// The same hash as a fraction of one.
fn unit(keys: impl IntoIterator<Item = u64>) -> f64 {
    (hash(keys) % 1_000_003) as f64 / 1_000_003.
}
