//! Where the shapes come from and go back to.
//!
//! A backend is what turns a project into the boards, sessions and articles in
//! it. [`fs`] is the one cydonia ships — files under the project's own
//! `.cydonia/` — and the reason the rest of this crate names no directory and
//! opens no file: what a reader gets is the same shape whichever backend
//! answered.

pub mod fs;
