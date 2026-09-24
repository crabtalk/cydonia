---
name: markdown
description: Write and edit Cydonia articles using its supported Markdown syntax, image sizing and paths, and rich links. Use for content displayed in Cydonia, not ordinary repository Markdown.
---

# Cydonia Markdown

Use this guidance alongside the connected article tools. Their descriptions
define operations and arguments; this guide defines the content they accept.

## Article content

An article's title is separate from its Markdown body. Use `article_rename`
to change the title; a heading inside the body does not rename the article.
Keep YAML frontmatter out of the body: it is not article metadata.

Cydonia writes the title at the head of the page. Do not open the body with a
heading that repeats it: begin at the first line of prose, and start the
body's own sections at `##`.

Read the current body before editing. Use `article_edit` for targeted changes
and `article_rewrite` for an intentional full replacement. After an editor
save, reread before constructing an exact match: whitespace, escaping and
list numbering can be normalized.

## Highlights

Wrap text in `==` to highlight it: `a ==key phrase== here`. Highlights paint
in one colour.

## Images

Put each displayed picture in its own paragraph, with blank lines around it:

```markdown
![Architecture overview](assets/overview.png)

![Architecture overview|480](assets/overview.png)

![|320](assets/overview.png)
```

- Alt text is also the standalone image's visible caption. Leave it empty
  for no caption; keep it plain text.
- A final `|480` in the alt text requests a width of 480 pixels, capped by the
  available page width. Width must be a positive integer. Omit it for natural
  sizing constrained to the page.
- The width suffix applies only to a standalone image. An image within prose
  is preserved as image Markdown but currently renders as linked alt text.
- Use the width suffix, not `=480x`, `{width=480}`, or HTML attributes.
  Height, crop, alignment and shape have no authored image syntax.
- Avoid captions ending in a literal `|` followed by digits: that tail is
  interpreted as width, even when the pipe was escaped.
- Image titles such as `![alt](path "title")` do not supply the caption or
  size; use the alt text and width suffix instead.

### Image files and destinations

Store new agent-created media in the article's own `assets/` directory, which
`article_read` and `article_add` return as `assets_path`, the absolute
directory on the Cydonia host. This is an explicit exception to the restriction
on direct access to managed artifacts. Create that directory if needed. Use
unique filenames; preserve existing files unless their replacement or removal
was requested. Images already under `<project>/.cydonia/assets/` continue to
work and need not be moved.

Write an image in the article's own `assets/` as a path relative to the
article's folder, such as `assets/overview.png`. A relative path is resolved
against the article's folder, so it keeps working when the folder is moved or
copied. Absolute filesystem paths also work. Do not assume `file://` URLs
behave like local paths. HTTP(S) image URLs can also be used when the image is
available to the app.

For paths containing spaces, enclose the destination in angle brackets:

```markdown
![Overview|480](<assets/system overview.png>)
```

With filesystem access to the Cydonia host, copy or generate the image in
`assets_path`, then insert it as `assets/<file>` using the article tools. This
media workflow is allowed by the server instructions; it does not need a
separate exception for each image. Read-only settings and filesystem permission restrictions
still apply. An article's `assets/` goes with it when it is moved, and is
deleted with it.

Article tools write Markdown, not image bytes. An MCP-only client without
filesystem access to the host must use an existing accessible image or an
HTTP(S) image URL. A local path on a remote client's machine is not a path on
the Cydonia host. Do not claim to upload or copy an image through article tools.

### The cover

The cover is the picture over the top of an article, and is not a body image:
it is never written into the Markdown, and it does not live in `assets/`. It
sits in the article's own folder, which `article_read` and `article_add` return
as `article_path`, under a name starting `cover-`. One article has one cover.

Set it with `article_set_cover`, which takes the path of a picture on the
Cydonia host and files it under the right name, removing whatever was there.
Do not write a `cover-` file into the folder by hand — the name carries a stamp
that keeps a replaced cover from being served from cache.

Draw or crop it **5:2** — 1500x600 is the size the app cuts its own at, and the
widest it keeps. A picture of another shape is shown at its own proportions
rather than cropped to fit, so a square one stands far taller than the band it
is meant to fill.

## Links and previews

The spelling controls whether a link stays text or becomes a rich preview:

```markdown
[Read the guide](https://example.com/guide)

<https://example.com/guide>

[https://example.com/guide](https://example.com/guide "chip")

[https://example.com/guide](https://example.com/guide "embed")
```

- Ordinary Markdown links and bare URLs stay text links, even alone.
- An angle-bracket HTTP(S) URL alone in a paragraph becomes a bookmark card;
  within prose it becomes a rich inline link.
- The exact titles `"chip"` and `"embed"` select a compact chip or a larger
  preview card. A standalone preview requires the label to equal the URL.
  A custom label such as `[Guide](url "embed")` stays inline.
- `"embed"` is a link preview, not an iframe or executable embed. Preview
  details depend on what metadata the destination supplies.

## Supported formatting and limits

Use headings, paragraphs, bold, italic, strikethrough, inline code, fenced
code, quotes, lists, task lists, thematic breaks and pipe tables. Use four
spaces for nested list levels, and keep table cells on one line. Escape
literal pipes in table cells with `\|`.

Raw HTML is literal text, not layout. Underline, text colors, `==highlight==`,
math typesetting, footnotes and special callout blocks are not enabled.
Mermaid fences display code; Cydonia does not install a diagram renderer.
Use an image when a rendered diagram is needed.

Avoid relying on nested quote depth or combinations such as a list inside a
quote: the editor flattens mixed containers. Soft and hard line breaks share
one representation. Exact source formatting is not preserved across editor
saves.
