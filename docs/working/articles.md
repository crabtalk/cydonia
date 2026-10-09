---
title: Articles
description: Cydonia's documents — the Markdown they accept, how images are sized, which links become previews, and links to sessions, articles and boards.
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
![Architecture overview](assets/overview.png)

![Architecture overview|480](assets/overview.png)

![|320](assets/overview.png)
```

- Alt text is also the caption. Leave it empty for no caption.
- A trailing `|480` asks for a width in pixels, capped by the page width. It
  applies to a standalone image only; an image inside a paragraph renders as
  linked alt text.
- Use the width suffix — not `=480x`, `{width=480}` or HTML attributes. Height,
  crop, alignment and shape have no authored syntax.
- A local destination is a path relative to the article's folder, such as
  `assets/overview.png`. Wrap a path containing spaces in
  angle brackets: `![Overview|480](<assets/system overview.png>)`.
- HTTP(S) URLs work where the image is reachable from the app.

**Settings › Editor › Download web pictures** saves a copy of a pasted picture link into the article's `assets/`; off, the link stays a web address. A pasted screenshot or picture file is always saved into `assets/`. Opening a picture in another app from the button over it always saves a copy into `assets/` and points the article at it, whichever way the switch is set, so edits saved there show in the article.

Each article keeps its pictures and its cover in its own `assets/`, which moves with it. Pictures already under `<project>/.cydonia/assets/` keep working.

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

## Links to sessions, articles and boards

A session, an article or a board is linked as `cydonia://` followed by its [reference](./references.md): `cydonia://cydonia#12`. It takes the same three forms as a web link:

```markdown
[Roadmap](cydonia://cydonia#12)

[cydonia://cydonia#12](cydonia://cydonia#12 "chip")

[cydonia://cydonia#43](cydonia://cydonia#43 "embed")
```

- An ordinary link stays text. Clicking it opens the entry in a drawer at the foot of the pane; clicking it again puts the drawer away.
- `"chip"` shows inline as the entry's mark and title.
- `"embed"` alone on a line is a card of the entry. A session's card is its transcript, read only. A link to some of its turns, such as `cydonia://cydonia#43:5-7`, shows those turns alone, read only, under a header naming the session and the range; clicking the header opens the session at the first of them. A link to part of an article, such as `cydonia://cydonia#12:5-7` or `cydonia://cydonia#12#setup`, shows the blocks those lines or that section cover, read only, under a header naming the article and the part; clicking the header opens the article with that part at the top. A whole article's or a board's card is its title, kind and number, and opens it in the pane's drawer.

Deleting a card leaves the entry where it was.

In the editor:

- Type `@` to link a session, article or board as a chip. What you type after it narrows the list — see [References](./references.md#finding-an-entry).
- `/Session` searches the project's sessions, with a preview of the last turns of each, and puts the one you pick here.
- Paste a `cydonia://` link on a line of its own and choose how it shows, as for a web link.

## Covers and width

An article can carry a cover picture, given to it as it is created. A page with
nothing of its own to say is set at the width **Settings › Appearance** asks
for; a page you have decided about carries that decision itself and ignores the
preference.
