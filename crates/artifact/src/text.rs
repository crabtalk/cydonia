//! Text cut down to fit a line.

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
