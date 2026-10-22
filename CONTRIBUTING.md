# Contributing

The rules a change follows — commits, docs and comments, Rust style, where UI
code goes — are in [AGENTS.md](AGENTS.md). They hold for code written by hand
and by an agent alike; point your agent at that file.

## Pull requests

- Open them against `main`.
- One change per pull request. The title becomes the squashed commit, so write
  it in Conventional Commits form, `type(scope): summary`, with no version
  number.
- Keep the body short: what changed, and how you tested it.
- No AI attribution in commits, pull requests or issues.
- The CLA check must pass before a pull request can be merged.

## Checks

CI runs `cargo fmt --all --check` and `cargo clippy --workspace --all-targets`
with warnings as errors, and builds on Linux and Windows. Run the first two
before pushing, and the tests the change touches with `cargo nextest run`.

## Running the app

`cargo run` starts it. [fixtures](https://github.com/crabtalk/fixtures) is a
demo project to open it against.

On macOS, some features need the app bundle rather than a bare binary. System
notifications are one: a binary run from `cargo run` has no identity to post
under. Build the bundle with `make bundle`, move `target/bundle/cydonia.app`
into an Applications folder and open it from there; a bundle launched from
anywhere else, `/tmp` included, never gets the authorization prompt.
