# patches

Repo-local copies of dependencies, applied through `[patch.crates-io]` in
`Cargo.toml`. Each is the published crate, at the version `Cargo.lock` names,
with the smallest change that could not be made from cydonia's side. Every
edited line is marked `(cydonia: Windows support.)` so the diff against the
registry copy reads on its own.

A patch is a loan, not a fork: it is meant to go upstream and come out of
here. `cargo publish` strips `[patch]` from the manifest it ships, so
`cargo install cydonia` builds against the registry crate and does not get
these — which is one more reason to send them up.

## cacp-agents 0.1.0

Installing an agent from **Settings › Agents** runs `npm`, and unpacks a
binary release with `tar`. The crate finds both with a lookup that joins each
`PATH` directory with the bare name and checks for a file — which on Windows
finds `npm` (npm ships a POSIX shell script under that name beside `npm.cmd`)
and then fails to run it, and never finds `tar` at all (`tar.exe`).

- `src/utils.rs` — `which` walks `PATHEXT` as `cmd.exe` does, and a `command`
  helper spawns the resolved file without a console window.
- `src/install.rs` — `npm`, `tar` and `unzip` are spawned through what `which`
  found rather than by name; an npm-installed agent's recorded command is the
  `.cmd` launcher on Windows, where the bare one is a script nothing can run.

Nothing else in the crate is touched; the tests it ships are its own.
