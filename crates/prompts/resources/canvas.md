---
name: canvas
description: Create and edit canvas entries through canvas_add, canvas_read, and canvas_edit. Read before writing canvas nodes or edges.
---

# Canvas entries

A canvas uses JSON Canvas nodes and edges. Address it by its stable entry
reference, such as `#12`. `project_entries` discovers canvases;
`project_read_entry` also reads them.

Create with `canvas_add`, read with `canvas_read`, then pass the returned
`version` to `canvas_edit`. A stale version refuses the whole edit. Read again
and reconcile your changes before retrying.

The edit object accepts `nodes`, `edges`, `remove_nodes`, and `remove_edges`.
All are optional arrays. Nodes and edges are complete replacements by id;
preserve fields you did not intend to change. Existing items keep their order;
new items append. Removing a node also removes its incident edges. All edits
are applied together, or none are saved.

Each node needs a unique nonempty `id`, a `type`, integer `x`, `y`, `width`,
and `height`. Dimensions must be positive. Standard types are `text` (`text`
contains Markdown), `file` (`file` is a path relative to the project), `link`
(`url`), and `group` (`label`, optional `background` and `backgroundStyle`).
Colors are `1` through `6` or a hex color. Keep unknown fields on existing
nodes and edges.

Text nodes accept `textMode`: `label` for unboxed annotations that grow in
height as text wraps, or `card` (the default) for document cards. Text nodes and
edges accept a positive `fontSize` in canvas units; the default is 16. Zoom
scales the authored size. A label's side handles change its wrapping width;
its corner handles scale its dimensions and text together.

An edge has a unique nonempty `id`, `fromNode`, and `toNode`. Both nodes must
exist after the edit. Optional `fromSide` and `toSide` are `top`, `right`,
`bottom`, or `left`; `fromEnd` and `toEnd` are `none` or `arrow`. Edges can carry
`label` and `color`.

Do not edit managed canvas files directly. The tools save through the shared
backend with concurrency checks and notify open views through the project watch.
