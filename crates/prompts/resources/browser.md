---
name: browser
description: Work in the in-app browser through the browser_* tools — opening pages, reading them, clicking and typing. Use before driving a browser tab.
---

# The in-app browser

The browser is the user's own: the tabs in the project's right panel, signed in
wherever they are signed in. It reaches pages a plain fetch gets a login wall or
an empty shell from, and it runs a page's scripts, so a read is what is on
screen. For an ordinary public page, a plain fetch is cheaper and enough.

Tabs belong to a project. `browser_tabs` lists the project's browser tabs; the
one marked front is the one the user is looking at, and the one you get when you
name no tab. `browser_open` without a tab opens a new one in front of the
project's right panel, where the user can watch it and take it over.

A tab's page exists only once its panel has been on screen. A call on a tab
whose panel is not showing answers that it cannot load; say so rather than
retrying.

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

`browser_console` gives what a tab's page has logged since the tab opened:
console calls, uncaught errors and unhandled rejections, each with the frame
that logged it. Failed loads and other messages the engine writes itself are
not in it.
