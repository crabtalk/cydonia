use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Home {
    Project(PathBuf),
    App(PathBuf),
}

#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(feature = "sqlite")]
pub use sqlite::Data;
#[cfg(not(feature = "sqlite"))]
mod none;
#[cfg(not(feature = "sqlite"))]
pub use none::Data;
