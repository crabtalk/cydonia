# The welcome project

The entries a fresh install opens on. This directory *is* a cydonia project:
open it in the app and edit the articles and the board in place, and git sees
what you changed.

Its `.cydonia/.gitignore` holds `*.db` rather than the `*` that
`fs::Project::init` writes, which is what lets the content be committed at all
— `init` leaves an ignore file that is already there alone. The seed must not
copy that file into the project it creates; `init` writes the real one.

Nothing else here is excluded, so keep the databases out by hand if one turns
up under another name: `data.db` and `entries.db` belong to a machine.

An entry's id is the name of the file it is in, and a board's `id` field
repeats it. Renaming one makes a different entry.

Entries are listed newest first, by file modification time — `artifact::stamp::of`,
and the sorts in `artifact::project::fs` and `model::article`. Git does not
carry mtimes, so the order a reader sees is set when the seed is written and
cannot be expressed here. Reading order is `Start here`, `Writing in Cydonia`,
`Getting started`; the seed writes them in the reverse of it.

Copy into this directory with `cp -p`. Without it every file takes the time of
the copy, the mtimes tie, and the order becomes whatever the sort settles on.
