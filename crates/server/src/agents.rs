//! The `agents` Cupola service: ACP agents installed on the server's machine,
//! by catalog id, from the catalog that machine fetches. [`Remote`] is the
//! client's side.

use cupola::{Client, Emitter, Error, Handle, Server, Service, lanes::Lanes};
use cydonia_acp_agents::Installed;
use futures::{
    FutureExt as _, StreamExt as _,
    future::{self, Either},
};
use serde::{Deserialize, Serialize};

/// Agents on the server's machine. The catalog is cached in
/// [`artifact::backup::cache_dir`] and agents are installed under
/// [`artifact::backup::data_dir`], both the server machine's.
pub struct Agents;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// The agent of this catalog id, installed first when it has no install
    /// record. Answered with its install record.
    Resolve(String),
}

/// One line an install printed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// The catalog id of the agent being installed.
    pub agent: String,
    pub line: String,
}

/// The [`Agents`] of the server one client reaches.
#[derive(Clone)]
pub struct Remote {
    handle: Handle<Agents>,
}

impl Service for Agents {
    const NAME: &'static str = "agents";
    type Request = Request;
    type Response = Installed;
    type Event = Event;
}

impl Agents {
    /// Answers [`Agents`] requests on `server`, on one [`Lanes`] lane per
    /// agent id: one agent's requests run one at a time, in the order they
    /// were sent.
    pub fn register(server: &Server) {
        let emitter = server.emitter::<Self>();
        let lanes = Lanes::default();
        server.register::<Self, _, _>(move |request: Request| {
            let emitter = emitter.clone();
            let answered = lanes.run(request.agent().to_owned(), move || request.answer(&emitter));
            async move { answered.await? }
        });
    }
}

impl Request {
    /// The catalog id this request is about.
    pub fn agent(&self) -> &str {
        match self {
            Self::Resolve(agent) => agent,
        }
    }

    /// Blocking: an install runs `npm` or downloads a release.
    fn answer(&self, emitter: &Emitter<Agents>) -> anyhow::Result<Installed> {
        match self {
            Self::Resolve(agent) => {
                let cache = artifact::backup::cache_dir()?;
                let data = artifact::backup::data_dir()?;
                Installed::resolve(&cache, &data, agent, |line| {
                    let _ = emitter.emit(&Event {
                        agent: agent.clone(),
                        line: line.to_owned(),
                    });
                })
            }
        }
    }
}

impl Remote {
    pub fn new(client: &Client) -> Self {
        Self {
            handle: client.service::<Agents>(),
        }
    }

    /// The install record of agent `id` on the server, which installs it
    /// first when it has none. `on_line` is handed each line an install of
    /// `id` prints while this waits, before this resolves.
    pub async fn resolve(
        &self,
        id: &str,
        mut on_line: impl FnMut(&str),
    ) -> Result<Installed, Error> {
        let mut events = self.handle.events();
        let mut heard = |event: Event| {
            if event.agent == id {
                on_line(&event.line);
            }
        };
        let mut answer = self.handle.send(&Request::Resolve(id.to_owned())).boxed();
        loop {
            match future::select(answer, events.next()).await {
                Either::Left((answered, next)) => {
                    drop(next);
                    // An install's lines reach this side before its answer.
                    while let Some(Some(Ok(event))) = events.next().now_or_never() {
                        heard(event);
                    }
                    return answered;
                }
                Either::Right((Some(Ok(event)), pending)) => {
                    heard(event);
                    answer = pending;
                }
                Either::Right((_, pending)) => return pending.await,
            }
        }
    }
}
