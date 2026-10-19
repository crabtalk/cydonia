//! The `artifact` Cupola service: [`Project`] calls on a project another
//! machine holds. A server answers them with the [`fs::Project`] at the root
//! each request names, and pushes a [`Knock`] for a root whenever that
//! project's watch knocks. [`super::remote`] is the client's side.
//!
//! Entries cross in their records' JSON, beside the fields those records
//! leave out.

use super::{Owner, Project as _, fs};
use crate::{
    article::{Article, comments::Comments, properties},
    board::Board,
    error::Error,
    kind::Kind,
    session::record::Record,
    wire::{Refusal, json},
};
use cupola::{Emitter, Server, Service, lanes::Lanes};
use futures::{FutureExt as _, future::BoxFuture};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// A project's entries, called over a connection.
pub struct Artifact;

/// One call on the project at `root`.
#[derive(Clone, Serialize, Deserialize)]
pub struct Request {
    #[serde(with = "cupola::bytes::path")]
    pub root: PathBuf,
    pub call: Call,
}

/// The [`Project`](super::Project) calls, and what each is answered with.
#[derive(Clone, Serialize, Deserialize)]
pub enum Call {
    /// Answered with [`Answer::Everything`]. The server watches the root from
    /// then on.
    Everything,
    /// Answered with [`Answer::Board`].
    CreateBoard { name: String, key: String },
    /// Answered with [`Answer::Board`]: the board as saved, with the versions
    /// it now has.
    SaveBoard(BoardEntry),
    /// Answered with [`Answer::Done`].
    RemoveBoard(String),
    /// Answered with [`Answer::Id`].
    CreateSession,
    /// Answered with [`Answer::Session`]: the record as saved, with its
    /// number.
    SaveSession(SessionEntry),
    /// Answered with [`Answer::Done`].
    RemoveSession(String),
    /// Answered with [`Answer::Article`].
    CreateArticle(String),
    /// Answered with [`Answer::Article`].
    WriteArticle { id: String, markdown: String },
    /// Answered with [`Answer::Article`].
    SaveComments {
        id: String,
        #[serde(with = "json")]
        comments: Comments,
    },
    /// Answered with [`Answer::Article`]. The properties are a
    /// `properties.toml`'s text.
    SaveProperties { id: String, properties: String },
    /// Answered with [`Answer::Done`].
    RemoveArticle(String),
    /// Answered with [`Answer::Bytes`].
    Asset { owner: Owner, name: String },
    /// Answered with [`Answer::Done`].
    PutAsset {
        owner: Owner,
        name: String,
        bytes: Vec<u8>,
    },
    /// Copies a card's files to a project on the same server. Answered with
    /// [`Answer::Done`].
    CarryCardFiles {
        board: String,
        card: String,
        #[serde(with = "cupola::bytes::path")]
        to: PathBuf,
        to_board: String,
        to_card: String,
    },
    /// Answered with [`Answer::Number`].
    Number { kind: Kind, id: String },
    /// Answered with [`Answer::Done`].
    Retire { kind: Kind, id: String },
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Answer {
    Everything(Everything),
    Board(BoardEntry),
    Session(SessionEntry),
    Article(ArticleEntry),
    Id(String),
    Number(u64),
    Bytes(Vec<u8>),
    Done,
}

/// Pushed for a root whose project changed on the server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knock {
    #[serde(with = "cupola::bytes::path")]
    pub root: PathBuf,
}

/// A whole project: every board, session and article, and every number
/// issued and not retired.
#[derive(Clone, Serialize, Deserialize)]
pub struct Everything {
    pub boards: Vec<BoardEntry>,
    pub sessions: Vec<SessionEntry>,
    pub articles: Vec<ArticleEntry>,
    pub numbers: Vec<(u64, Kind, String)>,
}

/// A [`Board`], with the fields its record skips.
#[derive(Clone, Serialize, Deserialize)]
pub struct BoardEntry {
    #[serde(with = "json")]
    board: Board,
    number: Option<u64>,
    touched: u128,
    created: Option<u128>,
    version: Option<String>,
    description: String,
    description_version: Option<String>,
    /// Each card's version, in the order of the board's columns and their
    /// cards.
    cards: Vec<Option<String>>,
}

/// A session's [`Record`], with its number.
#[derive(Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    #[serde(with = "json")]
    record: Record,
    number: Option<u64>,
}

/// An article: what [`Article`] lists, its markdown, its properties as a
/// `properties.toml`'s text, and its comments.
#[derive(Clone, Serialize, Deserialize)]
pub struct ArticleEntry {
    #[serde(with = "json")]
    pub article: Article,
    pub number: Option<u64>,
    pub markdown: String,
    pub properties: String,
    #[serde(with = "json")]
    pub comments: Comments,
}

/// The server's watches, one per root asked for.
struct Watches {
    held: Mutex<HashMap<PathBuf, super::Watching>>,
    emitter: Emitter<Artifact>,
}

impl Service for Artifact {
    const NAME: &'static str = "artifact";
    type Request = Request;
    type Response = Result<Answer, Refusal>;
    type Event = Knock;
}

impl Artifact {
    /// Answers [`Artifact`] requests on `server`, on one [`Lanes`] lane per
    /// root, so one root's requests land in the order they were sent. A
    /// request holds the lane of every root it writes in.
    pub fn register(server: &Server) {
        let watches = Arc::new(Watches {
            held: Mutex::default(),
            emitter: server.emitter::<Self>(),
        });
        let lanes = Lanes::default();
        server.register::<Self, _, _>(move |request: Request| {
            let watches = watches.clone();
            lanes.run_on(request.roots(), move || {
                futures::executor::block_on(watches.answer(request))
            })
        });
    }
}

impl Request {
    /// The roots the request reads or writes in.
    fn roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.root.clone()];
        if let Call::CarryCardFiles { to, .. } = &self.call {
            roots.push(to.clone());
        }
        roots
    }
}

impl Watches {
    fn answer(&self, request: Request) -> BoxFuture<'static, Result<Answer, Refusal>> {
        let project = fs::Project::new(&request.root);
        let answer: BoxFuture<'static, super::Result<Answer>> = match request.call {
            Call::Everything => {
                self.watch(&project);
                futures::future::ready(Everything::of(&project).map(Answer::Everything)).boxed()
            }
            Call::CreateBoard { name, key } => project
                .create_board(&name, &key)
                .map(|made| made.map(|board| Answer::Board(BoardEntry::from(&board))))
                .boxed(),
            Call::SaveBoard(entry) => {
                let mut board = Board::from(entry);
                let saved = project.save_board(&mut board);
                let entry = BoardEntry::from(&board);
                saved
                    .map(|saved| saved.map(|()| Answer::Board(entry)))
                    .boxed()
            }
            Call::RemoveBoard(id) => done(project.remove_board(&id)),
            Call::CreateSession => project
                .create_session()
                .map(|id| id.map(Answer::Id))
                .boxed(),
            Call::SaveSession(entry) => {
                let mut record = Record::from(entry);
                let saved = project.save_session(&record);
                async move {
                    saved.await?;
                    record.number = Some(project.number(Kind::Session, &record.id).await?);
                    Ok(Answer::Session(SessionEntry::from(&record)))
                }
                .boxed()
            }
            Call::RemoveSession(id) => done(project.remove_session(&id)),
            Call::CreateArticle(markdown) => {
                let made = project.create_article(&markdown);
                async move {
                    let article = made.await?;
                    ArticleEntry::read(&project, &article.id).map(Answer::Article)
                }
                .boxed()
            }
            Call::WriteArticle { id, markdown } => {
                article(project.write_article(&id, &markdown), project, id)
            }
            Call::SaveComments { id, comments } => {
                article(project.save_comments(&id, &comments), project, id)
            }
            Call::SaveProperties { id, properties } => {
                let saved = project.save_properties(&id, &properties::parse(&properties));
                article(saved, project, id)
            }
            Call::RemoveArticle(id) => done(project.remove_article(&id)),
            Call::Asset { owner, name } => project
                .asset(&owner, &name)
                .map(|bytes| bytes.map(Answer::Bytes))
                .boxed(),
            Call::PutAsset { owner, name, bytes } => done(project.put_asset(&owner, &name, &bytes)),
            Call::CarryCardFiles {
                board,
                card,
                to,
                to_board,
                to_card,
            } => done(project.carry_card_files(
                &board,
                &card,
                &fs::Project::new(to),
                &to_board,
                &to_card,
            )),
            Call::Number { kind, id } => project
                .number(kind, &id)
                .map(|number| number.map(Answer::Number))
                .boxed(),
            Call::Retire { kind, id } => done(project.retire(kind, &id)),
        };
        answer.map(|answer| answer.map_err(Refusal::from)).boxed()
    }

    /// Watches `project` unless a watch on everything it reads back is held
    /// already.
    fn watch(&self, project: &fs::Project) {
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if held
            .get(project.root())
            .is_some_and(|watching| watching.settled)
        {
            return;
        }
        let emitter = self.emitter.clone();
        let knock = Knock {
            root: project.root().to_owned(),
        };
        if let Some(watching) = project.watch(move || {
            let _ = emitter.emit(&knock);
        }) {
            held.insert(project.root().to_owned(), watching);
        }
    }
}

/// A write answered with [`Answer::Done`].
fn done(
    write: impl Future<Output = super::Result<()>> + Send + 'static,
) -> BoxFuture<'static, super::Result<Answer>> {
    write.map(|written| written.map(|()| Answer::Done)).boxed()
}

/// A write answered with the article it wrote, read once it has landed.
fn article(
    write: impl Future<Output = super::Result<()>> + Send + 'static,
    project: fs::Project,
    id: String,
) -> BoxFuture<'static, super::Result<Answer>> {
    async move {
        write.await?;
        ArticleEntry::read(&project, &id).map(Answer::Article)
    }
    .boxed()
}

impl Everything {
    fn of(project: &fs::Project) -> super::Result<Self> {
        let articles = project
            .articles()
            .iter()
            .map(|article| ArticleEntry::read(project, &article.id))
            .collect::<super::Result<_>>()?;
        Ok(Self {
            boards: project.boards().iter().map(BoardEntry::from).collect(),
            sessions: project.sessions().iter().map(SessionEntry::from).collect(),
            articles,
            numbers: project.numbers()?,
        })
    }
}

impl ArticleEntry {
    fn read(project: &fs::Project, id: &str) -> super::Result<Self> {
        let article = project.article(id).ok_or_else(|| Error::Gone {
            named: id.to_owned(),
        })?;
        Ok(Self {
            number: article.number,
            markdown: project.read_article(id)?,
            properties: properties::apply("", &project.properties(id)).unwrap_or_default(),
            comments: project.comments(id),
            article,
        })
    }

    /// The article as [`Article`] lists it, with its number.
    pub fn article(&self) -> Article {
        Article {
            number: self.number,
            ..self.article.clone()
        }
    }
}

impl From<&Board> for BoardEntry {
    fn from(board: &Board) -> Self {
        Self {
            board: board.clone(),
            number: board.number,
            touched: board.touched,
            created: board.created,
            version: board.version.clone(),
            description: board.description.clone(),
            description_version: board.description_version.clone(),
            cards: board
                .columns
                .iter()
                .flat_map(|column| &column.cards)
                .map(|card| card.version.clone())
                .collect(),
        }
    }
}

impl From<BoardEntry> for Board {
    fn from(entry: BoardEntry) -> Self {
        let mut board = entry.board;
        board.number = entry.number;
        board.touched = entry.touched;
        board.created = entry.created;
        board.version = entry.version;
        board.description = entry.description;
        board.description_version = entry.description_version;
        for (card, version) in board
            .columns
            .iter_mut()
            .flat_map(|column| &mut column.cards)
            .zip(entry.cards)
        {
            card.version = version;
        }
        board
    }
}

impl From<&Record> for SessionEntry {
    fn from(record: &Record) -> Self {
        Self {
            record: record.clone(),
            number: record.number,
        }
    }
}

impl From<SessionEntry> for Record {
    fn from(entry: SessionEntry) -> Self {
        Self {
            number: entry.number,
            ..entry.record
        }
    }
}
