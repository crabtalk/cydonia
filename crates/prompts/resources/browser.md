---
name: browser
description: Work in the in-app browser through the browser_* tools — opening pages, reading them, taking screenshots, clicking and typing. Use before driving a browser tab.
---

# The in-app browser

The browser is the user's own: the tabs in the project's right panel, signed in
wherever they are signed in. It reaches pages a plain fetch gets a login wall or
an empty shell from, and it runs a page's scripts, so a read is what is on
screen. For an ordinary public page, a plain fetch is cheaper and enough.

Tabs belong to a project. `browser_tabs` lists the project's browser tabs; the
one marked front is the one the user is looking at, and the one you get when you
name no tab. `browser_open` without a tab brings forward a tab already at that
address, as it stands and not reloaded, or else opens a new one in front of the
project's right panel, where the user can watch it and take it over.

Every read starts with its tab id. To go somewhere new in a tab you are working
in, or to reload it, pass that id to `browser_open` rather than opening another
tab.

A tab's page exists only once its panel has been on screen. A call on a tab
whose panel is not showing answers that it cannot load; say so rather than
retrying.

## Canvas web frames

For a web frame, pass `canvas` (the stable canvas reference, such as `#12`)
and `node` (its id from `canvas_read`) in place of `tab`. Read, screenshot,
scroll, console, inspect, evaluate, click and type use the frame's existing
page. `browser_tabs` and `browser_open` work with right-panel tabs.

Open the canvas and bring the frame into view to start its page. Frames with
no live page return an error. A screenshot uses the frame's configured page
viewport, independent of its size or zoom on the canvas.

## Design artboards

Pass `design` (a stable design reference) and `artboard` (an id from
`design_read`) as the artboard target. Browser artboard commands use the
design's configured viewport. If that artboard is open, commands use its live
WebView. Otherwise Cydonia loads it for the call and releases it afterward.
Scroll position, form state and console history persist between calls when the
artboard is open in the design view.

## Reading and acting

A read gives the page's text and a numbered list of the elements on it that can
be acted on. `browser_click` and `browser_type` take one of those numbers, and
both answer with the page as it stands afterwards.

The numbers describe the page you last read. Anything that changes the page —
your own click, a page that moves on its own, the user taking over — changes
them. Work from the read that came back last, and read again when you are not
sure it still holds.

A read already has the whole page's text, up to a limit it marks as truncated.
Scroll when a page loads more of itself as it goes down, as a feed does, or to
reach text past that limit.

## The console

`browser_console` gives what the page has logged since it was created:
console calls, uncaught errors and unhandled rejections, each with a message id,
level and the frame that logged it. Filter with `levels`, a list of `debug`,
`log`, `info`, `warn` or `error`. Omit it for all levels.

Every reply includes a `cursor`, even when nothing matches. Pass it as `since`
on the next call to read only newer messages. The last 500 messages are kept;
a reply reports when older messages have been dropped. IDs belong to the page
and continue across navigation. Failed loads and other messages the engine
writes itself are not in this log.

## Screenshots

`browser_screenshot` returns a PNG image of the page's current viewport, without
browser chrome. Use it to inspect layout, images, charts or canvas content.
Scroll to capture another part of the page. It accepts the same page target
as `browser_read`.

## Debugging

`browser_evaluate` takes a JavaScript expression in `script`, evaluates it in
the main frame and returns JSON. Promises are awaited. For statements, use an
IIFE: `(async () => { const value = await fetch('/api/status'); return await
value.json(); })()`. Thrown errors include their JavaScript stack when present.
Results must fit in 16000 characters; return selected fields or a small slice
of a collection. `undefined` becomes `null`. The timeout is ten seconds; it
stops waiting but does not cancel running JavaScript.

Evaluation can change the page, access its storage or send requests using its
signed-in session. It is available with the browser-action tools.

`browser_inspect` takes either an `element` number from the last read or a CSS
`selector`. A selector returns the first match, including hidden, disabled and
non-interactive elements. Inspection reports bounds, computed styles,
visibility, disabled state, ancestors, scroll containers and the hit-test
stack at the element's center. Ancestors and scroll containers are limited to
eight each; text, attributes and hit-test entries are also bounded. Inspection
does not scroll or change the element. Selectors address the main document.
