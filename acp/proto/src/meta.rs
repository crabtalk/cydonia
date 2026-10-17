//! `_meta` — the spec's extension slot, present on every message.

/// Whatever either side wants to attach to a message, under `_meta`.
///
/// This is where implementations put what the spec does not cover yet: Claude
/// negotiates steering through `_meta.steering`, and both it and Codex carry
/// terminal detail there. cydonia-acp reads and writes it untouched.
pub type Meta = serde_json::Map<String, serde_json::Value>;
