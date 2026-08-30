# bezel: two editor fixes waiting to land

Status: written and verified in a worktree, **uncommitted**. Cydonia is wired to
it by a temporary `[patch.crates-io]` so the article pane works now, without
waiting on a crates.io release.

Worktree: `~/code/bezel-editor-fix`, branch `fix/editor-empty-doc-and-handle`,
cut from `dev` at `054dfad`. `~/code/bezel` itself is untouched — it was busy.

## The fixes

Both surfaced building cydonia's article pane, and both are bezel's, not ours.

**An empty document had nowhere to put a caret.** `markdown::parse("")` returns a
`Doc` with no blocks, which is correct for the model and unusable as an editing
surface: nothing paints, so there is no caret and no placeholder, and neither a
click nor a hit test has a target — an empty file sat inert until something typed
a block into existence, which is the one thing you cannot do with no caret.
`ensure_block` in `crates/editor/src/editor.rs` keeps one empty paragraph. Called
from `Editor::new` *and* from `Editor::edit`, because construction is not the only
doorway — `remove_block` on the last block empties the document too. The model
layer is unchanged, so `parse("")` still returns an empty doc and markdown's
fixed-point tests still mean what they meant.

**The gutter handle was top-aligned to the block box.** `BlockLayouts` recorded
every painted run but exposed only whole-block bounds, so `menu.rs` had nothing
better to place the handle against. New `BlockLayouts::first_row` in
`crates/markdown/src/render.rs` gives the first run's top and line height, and the
handle centres on that. Body text (22pt line) was ~2pt out and read as fine; an H1
(27pt) was ~4.5pt out, and an H1 is the first line of every article.

`cargo test -p bezel-editor -p bezel-markdown`: 73 passed.

## What cydonia carries meanwhile

- Root `Cargo.toml` has a `[patch.crates-io]` block pointing all eight bezel-*
  crates at the worktree. All of them, not just `bezel-editor`: a partial patch
  puts two copies of the token system in the graph.
- `crates/app/Cargo.toml` moved `gpui` and `gpui_platform` from `0.3.6` to
  `0.3.7`, because bezel's tree needs `bezel-gpui ^0.3.7`. The whole platform
  stack has to move together — a lock with `bezel-gpui` at 0.3.7 and
  `bezel-gpui-macos` at 0.3.6 does not compile.

## To make it durable

1. Commit the worktree, merge `fix/editor-empty-doc-and-handle` into bezel's `dev`.
2. Publish bezel `0.1.4`.
3. Here: drop the `[patch.crates-io]` block, bump the four `0.1.3` deps in
   `crates/app/Cargo.toml` (`bezel`, `editor`, `markdown`, `syntax`) to `0.1.4`,
   and delete this file.

The gpui `0.3.7` bump stays — it is what bezel 0.1.4 will want anyway.
