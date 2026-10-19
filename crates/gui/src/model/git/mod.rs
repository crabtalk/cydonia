//! The git backend a directory's repository is read through.
//!
//! [`open`] is the one place the gui picks a backend.

use artifact::location::Location;
use std::sync::Arc;

pub use ::git::{Area, Backend, Change, Repository, Sources};

pub mod preview;

/// The backend for the repositories on the machine `location` is on:
/// [`Unreachable`] for a location on a host whose client cannot be had.
pub fn open(location: &Location) -> Arc<dyn Backend> {
    match location {
        Location::Local(_) => Arc::new(::git::Local),
        #[cfg(feature = "desktop")]
        Location::Remote { host, .. } => {
            match crate::model::remote::Remotes::get().and_then(|r| r.client(*host)) {
                Ok(client) => Arc::new(::git::Remote::new(&client)),
                Err(error) => Arc::new(Unreachable(format!("{error:#}").into())),
            }
        }
    }
}

/// The repositories of a host whose client could not be had: every call is
/// refused with why.
#[cfg(feature = "desktop")]
struct Unreachable(Arc<str>);

#[cfg(feature = "desktop")]
impl Backend for Unreachable {
    fn status(
        &self,
        _: &std::path::Path,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Option<Repository>>> {
        Box::pin(std::future::ready(Err(anyhow::anyhow!("{}", self.0))))
    }

    fn diff(
        &self,
        _: &std::path::Path,
        _: &Change,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<String>> {
        Box::pin(std::future::ready(Err(anyhow::anyhow!("{}", self.0))))
    }

    fn sources(
        &self,
        _: &std::path::Path,
        _: &Change,
    ) -> futures::future::BoxFuture<'static, Sources> {
        Box::pin(std::future::ready(Sources::default()))
    }
}
