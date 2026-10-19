//! A project another machine holds, reached through the [`Artifact`] service.
//!
//! Reads answer from a replica, a [`memory::Project`] filled with everything
//! the server holds when the project is opened and again on every knock. The
//! replica holds no assets: an asset is a request, answered by the server. A
//! write is a request: its future resolves when the server answers, and the
//! replica then takes the entry the server made, with its id, number and
//! versions. Entry numbers are the server's.
//!
//! An answer applies to the replica only when it is newer than the fill the
//! replica was last made from: requests reach the server in the order they
//! are sent, and every answer is ordered by when its request was sent.

use super::{
    Owner, Watching, fs, memory,
    service::{Answer, ArticleEntry, Artifact, Call, Everything, Knock, Request},
};
use crate::{
    article::{Article, comments::Comments, properties::Properties},
    board::Board,
    error::Error,
    kind::Kind,
    session::record::Record,
    stamp,
};
use cupola::{Client, Events, Handle};
use futures::{FutureExt as _, StreamExt as _, future::BoxFuture};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, Weak},
};

/// Keeps a [`Project`]'s replica fresh: pulls everything again on each knock
/// for its root, and on the server being started afresh, then knocks every
/// [`super::Project::watch`]. Ends when the project is dropped or the client
/// closed.
pub type Refresh = BoxFuture<'static, ()>;

type Knocker = Arc<dyn Fn() + Send + Sync>;

/// The project at one root on one server. Clones are the same project.
#[derive(Clone)]
pub struct Project {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    client: Client,
    handle: Handle<Artifact>,
    replica: Mutex<Replica>,
    knocks: Mutex<Knocks>,
}

struct Replica {
    held: Arc<memory::Project>,
    /// Requests sent, which numbers each one.
    sent: u64,
    /// The request the replica was last filled from.
    filled: u64,
    /// The fills sent and not yet answered.
    filling: Vec<u64>,
    /// Changes a fill in `filling` was sent before, by their request.
    since: Vec<(u64, Change)>,
}

/// What one answer does to the replica.
#[derive(Clone)]
enum Change {
    Board(Board),
    BoardGone(String),
    Session(Record),
    SessionGone(String),
    Article(ArticleEntry),
    ArticleGone(String),
    Number(Kind, String, u64),
    Retired(Kind, String),
}

#[derive(Default)]
struct Knocks {
    next: u64,
    held: Vec<(u64, Knocker)>,
}

/// Takes its knock out when dropped.
struct Knocking {
    inner: Weak<Inner>,
    id: u64,
}

impl Project {
    /// The project at `root` on the server `client` reaches, filled. Its
    /// replica is kept fresh only while the [`Refresh`] is polled.
    pub async fn open(client: Client, root: PathBuf) -> super::Result<(Self, Refresh)> {
        let handle = client.service::<Artifact>();
        let events = handle.events();
        let inner = Arc::new(Inner {
            root,
            client,
            handle,
            replica: Mutex::new(Replica {
                held: Arc::default(),
                sent: 0,
                filled: 0,
                filling: Vec::new(),
                since: Vec::new(),
            }),
            knocks: Mutex::default(),
        });
        inner.fill().await?;
        let refresh = Inner::refresh(Arc::downgrade(&inner), events).boxed();
        Ok((Self { inner }, refresh))
    }

    /// The project's directory on the server's machine.
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// The project's tables, on the same server.
    pub fn tables(&self) -> crate::table::remote::Tables {
        crate::table::remote::Tables::new(&self.inner.client, self.inner.root.clone())
    }

    /// The directory `owner`'s pictures are in on the server's machine,
    /// whether or not any have been written. Refused for an article or a
    /// board the replica does not hold.
    pub fn asset_dir(&self, owner: &Owner) -> anyhow::Result<PathBuf> {
        use super::Project as _;
        let held = self.held();
        match owner {
            Owner::Article(id) => anyhow::ensure!(held.article(id).is_some(), "no article {id}"),
            Owner::Page { board, .. } => {
                anyhow::ensure!(held.board(board).is_some(), "no board {board}")
            }
            Owner::Shared => {}
        }
        self.layout().asset_path(owner)
    }

    /// The project's files as they are laid out at its root on the server's
    /// machine.
    fn layout(&self) -> fs::Project {
        fs::Project::new(&self.inner.root)
    }

    fn held(&self) -> Arc<memory::Project> {
        self.inner.replica().held.clone()
    }

    /// Sends `call`, and answers what `take` makes of the server's answer,
    /// once the change it names is in the replica.
    fn request<T: Send + 'static>(
        &self,
        call: Call,
        take: impl FnOnce(Answer) -> Option<(T, Option<Change>)> + Send + 'static,
    ) -> BoxFuture<'static, super::Result<T>> {
        let (sent, answer) = self.inner.send(call);
        let inner = self.inner.clone();
        async move {
            let answer = answer.await?;
            let (value, change) = take(answer).ok_or_else(Inner::unexpected)?;
            if let Some(change) = change {
                inner.apply(sent, change);
            }
            Ok(value)
        }
        .boxed()
    }

    /// A write answered with [`Answer::Done`], making `change`.
    fn done(&self, call: Call, change: Option<Change>) -> BoxFuture<'static, super::Result<()>> {
        self.request(call, |answer| {
            matches!(answer, Answer::Done).then_some(((), change))
        })
    }

    /// A write answered with [`Answer::Article`].
    fn article_write(&self, call: Call) -> BoxFuture<'static, super::Result<()>> {
        self.request(call, |answer| match answer {
            Answer::Article(entry) => Some(((), Some(Change::Article(entry)))),
            _ => None,
        })
    }
}

impl Inner {
    fn replica(&self) -> MutexGuard<'_, Replica> {
        self.replica
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Sends `call` now, numbered after every request sent before it.
    fn send(
        &self,
        call: Call,
    ) -> (
        u64,
        impl Future<Output = super::Result<Answer>> + Send + use<>,
    ) {
        let request = Request {
            root: self.root.clone(),
            call,
        };
        let fills = matches!(request.call, Call::Everything);
        let mut replica = self.replica();
        replica.sent += 1;
        let sent = replica.sent;
        if fills {
            replica.filling.push(sent);
        }
        let answer = self.handle.send(&request);
        let answer = async move {
            answer
                .await
                .map_err(|error| Error::Other(error.into()))?
                .map_err(Error::from)
        };
        (sent, answer)
    }

    /// Puts what answered request `sent` into the replica, unless the replica
    /// was filled from a later request.
    fn apply(&self, sent: u64, change: Change) {
        let mut replica = self.replica();
        if sent <= replica.filled {
            return;
        }
        change.clone().apply(&replica.held);
        if replica.filling.iter().any(|fill| *fill < sent) {
            replica.since.push((sent, change));
        }
    }

    /// Fills the replica with everything the server holds.
    async fn fill(&self) -> super::Result<()> {
        let (sent, answer) = self.send(Call::Everything);
        let answer = answer.await;
        let mut replica = self.replica();
        replica.filling.retain(|fill| *fill != sent);
        let Answer::Everything(everything) = answer? else {
            return Err(Self::unexpected());
        };
        if sent > replica.filled {
            let held = Self::hold(everything);
            replica.since.sort_by_key(|(answered, _)| *answered);
            for (_, change) in replica
                .since
                .iter()
                .filter(|(answered, _)| *answered > sent)
            {
                change.clone().apply(&held);
            }
            replica.held = Arc::new(held);
            replica.filled = sent;
        }
        let Replica { filling, since, .. } = &mut *replica;
        since.retain(|(answered, _)| filling.iter().any(|fill| fill < answered));
        Ok(())
    }

    fn hold(everything: Everything) -> memory::Project {
        let held = memory::Project::new();
        for (number, kind, id) in everything.numbers {
            held.hold_number(kind, &id, number);
        }
        for board in everything.boards {
            held.hold_board(board.into());
        }
        for session in everything.sessions {
            held.hold_session(session.into());
        }
        for article in everything.articles {
            Change::Article(article).apply(&held);
        }
        held
    }

    async fn refresh(inner: Weak<Self>, mut events: Events<Artifact>) {
        while let Some(event) = events.next().await {
            let Some(this) = inner.upgrade() else {
                return;
            };
            let mut due = this.concerns(&event);
            while let Some(Some(event)) = events.next().now_or_never() {
                due |= this.concerns(&event);
            }
            if due && this.fill().await.is_ok() {
                this.knock();
            }
        }
    }

    fn concerns(&self, event: &Result<Knock, cupola::Error>) -> bool {
        match event {
            Ok(knock) => knock.root == self.root,
            Err(_) => true,
        }
    }

    fn knock(&self) {
        let knocks: Vec<Knocker> = self
            .knocks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .held
            .iter()
            .map(|(_, knock)| knock.clone())
            .collect();
        for knock in knocks {
            knock();
        }
    }

    fn unexpected() -> Error {
        Error::Other(anyhow::anyhow!("the server answered another call"))
    }
}

impl Change {
    fn apply(self, held: &memory::Project) {
        use super::Project as _;
        match self {
            Self::Board(board) => held.hold_board(board),
            Self::BoardGone(id) => drop(held.remove_board(&id)),
            Self::Session(record) => held.hold_session(record),
            Self::SessionGone(id) => drop(held.remove_session(&id)),
            Self::Article(entry) => held.hold_article(
                &entry.article(),
                entry.markdown,
                &crate::article::properties::parse(&entry.properties),
                entry.comments,
            ),
            Self::ArticleGone(id) => drop(held.remove_article(&id)),
            Self::Number(kind, id, number) => held.hold_number(kind, &id, number),
            Self::Retired(kind, id) => drop(held.retire(kind, &id)),
        }
    }
}

impl Drop for Knocking {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner
                .knocks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .held
                .retain(|(id, _)| *id != self.id);
        }
    }
}

impl super::Project for Project {
    fn boards(&self) -> Vec<Board> {
        self.held().boards()
    }

    fn board(&self, id: &str) -> Option<Board> {
        self.held().board(id)
    }

    fn create_board(
        &self,
        name: &str,
        key: &str,
    ) -> impl Future<Output = super::Result<Board>> + Send + use<> {
        let call = Call::CreateBoard {
            name: name.to_owned(),
            key: key.to_owned(),
        };
        self.request(call, |answer| match answer {
            Answer::Board(entry) => {
                let board = Board::from(entry);
                Some((board.clone(), Some(Change::Board(board))))
            }
            _ => None,
        })
    }

    /// The board takes, at the call, the versions the server gives what it
    /// holds — see [`fs::Project::stamp`].
    fn save_board(
        &self,
        board: &mut Board,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let call = Call::SaveBoard((&*board).into());
        if let Err(error) = fs::Project::stamp(board) {
            return futures::future::ready(Err(error.into())).boxed();
        }
        board.touched = stamp::now();
        self.request(call, |answer| match answer {
            Answer::Board(entry) => Some(((), Some(Change::Board(entry.into())))),
            _ => None,
        })
    }

    fn remove_board(&self, id: &str) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.done(
            Call::RemoveBoard(id.to_owned()),
            Some(Change::BoardGone(id.to_owned())),
        )
    }

    fn sessions(&self) -> Vec<Record> {
        self.held().sessions()
    }

    fn session(&self, id: &str) -> Option<Record> {
        self.held().session(id)
    }

    fn create_session(&self) -> impl Future<Output = super::Result<String>> + Send + use<> {
        self.request(Call::CreateSession, |answer| match answer {
            Answer::Id(id) => Some((id, None)),
            _ => None,
        })
    }

    fn save_session(
        &self,
        record: &Record,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.request(Call::SaveSession(record.into()), |answer| match answer {
            Answer::Session(entry) => Some(((), Some(Change::Session(entry.into())))),
            _ => None,
        })
    }

    fn remove_session(&self, id: &str) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.done(
            Call::RemoveSession(id.to_owned()),
            Some(Change::SessionGone(id.to_owned())),
        )
    }

    fn articles(&self) -> Vec<Article> {
        self.held().articles()
    }

    fn article(&self, id: &str) -> Option<Article> {
        self.held().article(id)
    }

    fn create_article(
        &self,
        markdown: &str,
    ) -> impl Future<Output = super::Result<Article>> + Send + use<> {
        self.request(
            Call::CreateArticle(markdown.to_owned()),
            |answer| match answer {
                Answer::Article(entry) => Some((entry.article(), Some(Change::Article(entry)))),
                _ => None,
            },
        )
    }

    fn read_article(&self, id: &str) -> super::Result<String> {
        self.held().read_article(id)
    }

    fn write_article(
        &self,
        id: &str,
        markdown: &str,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.article_write(Call::WriteArticle {
            id: id.to_owned(),
            markdown: markdown.to_owned(),
        })
    }

    fn comments(&self, id: &str) -> Comments {
        self.held().comments(id)
    }

    fn save_comments(
        &self,
        id: &str,
        comments: &Comments,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.article_write(Call::SaveComments {
            id: id.to_owned(),
            comments: comments.clone(),
        })
    }

    fn properties(&self, id: &str) -> Properties {
        self.held().properties(id)
    }

    fn save_properties(
        &self,
        id: &str,
        properties: &Properties,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.article_write(Call::SaveProperties {
            id: id.to_owned(),
            properties: crate::article::properties::apply("", properties).unwrap_or_default(),
        })
    }

    fn remove_article(&self, id: &str) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.done(
            Call::RemoveArticle(id.to_owned()),
            Some(Change::ArticleGone(id.to_owned())),
        )
    }

    fn asset(
        &self,
        owner: &Owner,
        name: &str,
    ) -> impl Future<Output = super::Result<Vec<u8>>> + Send + use<> {
        let call = Call::Asset {
            owner: owner.clone(),
            name: name.to_owned(),
        };
        self.request(call, |answer| match answer {
            Answer::Bytes(bytes) => Some((bytes, None)),
            _ => None,
        })
    }

    fn put_asset(
        &self,
        owner: &Owner,
        name: &str,
        bytes: &[u8],
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let call = Call::PutAsset {
            owner: owner.clone(),
            name: name.to_owned(),
            bytes: bytes.to_vec(),
        };
        self.done(call, None)
    }

    /// Refused when `to` is on another server.
    fn carry_card_files(
        &self,
        board: &str,
        card: &str,
        to: &Self,
        to_board: &str,
        to_card: &str,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        if !self.inner.client.same(&to.inner.client) {
            let refused = anyhow::anyhow!("a card's files do not move between servers");
            return futures::future::ready(Err(Error::Other(refused))).boxed();
        }
        let call = Call::CarryCardFiles {
            board: board.to_owned(),
            card: card.to_owned(),
            to: to.inner.root.clone(),
            to_board: to_board.to_owned(),
            to_card: to_card.to_owned(),
        };
        self.done(call, None)
    }

    /// A path on the server's machine, for an entry the replica holds.
    fn place(&self, kind: Kind, id: &str) -> Option<PathBuf> {
        let held = self.held();
        let there = match kind {
            Kind::Article => held.article(id).is_some(),
            Kind::Board => held.board(id).is_some(),
            Kind::Session => held.session(id).is_some(),
            Kind::Table | Kind::App => false,
        };
        there.then(|| self.layout().entry_path(kind, id)).flatten()
    }

    fn files(&self, dir: &Path) -> bool {
        self.layout().document_dir(dir)
    }

    /// Knocks once the replica has been filled again.
    fn watch(&self, knock: impl Fn() + Send + Sync + 'static) -> Option<Watching> {
        let mut knocks = self
            .inner
            .knocks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        knocks.next += 1;
        let id = knocks.next;
        knocks.held.push((id, Arc::new(knock)));
        let guard = Knocking {
            inner: Arc::downgrade(&self.inner),
            id,
        };
        Some(Watching::new(true, guard))
    }

    fn number(
        &self,
        kind: Kind,
        id: &str,
    ) -> impl Future<Output = super::Result<u64>> + Send + use<> {
        let call = Call::Number {
            kind,
            id: id.to_owned(),
        };
        let id = id.to_owned();
        self.request(call, move |answer| match answer {
            Answer::Number(number) => Some((number, Some(Change::Number(kind, id, number)))),
            _ => None,
        })
    }

    fn resolve(&self, kind: Kind, number: u64) -> super::Result<Option<String>> {
        self.held().resolve(kind, number)
    }

    fn retire(
        &self,
        kind: Kind,
        id: &str,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.done(
            Call::Retire {
                kind,
                id: id.to_owned(),
            },
            Some(Change::Retired(kind, id.to_owned())),
        )
    }
}
