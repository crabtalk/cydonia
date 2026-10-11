//! Numbers and names as the app writes them in a line of text.

use artifact::board::Card;
use std::time::Duration;

/// The longest a [`CardTitle::short_title`] runs, in characters, ahead of its
/// `…`.
const CARD_TITLE: usize = 48;

/// `n` with its thousands grouped: `12,480`.
pub(crate) fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (ix, digit) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Tokens, thinned to the digits that carry: `840`, `4.2K`, `128K`, `1.2M`,
/// `3.40B`.
pub(crate) fn tokens(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..100_000 => format!("{:.1}K", n as f64 / 1e3),
        100_000..1_000_000 => format!("{}K", n / 1_000),
        1_000_000..100_000_000 => format!("{:.1}M", n as f64 / 1e6),
        100_000_000..1_000_000_000 => format!("{}M", n / 1_000_000),
        _ => format!("{:.2}B", n as f64 / 1e9),
    }
}

/// A running time: `42s`, then `03m 07s`.
pub(crate) fn elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    match seconds < 60 {
        true => format!("{seconds}s"),
        false => format!("{:02}m {:02}s", seconds / 60, seconds % 60),
    }
}

/// `title`, or `fallback` when it is blank.
pub(crate) fn untitled(title: &str, fallback: &str) -> String {
    match title.trim() {
        "" => fallback.to_owned(),
        title => title.to_owned(),
    }
}

/// The titles a card is shown under, read off its text.
pub(crate) trait CardTitle {
    /// The one line a card is listed under.
    fn title(&self) -> String;

    /// What a card's chip paints ahead of its handle: the heading the card
    /// opens with, whole, and otherwise [`CardTitle::short_title`].
    fn chip_title(&self) -> String;

    /// [`CardTitle::title`] cut to [`CARD_TITLE`] characters at a word, `…`
    /// marking the cut. A first word longer than that is cut inside it.
    fn short_title(&self) -> String {
        let title = self.title();
        if title.chars().count() <= CARD_TITLE {
            return title;
        }
        let end = title
            .char_indices()
            .nth(CARD_TITLE)
            .map_or(title.len(), |(at, _)| at);
        let head = &title[..end];
        let head = match head.rfind(' ') {
            Some(at) if at > 0 => &head[..at],
            _ => head,
        };
        format!("{}…", head.trim_end_matches([',', ';', ':', '.', ' ']))
    }
}

impl CardTitle for markdown::Doc {
    fn title(&self) -> String {
        let doc = self;
        use markdown::BlockKind;
        for block in &doc.blocks {
            let candidate = match &block.kind {
                BlockKind::Image { alt, .. } => {
                    if alt.text.trim().is_empty() {
                        "Image".into()
                    } else {
                        alt.text.clone()
                    }
                }
                BlockKind::Bookmark { url, .. } => url.clone(),
                BlockKind::Table { header, rows, .. } => header
                    .iter()
                    .chain(rows.iter().flatten())
                    .find(|cell| !cell.text.trim().is_empty())
                    .map(|cell| cell.text.clone())
                    .unwrap_or_else(|| "Table".into()),
                BlockKind::Rule => continue,
                BlockKind::Code { code, .. } => code
                    .text
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("Code")
                    .to_owned(),
                _ => block
                    .text_at(markdown::Part::Body)
                    .map(|text| text.text.clone())
                    .unwrap_or_default(),
            };
            let mut title = String::new();
            let mut length = 0;
            for word in candidate.split_whitespace() {
                if !title.is_empty() {
                    title.push(' ');
                    length += 1;
                }
                for ch in word.chars() {
                    if length >= 512 {
                        title.push('…');
                        return title;
                    }
                    title.push(ch);
                    length += 1;
                }
            }
            if !title.is_empty() {
                return title;
            }
        }
        "Untitled card".into()
    }

    fn chip_title(&self) -> String {
        let opening = self
            .blocks
            .iter()
            .find(|block| !matches!(block.kind, markdown::BlockKind::Rule));
        match opening.map(|block| &block.kind) {
            Some(markdown::BlockKind::Heading { text: heading, .. })
                if !heading.text.trim().is_empty() =>
            {
                artifact::text::one_line(&heading.text)
            }
            _ => self.short_title(),
        }
    }
}

impl CardTitle for Card {
    fn title(&self) -> String {
        markdown::parse(&self.text).title()
    }

    fn chip_title(&self) -> String {
        markdown::parse(&self.text).chip_title()
    }
}
