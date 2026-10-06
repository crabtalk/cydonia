---
title: Backups and rolling back
description: What a migration keeps of the files it replaces, and how to go back to the release they belong to.
---

When a new release changes how a project or the configuration is stored, it copies every file it is about to replace into `~/.local/share/cydonia/backup/`, under the release those files belong to:

```
~/.local/share/cydonia/backup/v0_1_26/
  projects/-Users-me-repos-site/
    path                 # the project's path: /Users/me/repos/site
    entries.db
    boards/<id>.toml
  config/
    settings.toml
```

A file is copied once, before its first rewrite, so the backup is always what the older release wrote. Only the files a migration replaces are kept; sessions, articles and pictures are not copied, because no migration rewrites them.

## Settings › Backups

The section is listed while a backup exists. Each backup is a row with three actions:

- **Reveal in Finder** opens the backup's directory.
- **Roll back** downloads that release and quits. Once Cydonia has quit, the backed-up files are put back and the older release opens. Anything made or changed since the migration — cards, boards, entry numbers — is lost.
- **Delete** removes the backup. Rolling back to that release is no longer possible afterwards.

Backups are a few kilobytes each and are never deleted on their own.

## Rolling back by hand

For a build that cannot roll back itself — one installed with `cargo install`, or run from a working copy:

1. Quit Cydonia, so nothing rewrites a project while you restore it.
2. Install the release the backup is named after, from its page on [GitHub releases](https://github.com/crabtalk/cydonia/releases) — `v0_1_26` is `v0.1.26`.
3. For each directory under `projects/`, copy everything but `path` into `<project>/.cydonia/`, `<project>` being the path written in `path`. Then delete `<project>/.cydonia/state.db`, and the directory `<project>/.cydonia/boards/<id>/` of each `boards/<id>.toml` you copied back.
4. Copy everything under `config/` into `~/.config/cydonia/`.
5. Open the older release.

The older release still checks for updates and offers the newer one again. Installing it migrates the projects again, taking a fresh backup first.
