//! Text cut down to fit a line.

/// How many characters [`preview`] keeps.
const PREVIEW: usize = 200;

/// `text` with every run of whitespace, newlines included, as one space.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// [`one_line`], cut to `max` characters and ellipsized.
pub fn clip(text: &str, max: usize) -> String {
    let line = one_line(text);
    match line.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", line[..at].trim_end()),
        None => line,
    }
}

/// The start of `markdown` as plain text on one line: the words of its
/// blocks, without the syntax around them, cut to [`PREVIEW`] characters.
/// Pictures and raw HTML say nothing here.
pub fn preview(markdown: &str) -> String {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let mut out = String::new();
    let mut pictures = 0usize;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Image { .. }) => pictures += 1,
            Event::End(TagEnd::Image) => pictures = pictures.saturating_sub(1),
            Event::Text(text) | Event::Code(text) if pictures == 0 => out.push_str(&text),
            Event::SoftBreak | Event::HardBreak | Event::End(_) => out.push(' '),
            _ => {}
        }
        if out.chars().count() > PREVIEW {
            break;
        }
    }
    clip(&out, PREVIEW)
}
