---
title: Getting started
description: Install Cydonia, open a directory as a project, and write the first article.
---

## Install

Download the disk image from [cydonia.sh](https://cydonia.sh), or build it from crates.io:

```sh
cargo install cydonia
cydonia
```

Cydonia runs on macOS on Apple silicon. It asks for no account and reaches the
network only to look for a new release and to install an agent you asked for.

## Open a project

**File › Open Project…** (⌘O) picks a directory. Anything works — a repository,
a notes folder, a directory that is empty for now. Cydonia creates `.cydonia/`
inside it the first time something is written, and that directory carries a
`.gitignore`: none of what Cydonia writes there is your project's source.

Several projects can be open at once. They stack in the sidebar, and a session
started in one can work in any of them.

## Write something

**File › New Article** opens a document. It is Markdown — headings, code fences,
tables, task lists, images with captions — rendered as you type, with ⌘E to
switch to the source and back. See [Articles](../working/articles.md) for what the
syntax supports.

**File › New Board** gives you cards in columns for anything that is a plan
rather than a document.

## Turn on sessions

A fresh install has no agents running. Open **Settings › Features** (⌘,) and
turn on `sessions`, then **Settings › Agents** to install one from the ACP
registry or point Cydonia at a command you already have.

With an agent installed, ⌘N starts a session in the focused project. See
[Sessions](../agents/sessions.md).
