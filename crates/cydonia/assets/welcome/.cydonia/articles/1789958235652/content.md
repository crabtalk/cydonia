Articles are Markdown. Most of what you know works; this page is the parts that are specific to Cydonia, shown rather than described.

## The ordinary things

Headings, **bold**, _italic_, ~~strikethrough~~, `inline code`, and lists all behave as you would expect. Task lists tick:

- [x] Something finished
- [ ] Something not

Fenced code is painted once the grammar for its language is installed. Grammars are downloaded on demand, offered when you open a file in that language:

```rust
fn main() {
    println!("hello from a fence");
}
```

Tables keep each cell on one line. Escape a literal pipe with `\|`.

| Column | Another |
| --- | --- |
| a cell | another cell |

> Quotes work. Nesting them deeply does not — the editor flattens mixed containers, so keep a quote a quote and a list a list.

---

## Images

An image in a paragraph of its own is displayed. Its alt text is the caption, and a `|480` at the end of the alt text asks for a width in pixels:

```markdown
![A caption|480](/absolute/path/to/picture.png)
```

Leave the alt text empty for no caption. Local paths must be absolute — a relative path is resolved against the app, not the article. Put pictures in the project's `.cydonia/assets/`, and wrap a path containing spaces in angle brackets: `](<…/my picture.png>)`.

There is no height, crop, or alignment syntax. `=480x`, `{width=480}` and HTML attributes do nothing.

## Links

How you spell a link decides whether it stays text or becomes a card.

An ordinary link stays text: [the Cydonia repository](https://github.com/crabtalk/cydonia).

A bare URL in angle brackets, alone in its paragraph, becomes a bookmark card:

<https://github.com/crabtalk/cydonia>

Adding the exact title `"chip"` or `"embed"` selects a compact chip or a larger preview. The label has to equal the URL for a standalone one:

[https://github.com/crabtalk/cydonia](https://github.com/crabtalk/cydonia "chip")

## What is not here

Raw HTML is literal text, not layout. Underline, text colours, `==highlight==`, maths, footnotes and callout blocks are not enabled. A `mermaid` fence shows its source; use an image where a rendered diagram is needed.

## The cover

The band across the top of an article is its cover, and it is not part of the Markdown — set it from the article's own menu. Cut it **5:2**; a picture of another shape is shown at its own proportions rather than cropped to fit.