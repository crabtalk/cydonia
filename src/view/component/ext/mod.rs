//! Components that belong to bezel and are not there yet.
//!
//! Everything under here is written as bezel would write it — generic over the
//! view, built from bezel's own primitives, taking no part of cydonia with it —
//! so that landing one upstream is a move and a changed `use`, not a rewrite.
//!
//! Nothing else in the app should grow a dependency on the *shape* of these
//! beyond what bezel would offer. When a file here goes, it goes whole.

pub mod submenu;
