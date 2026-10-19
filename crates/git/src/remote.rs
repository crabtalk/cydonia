//! The backend that asks the [`Git`] service of the server a Cupola client
//! reaches. Paths are the server's.

use crate::{
    Backend, Change, Repository, Sources,
    service::{Answer, Git, Request},
};
use anyhow::{Result, anyhow};
use cupola::{Client, Handle};
use futures::{FutureExt as _, future::BoxFuture};
use std::path::Path;

/// Git on the server one client reaches. Each call is one request, sent when
/// it is made.
#[derive(Clone)]
pub struct Remote {
    handle: Handle<Git>,
}

impl Remote {
    pub fn new(client: &Client) -> Self {
        Self {
            handle: client.service::<Git>(),
        }
    }

    fn ask(&self, request: &Request) -> BoxFuture<'static, Result<Answer>> {
        let answer = self.handle.send(request);
        async move { answer.await?.map_err(|message| anyhow!(message)) }.boxed()
    }

    fn unexpected() -> anyhow::Error {
        anyhow!("the server answered another call")
    }
}

impl Backend for Remote {
    fn status(&self, dir: &Path) -> BoxFuture<'static, Result<Option<Repository>>> {
        self.ask(&Request::Status {
            dir: dir.to_owned(),
        })
        .map(|answer| match answer? {
            Answer::Status(status) => Ok(status),
            _ => Err(Self::unexpected()),
        })
        .boxed()
    }

    fn diff(&self, root: &Path, change: &Change) -> BoxFuture<'static, Result<String>> {
        self.ask(&Request::Diff {
            root: root.to_owned(),
            change: change.clone(),
        })
        .map(|answer| match answer? {
            Answer::Diff(patch) => Ok(patch),
            _ => Err(Self::unexpected()),
        })
        .boxed()
    }

    fn sources(&self, root: &Path, change: &Change) -> BoxFuture<'static, Sources> {
        self.ask(&Request::Sources {
            root: root.to_owned(),
            change: change.clone(),
        })
        .map(|answer| match answer {
            Ok(Answer::Sources(sources)) => sources,
            _ => Sources::default(),
        })
        .boxed()
    }
}
