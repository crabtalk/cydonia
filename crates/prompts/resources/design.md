---
name: design
description: Create, verify and revise Design pages with the design and browser tools. Read before working on a design.
---

# Design entries

Use `design_list` to find existing designs and `design_read` to read one by
its stable reference, such as `#12`. Its pages appear in a review grid with
one shared viewport. Use `design_add` when the work calls for a new design.

## Write a page

Start with the project's design tokens, typography, components and closest
finished screen. Reuse its layout and interaction conventions. Use one page
per screen or variant. Add a variant as its own page so the reviewer can see
both versions in the grid.

Use `design_add_page` with the design's reference and a page title. The tool
returns the page id and a project-relative path in this format.

```text
.cydonia/designs/{design}/{page}/index.html
```

Write `index.html` and that page's CSS, images and scripts in the returned
page directory. Reference them with relative paths within that directory.
Use the design tools for entry metadata. When revising an existing page, use its path from
`design_read` and edit the existing files so its comments remain with it.

## Check what the reviewer will see

Use `browser_screenshot` with the design reference in `design` and the page
id from `design_read` in `page`. Inspect each page's screenshot at the
design's configured viewport before handing it over. For a Design page
with a form, menu or other interaction, use `browser_read`,
`browser_inspect` and `browser_click` with the same target. Read the
`cydonia://resources/browser` for element numbers and
the behavior of pages loaded for a single call.

## Respond to review

`design_read` reports `new`, `approved` or `changes` for each page, along
with its comment count. The reviewer sets the status. Editing a page or
its assets returns it to `new`.

For a page marked `changes`, call `design_comments` with its design reference
and page id. Read each thread and the original selector and text. Detached
threads still carry their original anchors. Revise the page, check the
result in the browser, and reply in each addressed thread with
`design_comment_reply`. Pass the same design reference and page id, the
comment id, and an answer that describes what changed or what remains open.
