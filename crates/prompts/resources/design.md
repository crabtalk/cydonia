---
name: design
description: Create and read Design entries and write pages for review. Read before working on a design.
---

# Design entries

Use `design_add` to create an entry and `design_add_page` to reserve a page.
`design_list` finds designs. `design_read` accepts a stable entry reference
such as `#12` and returns the page paths, review statuses and comment counts.

The returned paths are relative to the project. Write each page's
`index.html`, CSS, images and scripts inside its own page directory. Use
relative references to those files. Use the design tools for entry metadata.

The review statuses are `new`, `approved` and `changes`. A write to a page
or its assets resets its status to `new`. The reviewer sets its status.

Target a page with `design` and `page` in the browser tools. Use the page id
from `design_read`. Screenshots use the design's current viewport.

Use `design_comments` with the design reference and page id to read its
threads and original anchor text. Use `design_comment_reply` with the same
target and a comment id to answer. Detached threads remain readable.
