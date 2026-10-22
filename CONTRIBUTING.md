# Contributing

How code is written here — docs and comments, Rust style, where UI code goes —
is in [AGENTS.md](AGENTS.md). It holds for code written by hand and by an agent
alike; point your agent at that file. This one covers getting a change in.

## Pull requests

- Target `main`, one change per pull request.
- A pull request is squash-merged, so its title is the commit: Conventional
  Commits, `type(scope): summary`, one line. No version number in the title.
- The body is a few lines or short bullets: what changed and how it was tested.
- No AI attribution anywhere: no `Co-Authored-By: Claude …`, no "Generated with
  …", no badge or footer, in commits, pull request bodies, comments or issues.
- Never add, edit or delete CI (`.github/workflows/`, `.github/scripts/`,
  anything a CI run executes) in a pull request without asking in an issue
  first.
- The CLA check must pass before a pull request is merged.

## Before you push

CI runs these with `RUSTFLAGS=-D warnings`, and builds on Linux and Windows.
Run them first:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

Run tests with `cargo nextest run` (`cargo install cargo-nextest`), filtered to
the crates and tests the change touches.

A change you could not check says so in the pull request, with what was left
unchecked.

## Running the app

`cargo run` starts it. [fixtures](https://github.com/crabtalk/fixtures) is a
demo project to open it against.

On macOS, some features need the app bundle rather than a bare binary. System
notifications are one: a binary run from `cargo run` has no identity to post
under. Build the bundle with `make bundle`, move `target/bundle/cydonia.app`
into an Applications folder and open it from there; a bundle launched from
anywhere else, `/tmp` included, never gets the authorization prompt.
