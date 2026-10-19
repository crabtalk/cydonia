//! The `git` Cupola service: [`Backend`] calls on repositories another
//! machine holds. A server answers them with [`Local`]; [`crate::Remote`] is
//! the client's side.

use crate::{Backend, Change, Local, Repository, Sources};
use cupola::{Server, Service};
use futures::{FutureExt as _, future::BoxFuture};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Git, run where the repository is.
pub struct Git;

/// The [`Backend`] calls, and what each is answered with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Answered with [`Answer::Status`].
    Status {
        #[serde(with = "cupola::bytes::path")]
        dir: PathBuf,
    },
    /// Answered with [`Answer::Diff`].
    Diff {
        #[serde(with = "cupola::bytes::path")]
        root: PathBuf,
        change: Change,
    },
    /// Answered with [`Answer::Sources`].
    Sources {
        #[serde(with = "cupola::bytes::path")]
        root: PathBuf,
        change: Change,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Answer {
    Status(Option<Repository>),
    Diff(String),
    Sources(Sources),
}

impl Service for Git {
    const NAME: &'static str = "git";
    type Request = Request;
    /// `Err` carries the `{:#}` text of what the backend failed with.
    type Response = Result<Answer, String>;
    type Event = ();
}

impl Git {
    /// Answers [`Git`] requests on `server` with [`Local`].
    pub fn register(server: &Server) {
        server.register::<Self, _, _>(|request: Request| request.answer(&Local).map(Ok));
    }
}

impl Request {
    /// What `backend` answers this request with.
    pub fn answer(self, backend: &dyn Backend) -> BoxFuture<'static, Result<Answer, String>> {
        let failed = |error: anyhow::Error| format!("{error:#}");
        match self {
            Self::Status { dir } => backend
                .status(&dir)
                .map(move |status| status.map(Answer::Status).map_err(failed))
                .boxed(),
            Self::Diff { root, change } => backend
                .diff(&root, &change)
                .map(move |patch| patch.map(Answer::Diff).map_err(failed))
                .boxed(),
            Self::Sources { root, change } => backend
                .sources(&root, &change)
                .map(|sources| Ok(Answer::Sources(sources)))
                .boxed(),
        }
    }
}
