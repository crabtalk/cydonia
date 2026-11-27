---
name: design
description: Create, verify and revise Design artboards.
---

# Design entries

Use `design_list` to find existing designs and `design_read` to read one by
its stable reference, such as `#12`. Use `design_add` to create a new design.
The Design canvas lays out its artboards side by side at a shared viewport
width. A reviewer can pan, zoom, or expand an artboard to interact with it.

## Create an artboard

Start from the project's tokens, typography, components and closest finished
screen. Use one artboard per screen or variant so the reviewer can compare
them on the canvas.

Call `design_add_artboard` with the design reference and an artboard title.
Use the artboard id and project-relative path returned by the tool.

```text
.cydonia/designs/{design}/{artboard}/index.html
```

Write `index.html` and its CSS, images and scripts in that artboard directory.
Use relative paths for those files. Keep entry metadata in the design tools.
To revise an existing artboard, edit the files at the path from `design_read`
so the artboard keeps its id.

## Check each artboard

Call `browser_screenshot` with the design reference in `design` and the id
from `design_read` in `artboard`. Inspect each screenshot at the design's
configured viewport before handing the work back. Exercise forms, menus and
other interactions with `browser_read`, `browser_inspect` and `browser_click`.
See `cydonia://resources/browser` for target fields and element numbers.
