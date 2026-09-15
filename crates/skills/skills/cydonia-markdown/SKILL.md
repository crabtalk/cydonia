---
name: cydonia-markdown
description: Write and edit Cydonia articles using its supported Markdown syntax, image sizing and paths, and rich links. Use for content displayed in Cydonia, not ordinary repository Markdown.
---

# Cydonia Markdown

Use this guidance alongside the connected article tools. Their descriptions
define operations and arguments; this skill defines the content they accept.

## Article content

An article's title is separate from its Markdown body. Use `article_rename`
to change the title; a heading inside the body does not rename the article.
Keep YAML frontmatter out of the body: it is not article metadata.

Read the current body before editing. Use `article_edit` for targeted changes
and `article_rewrite` for an intentional full replacement. After an editor
save, reread before constructing an exact match: whitespace, escaping and
list numbering can be normalized.

## Images

Put each displayed picture in its own paragraph, with blank lines around it:

```markdown
![Architecture overview](/absolute/project/.cydonia/articles/ARTICLE_ID/overview.png)

![Architecture overview|480](/absolute/project/.cydonia/articles/ARTICLE_ID/overview.png)

![|320](/absolute/project/.cydonia/articles/ARTICLE_ID/overview.png)
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

Each article lives at `<project>/.cydonia/articles/<id>/content.md`. Its media
belong beside `content.md`. Obtain the project and article ID from session
context and article tool results; never derive the directory from the title.

Local image destinations must be absolute filesystem paths. Relative paths
are resolved against the app process, not the article directory. Do not
assume `file://` URLs behave like local paths. HTTP(S) image URLs can also be
used when the image is available to the app.

For paths containing spaces, enclose the destination in angle brackets:

```markdown
![Overview|480](</absolute/project/.cydonia/articles/ARTICLE_ID/system overview.png>)
```

When permitted filesystem access is available, copy or generate media into
the article directory and link the resulting file. Preserve existing media
and managed metadata. Article tools only write Markdown; they do not upload
or copy image bytes. If the connected server forbids direct `.cydonia/`
access, use an existing accessible image or report that media placement is
unavailable through that connection.

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
