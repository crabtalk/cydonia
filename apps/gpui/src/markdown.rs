//! pulldown-cmark → gpui elements. Parsing happens per visible row per
//! notify, which is cheap at chat-message sizes — no cache needed.

use crate::theme::Theme;
use gpui::{
    AnyElement, FontStyle, FontWeight, HighlightStyle, Hsla, StyledText, Window, div, prelude::*,
    px,
};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub const FONT_MONO: &str = "Menlo";

enum Block {
    Paragraph {
        inline: Inline,
        quote: bool,
    },
    Heading {
        level: HeadingLevel,
        inline: Inline,
    },
    Bullet {
        depth: usize,
        marker: String,
        inline: Inline,
    },
    Code {
        code: String,
    },
    Rule,
}

#[derive(Default)]
struct Inline {
    text: String,
    highlights: Vec<(Range<usize>, Emphasis)>,
}

impl Inline {
    fn push(&mut self, text: &str, emphasis: Emphasis) {
        let start = self.text.len();
        self.text.push_str(text);
        if emphasis.any() {
            self.highlights.push((start..self.text.len(), emphasis));
        }
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// Active inline emphasis, merged into one highlight per text span.
#[derive(Default, Clone, Copy)]
struct Emphasis {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
}

impl Emphasis {
    fn any(&self) -> bool {
        self.bold || self.italic || self.strike || self.code || self.link
    }

    fn style(&self, theme: &Theme) -> HighlightStyle {
        HighlightStyle {
            color: self.link.then_some(theme.accent),
            font_weight: self.bold.then_some(FontWeight::BOLD),
            font_style: self.italic.then_some(FontStyle::Italic),
            background_color: self.code.then_some(theme.code_wash),
            underline: None,
            strikethrough: self.strike.then(|| gpui::StrikethroughStyle {
                thickness: px(1.),
                color: None,
            }),
            fade_out: None,
        }
    }
}

fn parse(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut inline = Inline::default();
    let mut emphasis = Emphasis::default();
    let mut quote = 0usize;
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut in_item = false;
    let mut code: Option<String> = None;

    fn flush_paragraph(blocks: &mut Vec<Block>, inline: &mut Inline, quote: usize) {
        if !inline.is_empty() {
            blocks.push(Block::Paragraph {
                inline: std::mem::take(inline),
                quote: quote > 0,
            });
        }
    }

    let parser = Parser::new_ext(
        source,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES,
    );
    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(mut code) = code.take() {
                    while code.ends_with('\n') {
                        code.pop();
                    }
                    blocks.push(Block::Code { code });
                }
            }
            Event::Text(text) if code.is_some() => {
                code.as_mut().expect("checked").push_str(&text);
            }

            Event::Start(Tag::Heading { .. }) => flush_paragraph(&mut blocks, &mut inline, quote),
            Event::End(TagEnd::Heading(level)) => {
                if !inline.is_empty() {
                    blocks.push(Block::Heading {
                        level,
                        inline: std::mem::take(&mut inline),
                    });
                }
            }

            Event::Start(Tag::List(start)) => list_stack.push(start),
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                in_item = true;
                inline = Inline::default();
            }
            Event::End(TagEnd::Item) => {
                in_item = false;
                let depth = list_stack.len().saturating_sub(1);
                let marker = match list_stack.last_mut() {
                    Some(Some(n)) => {
                        let marker = format!("{n}. ");
                        *n += 1;
                        marker
                    }
                    _ => "•  ".to_owned(),
                };
                if !inline.is_empty() {
                    blocks.push(Block::Bullet {
                        depth,
                        marker,
                        inline: std::mem::take(&mut inline),
                    });
                }
            }

            Event::Start(Tag::BlockQuote(_)) => quote += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote = quote.saturating_sub(1),

            Event::Start(Tag::Paragraph) => {
                // Loose list items contain paragraphs; keep them in one bullet.
                if in_item && !inline.is_empty() {
                    inline.push("\n", Emphasis::default());
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if !in_item {
                    flush_paragraph(&mut blocks, &mut inline, quote);
                }
            }

            Event::Start(Tag::Emphasis) => emphasis.italic = true,
            Event::End(TagEnd::Emphasis) => emphasis.italic = false,
            Event::Start(Tag::Strong) => emphasis.bold = true,
            Event::End(TagEnd::Strong) => emphasis.bold = false,
            Event::Start(Tag::Strikethrough) => emphasis.strike = true,
            Event::End(TagEnd::Strikethrough) => emphasis.strike = false,
            Event::Start(Tag::Link { .. }) => emphasis.link = true,
            Event::End(TagEnd::Link) => emphasis.link = false,

            Event::Rule => blocks.push(Block::Rule),
            Event::SoftBreak => inline.push(" ", Emphasis::default()),
            Event::HardBreak => inline.push("\n", Emphasis::default()),
            Event::Code(text) => inline.push(
                &text,
                Emphasis {
                    code: true,
                    ..emphasis
                },
            ),
            Event::Text(text) => inline.push(&text, emphasis),
            _ => {}
        }
    }
    flush_paragraph(&mut blocks, &mut inline, quote);
    blocks
}

/// Render markdown `source` as a column of block elements.
pub fn markdown(source: &str, color: Hsla, theme: Theme, window: &Window) -> AnyElement {
    let blocks = parse(source);
    div()
        .flex()
        .flex_col()
        .gap_2()
        .children(
            blocks
                .into_iter()
                .map(|block| render_block(block, color, theme, window)),
        )
        .into_any_element()
}

fn render_block(block: Block, color: Hsla, theme: Theme, window: &Window) -> AnyElement {
    match block {
        Block::Paragraph { inline, quote } => {
            let text = styled(inline, color, 14., FontWeight::NORMAL, theme, window);
            if quote {
                div()
                    .pl_3()
                    .border_l_2()
                    .border_color(theme.border)
                    .text_color(theme.text_secondary)
                    .child(text)
                    .into_any_element()
            } else {
                text
            }
        }
        Block::Heading { level, inline } => {
            let size = match level {
                HeadingLevel::H1 => 20.,
                HeadingLevel::H2 => 17.,
                _ => 15.,
            };
            styled(inline, color, size, FontWeight::BOLD, theme, window)
        }
        Block::Bullet {
            depth,
            marker,
            inline,
        } => div()
            .flex()
            .flex_row()
            .pl(px(16. * depth as f32))
            .child(
                div()
                    .flex_none()
                    .w(px(22.))
                    .text_color(theme.text_tertiary)
                    .child(marker),
            )
            .child(div().flex_1().min_w_0().child(styled(
                inline,
                color,
                14.,
                FontWeight::NORMAL,
                theme,
                window,
            )))
            .into_any_element(),
        Block::Code { code } => div()
            .rounded_md()
            .bg(theme.code_wash)
            .border_1()
            .border_color(theme.border)
            .px_3()
            .py_2()
            .font_family(FONT_MONO)
            .text_size(px(12.5))
            .child(code)
            .into_any_element(),
        Block::Rule => div().h(px(1.)).w_full().bg(theme.border).into_any_element(),
    }
}

fn styled(
    inline: Inline,
    color: Hsla,
    size: f32,
    weight: FontWeight,
    theme: Theme,
    window: &Window,
) -> AnyElement {
    let mut style = window.text_style();
    style.color = color;
    style.font_size = px(size).into();
    style.font_weight = weight;
    let highlights = inline
        .highlights
        .into_iter()
        .map(|(range, emphasis)| (range, emphasis.style(&theme)));
    StyledText::new(inline.text)
        .with_default_highlights(&style, highlights)
        .into_any_element()
}
