---
title: Articles
description: Cydonia's documents — the Markdown they accept, how images are sized, and which links become previews.
---

An article is a Markdown document with a title of its own. The title is not a
heading in the body: renaming the article changes the title, and a `#` line at
the top is just a heading.

⌘E switches between the rendered document and its Markdown source, carrying the
caret across.

## Formatting

Headings, paragraphs, bold, italic, strikethrough, inline code, fenced code,
quotes, lists, task lists, thematic breaks and pipe tables. Use four spaces for
nested list levels, and keep table cells on one line — escape a literal pipe as
`\|`.

Raw HTML is literal text. Underline, text colours, `==highlight==`, math,
footnotes and callout blocks are not enabled. A `mermaid` fence displays as
code; use an image where a rendered diagram is needed.

## Images

Put a displayed picture in its own paragraph, with blank lines around it:

```markdown
![Architecture overview](/absolute/path/.cydonia/assets/overview.png)

![Architecture overview|480](/absolute/path/.cydonia/assets/overview.png)

![|320](/absolute/path/.cydonia/assets/overview.png)
```

- Alt text is also the caption. Leave it empty for no caption.
- A trailing `|480` asks for a width in pixels, capped by the page width. It
  applies to a standalone image only; an image inside a paragraph renders as
  linked alt text.
- Use the width suffix — not `=480x`, `{width=480}` or HTML attributes. Height,
  crop, alignment and shape have no authored syntax.
- Local destinations must be absolute paths. Wrap a path containing spaces in
  angle brackets: `![Overview|480](</path/system overview.png>)`.
- HTTP(S) URLs work where the image is reachable from the app.

Media an agent creates belongs in `<project>/.cydonia/assets/`, which is shared:
an asset outlives the article that referenced it.

## Links

How a link is spelled decides whether it stays text or becomes a preview:

```markdown
[Read the guide](https://example.com/guide)

<https://example.com/guide>

[https://example.com/guide](https://example.com/guide "chip")

[https://example.com/guide](https://example.com/guide "embed")
```

- Ordinary links and bare URLs stay text.
- An angle-bracket URL alone in a paragraph becomes a bookmark card; inside
  prose it becomes a rich inline link.
- The exact titles `"chip"` and `"embed"` select a compact chip or a larger
  preview card. A standalone preview needs the label to equal the URL. What the
  preview shows depends on the metadata the destination supplies.

## Covers and width

An article can carry a cover picture, given to it as it is created. A page with
nothing of its own to say is set at the width **Settings › Appearance** asks
for; a page you have decided about carries that decision itself and ignores the
preference.
