//! A session, as the project keeps it: the record it is filed as, and the
//! transcript inside it.
//!
//! Split because they are read at different times. A sidebar lists every
//! [`record`] in a project and shows none of their [`chat`]; the pane that
//! opens one reads the rest.

pub mod chat;
pub mod record;
