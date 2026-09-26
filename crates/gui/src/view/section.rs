//! Where the settings window opens to. Named here, apart from the window, so a
//! build without the window can still say where a button would have gone.

/// Which section the sidebar has selected.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Appearance,
    // With Appearance, because the two answer the same question — how the app
    // meets you — and before the three that answer what it does.
    Shortcuts,
    // Before Agents, because it is what decides whether agents matter: with
    // sessions off, nothing installed under Agents can be launched.
    Features,
    Agents,
    // After Agents, because tools come after the things that use them — the
    // same reading that puts Features before it.
    Mcp,
    Performance,
    // Last, and in a debug build alone — see [`Section::listed`].
    Developer,
}
