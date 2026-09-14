//! The one chord the app does not own: the key macOS holds on its behalf, so
//! that pressing it inside some other application brings cydonia forward.
//!
//! Not a keymap binding, and it cannot be one. gpui only sees a keystroke while
//! one of our windows has it, and the whole point of this key is the moment
//! when none of them does. So it is registered with the system instead —
//! Carbon's `RegisterEventHotKey`, underneath [`global_hotkey`] — which asks no
//! permission of the user, unlike a global event monitor, and which is the same
//! door every launcher on the desktop is already queueing at.
//!
//! That queue is the failure this module exists to report. A chord another app
//! registered first is refused outright and says so; a chord macOS itself has
//! claimed — ⌃Space is the input-source switch, and is the one most people
//! reach for here — registers cleanly and then never fires, because the system
//! took it upstream of us. Nothing can detect the second case, which is why
//! nothing here ships on by default: [`crate::model::settings::Shortcuts`]
//! holds no `activate` until somebody asks for one.

use crate::{
    model::{settings, state},
    view::root::{self, Cydonia},
};
use bezel::gpui::{App, Global, Keystroke, SharedString, Task};
use futures::StreamExt;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use std::str::FromStr;

/// What the app is holding from the system, and what went wrong getting it.
struct Held {
    /// `None` where the system would not give us a manager at all — a build
    /// with no event loop on the main thread, which is not this one, but the
    /// call returns a `Result` and an app that unwrapped it would die at
    /// launch over a shortcut.
    manager: Option<GlobalHotKeyManager>,
    /// What is registered right now, so it can be handed back before the next
    /// one is taken.
    key: Option<HotKey>,
    refused: Option<SharedString>,
    /// The task turning presses into activations. Held so it lives as long as
    /// the app rather than until the end of [`apply`].
    _pump: Task<()>,
}

impl Global for Held {}

/// Hold `chord`, letting go of whatever was held before. `None` lets go and
/// takes nothing.
///
/// Called at launch and again whenever the row in Settings moves, so it has to
/// be the whole transaction rather than a register: the old key stays live
/// until it is explicitly given back.
pub fn apply(chord: Option<&str>, cx: &mut App) {
    if !cx.has_global::<Held>() {
        install(cx);
    }
    let wanted = chord.and_then(translate);
    let refused = match (chord, wanted.as_ref()) {
        // Said, and not a chord this platform can hold. A trackpad-era key
        // like `fn` has no Carbon code, and neither has a two-stroke sequence.
        (Some(_), None) => Some(SharedString::from("Not a chord the system can hold.")),
        _ => None,
    };
    let held = cx.global_mut::<Held>();
    let Some(manager) = held.manager.as_ref() else {
        held.refused = Some(SharedString::from("The system refused the shortcut."));
        return;
    };
    if let Some(key) = held.key.take() {
        let _ = manager.unregister(key);
    }
    held.refused = refused;
    let Some(wanted) = wanted else {
        return;
    };
    match manager.register(wanted) {
        Ok(()) => held.key = Some(wanted),
        // Almost always another app that got there first — the registration is
        // desktop-wide and first come, first served.
        Err(_) => held.refused = Some(SharedString::from("Another app is already holding it.")),
    }
}

/// Why the chord in the file is not being held, when it is not.
///
/// Silence is not the same as success and this cannot tell you which you have:
/// a chord the system claimed for itself registers without complaint and then
/// never arrives. What is reported is only what registration knew.
pub fn refused(cx: &App) -> Option<SharedString> {
    cx.try_global::<Held>()?.refused.clone()
}

/// Take the manager and start draining its events.
///
/// Once per launch. The handler runs on the thread the system delivers on, so
/// it does no more than post — the activation happens back on the app's own
/// turn, where there is a window to bring forward.
fn install(cx: &mut App) {
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        // Press and release both arrive; acting on the release would fire the
        // second time a held key let go.
        if event.state() == HotKeyState::Pressed {
            let _ = tx.unbounded_send(());
        }
    }));
    let pump = cx.spawn(async move |cx| {
        while rx.next().await.is_some() {
            cx.update(activate);
        }
    });
    cx.set_global(Held {
        manager: GlobalHotKeyManager::new().ok(),
        key: None,
        refused: None,
        _pump: pump,
    });
}

/// Bring the app forward, opening a window if ⌘W left it with none — the same
/// door the Dock opens, and the reason this is more than `activate_window`.
fn activate(cx: &mut App) {
    cx.activate(true);
    if let Some(handle) = cx
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<Cydonia>())
    {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let Ok(settings) = settings::load() else {
        return;
    };
    let _ = root::open(settings, state::restore(), cx);
}

/// Whether the system could hold `chord` at all — asked by the settings row
/// before it writes one, so a chord that could never be registered is refused
/// where it is pressed rather than reported once it is in the file.
pub fn holdable(chord: &str) -> bool {
    translate(chord).is_some()
}

/// A gpui chord as the system wants it, or nothing where the two vocabularies
/// do not meet.
///
/// One keystroke only. A sequence is something an app can dispatch because it
/// can hold the first key and wait; the system registers a single combination
/// and has nowhere to put the waiting.
fn translate(chord: &str) -> Option<HotKey> {
    if chord.split_whitespace().count() != 1 {
        return None;
    }
    let stroke = Keystroke::parse(chord.trim()).ok()?;
    let mut modifiers = Modifiers::empty();
    modifiers.set(Modifiers::CONTROL, stroke.modifiers.control);
    modifiers.set(Modifiers::ALT, stroke.modifiers.alt);
    modifiers.set(Modifiers::SHIFT, stroke.modifiers.shift);
    modifiers.set(Modifiers::SUPER, stroke.modifiers.platform);
    // Every one of these is an app-wide key with no window to be in. Without a
    // modifier it would be that key, everywhere, for every application on the
    // desktop — which is not a shortcut, it is a broken keyboard.
    if modifiers.is_empty() {
        return None;
    }
    Some(HotKey::new(Some(modifiers), code(&stroke.key)?))
}

/// gpui's name for a key, as the W3C code the system is addressed in.
fn code(key: &str) -> Option<Code> {
    let named = match key {
        "space" => "Space",
        "enter" => "Enter",
        "escape" => "Escape",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "insert" => "Insert",
        "," => "Comma",
        "." => "Period",
        "/" => "Slash",
        ";" => "Semicolon",
        "'" => "Quote",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        "\\" => "Backslash",
        "-" => "Minus",
        "=" => "Equal",
        "`" => "Backquote",
        // A letter, a digit or a function key, all of which the code is spelled
        // out of rather than listed.
        key => return spelled(key),
    };
    Code::from_str(named).ok()
}

/// The three families whose codes follow from the key itself.
fn spelled(key: &str) -> Option<Code> {
    let spelling = match key.as_bytes() {
        [letter @ b'a'..=b'z'] => format!("Key{}", letter.to_ascii_uppercase() as char),
        [digit @ b'0'..=b'9'] => format!("Digit{}", *digit as char),
        [b'f', ..] if key[1..].parse::<u8>().is_ok() => key.to_uppercase(),
        _ => return None,
    };
    Code::from_str(&spelling).ok()
}
