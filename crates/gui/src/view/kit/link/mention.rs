//! What `@` in an article lists: the open projects' sessions, boards and
//! articles, linked as `cydonia://<project>#<number>` chips, and their boards'
//! cards, as `cydonia://<project>#<handle>` — and the title a chip of one
//! paints: cydonia's mark and the title.

use crate::{
    model::workspace::{
        Showing, Workspace,
        references::{self, Part},
    },
    view::{
        kit::{
            icon::kind_icon,
            text::{CardTitle, untitled},
        },
        sidebar::Row,
    },
};
use artifact::{
    board::{Board, Card},
    kind::Kind,
    reference::{self, Prefix, Reference, Target},
};
use bezel::gpui::{App, Global, SharedString};
use bezel::ui::icons::Icon;
use editor::Mention;
use markdown::Preview;

/// Rows the menu lists at most.
const SHOWN: usize = 20;

/// The mark every `cydonia://` chip paints. Inline chips paint only a glyph's
/// SVG, in the text colour — a file icon paints nothing there.
const MARK: Icon = Icon::glyph(include_bytes!("../../../../assets/mark.svg"));

#[derive(Clone)]
pub(crate) struct Linkable {
    /// A session's agent's mark, else its kind's.
    pub(crate) icon: Icon,
    pub(crate) title: String,
    pub(crate) about: String,
    pub(crate) url: String,
    pub(crate) touched: u128,
    /// The directory name of the project it is in.
    pub(crate) project: String,
    /// In the active project, the one a bare `#12` means.
    pub(crate) active: bool,
    pub(crate) number: u64,
    /// A board's card prefix.
    pub(crate) key: Option<String>,
    /// A session's agent.
    pub(crate) agent: Option<String>,
    pub(crate) archived: bool,
    pub(crate) kind: Kind,
    /// Its labels — see [`artifact::label`].
    pub(crate) labels: Vec<String>,
    /// The sidebar's row for it.
    pub(crate) row: Row,
}

/// The entries `@` can link, most recently touched first. Rebuilt whenever
/// the workspace changes.
#[derive(Default)]
pub(crate) struct Linkables(pub(crate) Vec<Linkable>);

impl Global for Linkables {}

/// Every open project's entries, as what `@` lists. Installed with
/// `cx.set_global`.
pub(crate) fn read(workspace: &Workspace, cx: &App) -> Linkables {
    let active = workspace.active_project().map(|project| &project.path);
    let mut held = Vec::new();
    for project in &workspace.projects {
        let Some(name) = reference::project_name(&project.path) else {
            continue;
        };
        let active = active == Some(&project.path);
        let reference = |number: u64| Reference::entry(Some(&name), number, None).to_string();
        let shown = |number: u64| match active {
            true => format!("#{number}"),
            false => reference(number),
        };
        let row = |showing: Showing| Row::Entry {
            project: project.path.clone(),
            showing,
        };
        let linkable =
            |icon: Icon, title: String, about: String, number: u64, touched: u128, row: Row| {
                Linkable {
                    archived: false,
                    kind: Kind::Session,
                    icon,
                    title,
                    about,
                    url: Reference::entry(Some(&name), number, None).link(),
                    touched,
                    project: name.to_string(),
                    active,
                    number,
                    key: None,
                    agent: None,
                    labels: Vec::new(),
                    row,
                }
            };
        held.extend(project.chats(cx).filter_map(|chat| {
            let number = chat.number?;
            Some(Linkable {
                agent: Some(chat.entry.name.clone()),
                archived: chat.closed,
                labels: chat.labels.clone(),
                ..linkable(
                    workspace
                        .agent_icon(&chat.entry.name)
                        .unwrap_or_else(|| kind_icon(Kind::Session)),
                    untitled(&chat.title, "Untitled session"),
                    format!("Session · {} · {}", chat.entry.name, shown(number)),
                    number,
                    chat.touched(),
                    row(Showing::Session(chat.id)),
                )
            })
        }));
        held.extend(project.articles.iter().filter_map(|article| {
            let number = article.number?;
            Some(Linkable {
                archived: article.archived,
                kind: Kind::Article,
                labels: article.labels.clone(),
                ..linkable(
                    kind_icon(Kind::Article),
                    untitled(&article.title, "Untitled article"),
                    format!("Article · {}", shown(number)),
                    number,
                    article.touched,
                    row(Showing::Article(article.id.clone())),
                )
            })
        }));
        held.extend(project.boards.iter().filter_map(|held| {
            let board = held.board.read(cx);
            let number = board.number?;
            let key = (!board.key.is_empty()).then(|| board.key.clone());
            let about = match &key {
                Some(key) => format!("Board · {key} · {}", shown(number)),
                None => format!("Board · {}", shown(number)),
            };
            Some(Linkable {
                key,
                archived: board.archived,
                kind: Kind::Board,
                labels: board.labels.clone(),
                ..linkable(
                    kind_icon(Kind::Board),
                    untitled(&board.name, "Untitled board"),
                    about,
                    number,
                    board.touched,
                    row(Showing::Board(board.id.clone())),
                )
            })
        }));
    }
    held.sort_by_key(|linkable| std::cmp::Reverse(linkable.touched));
    Linkables(held)
}

/// The mention source cydonia installs: the cards a handle names, then the
/// entries — see [`rank`] — then the cards whose text holds the query — see
/// [`rank_cards`].
pub(crate) fn source(query: &str, cx: &App) -> Vec<Mention> {
    let held = cx
        .try_global::<Linkables>()
        .map_or(&[][..], |Linkables(held)| held.as_slice());
    let workspace = references::current(cx);
    let cards = workspace
        .as_ref()
        .map(|workspace| carded(workspace.read(cx), cx))
        .unwrap_or_default();
    let query = Query::parse(query);
    let (named, holding) = query.rank_cards(&cards);
    let entries = query.rank(held).into_iter().map(|ix| {
        let linkable = &held[ix];
        Mention {
            icon: Some(linkable.icon.clone()),
            label: SharedString::from(linkable.title.clone()),
            description: Some(SharedString::from(linkable.about.clone())),
            url: linkable.url.clone(),
        }
    });
    named
        .into_iter()
        .map(|ix| card_mention(&cards[ix]))
        .chain(entries)
        .chain(holding.into_iter().map(|ix| card_mention(&cards[ix])))
        .take(SHOWN)
        .collect()
}

/// A card `@` can link: one with a handle, on a board with a key.
pub(crate) struct Carded<'a> {
    /// The directory name of the project it is in.
    pub(crate) project: String,
    /// In the active project, the one a bare `DEV-12` means.
    pub(crate) active: bool,
    pub(crate) board: &'a Board,
    pub(crate) card: &'a Card,
    /// The `12` in `DEV-12`.
    pub(crate) number: u64,
    /// `DEV-12`.
    pub(crate) handle: String,
}

/// Every open project's cards `@` can link, in board and column order.
/// Read per query rather than kept in [`Linkables`]: that is rebuilt on
/// every workspace change, and a project's cards run to hundreds.
impl Carded<'_> {
    /// `foo#DEV-12`.
    pub(crate) fn reference(&self) -> Reference<'_> {
        Reference {
            project: Some(&self.project),
            target: Target::Card {
                key: &self.board.key,
                handle: self.number,
            },
        }
    }
}

pub(crate) fn carded<'a>(workspace: &'a Workspace, cx: &'a App) -> Vec<Carded<'a>> {
    let active = workspace.active_project().map(|project| &project.path);
    let mut cards = Vec::new();
    for project in &workspace.projects {
        let Some(name) = reference::project_name(&project.path) else {
            continue;
        };
        for board in project.boards.iter().map(|held| held.board.read(cx)) {
            for card in board.columns.iter().flat_map(|column| &column.cards) {
                let (Some(number), Some(handle)) = (card.handle, board.handle_of(card)) else {
                    continue;
                };
                cards.push(Carded {
                    project: name.to_string(),
                    active: active == Some(&project.path),
                    board,
                    card,
                    number,
                    handle,
                });
            }
        }
    }
    cards
}

/// What `query` lists of `cards`, as positions in it: the cards a card
/// prefix names — see [`reference::partial`] — in the project named before
/// the `#` or the active one, the exact one first, then by number; and apart,
/// the others whose text holds the query, the active project first and
/// archived boards last. Nothing for an empty query or one with a kind or
/// label prefix — see [`prefixes`] — and no text matches for a query holding
/// `#`.
/// What `@` or the search palette is asked for: a kind's prefix, `s:`, `a:` or
/// `b:` in either case, then a label's, `l:` and the label, then the rest.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Query<'a> {
    pub(crate) kind: Option<Kind>,
    pub(crate) label: Option<String>,
    pub(crate) rest: &'a str,
}

impl<'a> Query<'a> {
    pub(crate) fn parse(query: &'a str) -> Self {
        let (kind, query) = Self::kind_prefix(query);
        let (label, rest) = Self::label_prefix(query);
        Self { kind, label, rest }
    }

    /// The kind a query opens with, `s:`, `a:` or `b:` in either case, and the
    /// query after it.
    fn kind_prefix(query: &'a str) -> (Option<Kind>, &'a str) {
        let query = query.trim_start();
        let kind = match query.get(..2).map(str::to_ascii_lowercase).as_deref() {
            Some("s:") => Kind::Session,
            Some("a:") => Kind::Article,
            Some("b:") => Kind::Board,
            _ => return (None, query),
        };
        (Some(kind), &query[2..])
    }

    /// The label a query opens with, `l:` in either case and the word after it,
    /// normalised, and the query after it.
    fn label_prefix(query: &'a str) -> (Option<String>, &'a str) {
        let query = query.trim_start();
        if query.get(..2).map(str::to_ascii_lowercase).as_deref() != Some("l:") {
            return (None, query);
        }
        let rest = &query[2..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        match artifact::label::normalize(&rest[..end]) {
            Some(label) => (Some(label), &rest[end..]),
            None => (None, query),
        }
    }

    /// What `query` lists, as positions in `held`, best first. An empty query
    /// lists the active project, archived entries last. A query opening with a
    /// kind's prefix — `s:`, `a:` or `b:` — lists that kind alone, and one
    /// naming a label — `l:` and the label — the entries carrying it alone. A
    /// label with nothing after it lists every entry carrying it.
    ///
    /// Each entry is placed by its best match: an exact reference — see
    /// [`Query::references`] — then a title starting with the query, then a reference
    /// prefix or a title holding it, then a session's agent holding it. Titles
    /// are matched in every project, and not for a query holding `#`. Archived
    /// entries follow every other match but an exact reference. Within a place
    /// the active project leads; ties keep the most recently touched first.
    pub(crate) fn rank(&self, held: &[Linkable]) -> Vec<usize> {
        let mut rows = match (&self.label, self.rest.trim().is_empty()) {
            (Some(_), true) => {
                let mut rows: Vec<usize> = (0..held.len()).collect();
                rows.sort_by_key(|&ix| held[ix].archived);
                rows
            }
            _ => self.rank_all(held),
        };
        if let Some(kind) = self.kind {
            rows.retain(|&ix| held[ix].kind == kind);
        }
        if let Some(label) = &self.label {
            rows.retain(|&ix| held[ix].labels.contains(label));
        }
        rows
    }

    /// The cards the query names: by handle, best first, then the cards whose
    /// text holds it. Nothing for a query with a kind or label prefix.
    pub(crate) fn rank_cards(&self, cards: &[Carded]) -> (Vec<usize>, Vec<usize>) {
        let query = self.rest.trim();
        if self.kind.is_some() || self.label.is_some() || query.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let mut named: Vec<(bool, u64, usize)> = Vec::new();
        if let Some(reference::Partial {
            project,
            target: Prefix::Card { key, handle },
        }) = reference::partial(query)
        {
            for (ix, carded) in cards.iter().enumerate() {
                let scoped = match project {
                    None => carded.active,
                    Some(name) => carded.project.eq_ignore_ascii_case(name),
                };
                let Some(number) = carded.card.handle else {
                    continue;
                };
                let digits = number.to_string();
                if scoped
                    && carded.board.key.eq_ignore_ascii_case(key)
                    && digits.starts_with(handle)
                {
                    named.push((digits != handle, number, ix));
                }
            }
        }
        named.sort_by_key(|&(inexact, number, _)| (inexact, number));
        let named: Vec<usize> = named.into_iter().map(|(.., ix)| ix).collect();
        if query.contains('#') {
            return (named, Vec::new());
        }
        let lower = query.to_lowercase();
        let mut holding: Vec<usize> = (0..cards.len())
            .filter(|ix| !named.contains(ix))
            .filter(|&ix| cards[ix].card.text.to_lowercase().contains(&lower))
            .collect();
        holding.sort_by_key(|&ix| (cards[ix].board.archived, !cards[ix].active));
        (named, holding)
    }

    fn rank_all(&self, held: &[Linkable]) -> Vec<usize> {
        let query = self.rest.trim();
        if query.is_empty() {
            let mut rows: Vec<usize> = (0..held.len()).filter(|&ix| held[ix].active).collect();
            rows.sort_by_key(|&ix| held[ix].archived);
            return rows;
        }
        let mut place: Vec<Option<u8>> = vec![None; held.len()];
        for ix in Self::references(query, held) {
            place[ix] = Some(match held[ix].named_by(query) {
                true => 0,
                false => 2,
            });
        }
        if !query.contains('#') {
            let lower = query.to_lowercase();
            for (ix, linkable) in held.iter().enumerate() {
                let title = linkable.title.to_lowercase();
                let found = if title.starts_with(&lower) {
                    Some(1)
                } else if title.contains(&lower) {
                    Some(2)
                } else if linkable
                    .agent
                    .as_ref()
                    .is_some_and(|agent| agent.to_lowercase().contains(&lower))
                {
                    Some(3)
                } else {
                    None
                };
                place[ix] = match (place[ix], found) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
            }
        }
        let mut rows: Vec<(bool, u8, bool, usize)> = place
            .into_iter()
            .enumerate()
            .filter_map(|(ix, place)| {
                let place = place?;
                let sunk = place > 0 && held[ix].archived;
                Some((sunk, place, !held[ix].active, ix))
            })
            .collect();
        rows.sort_by_key(|&(sunk, place, inactive, _)| (sunk, place, inactive));
        rows.into_iter().map(|(.., ix)| ix).collect()
    }

    /// The entries `query` starts a reference to, as positions in `held`, best
    /// first. Nothing for an empty query.
    ///
    /// Read as [`reference::partial`]: a number prefix lists the entries whose
    /// number starts with it, a key prefix the boards whose key does, each in the
    /// project named before the `#` or the active one. An exact number or key
    /// leads; ties keep the most recently touched first.
    fn references(query: &str, held: &[Linkable]) -> Vec<usize> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }
        let Some(partial) = reference::partial(query) else {
            return Vec::new();
        };
        let scoped: Vec<usize> = (0..held.len())
            .filter(|&ix| match partial.project {
                None => held[ix].active,
                Some(name) => held[ix].project.eq_ignore_ascii_case(name),
            })
            .collect();
        match partial.target {
            Prefix::Number(number) => Self::by_prefix(&scoped, number, held, |linkable| {
                Some(linkable.number.to_string())
            }),
            Prefix::Key(key) => Self::by_prefix(&scoped, key, held, |linkable| {
                linkable.key.as_ref().map(|key| key.to_lowercase())
            }),
            Prefix::Card { .. } => Vec::new(),
        }
    }

    /// The positions in `scoped` whose `field` starts with `prefix`, compared
    /// lowercase, the exact one first.
    fn by_prefix(
        scoped: &[usize],
        prefix: &str,
        held: &[Linkable],
        field: impl Fn(&Linkable) -> Option<String>,
    ) -> Vec<usize> {
        let prefix = prefix.to_lowercase();
        let mut rows: Vec<(bool, usize)> = scoped
            .iter()
            .filter_map(|&ix| {
                let value = field(&held[ix])?;
                value.starts_with(&prefix).then_some((value != prefix, ix))
            })
            .collect();
        rows.sort_by_key(|&(inexact, _)| inexact);
        rows.into_iter().map(|(_, ix)| ix).collect()
    }
}

impl Linkable {
    /// Whether `query` names this entry exactly: its number or its key.
    fn named_by(&self, query: &str) -> bool {
        let target = query.rsplit_once('#').map_or(query, |(_, rest)| rest);
        target == self.number.to_string()
            || self
                .key
                .as_ref()
                .is_some_and(|key| key.eq_ignore_ascii_case(target))
    }
}

fn card_mention(carded: &Carded) -> Mention {
    let shown = match carded.active {
        true => carded.handle.clone(),
        false => format!("{}#{}", carded.project, carded.handle),
    };
    Mention {
        icon: Some(kind_icon(Kind::Board)),
        label: SharedString::from(shown),
        description: Some(SharedString::from(format!(
            "{} · {}",
            carded.card.short_title(),
            untitled(&carded.board.name, "Untitled board")
        ))),
        url: carded.reference().link(),
    }
}

/// What a chip linking an entry paints: its title, and its kind's mark. A
/// run of a session's turns adds the run, and part of an article the lines or
/// the heading. A card paints its title — see [`card_chip_title`] — and handle. A link naming no entry
/// paints the reference and the app's mark.
pub(crate) fn preview(url: &str, cx: &App) -> Option<Preview> {
    let reference = url.strip_prefix(crate::view::kit::link::SCHEME)?;
    let Some(resolved) = crate::model::workspace::references::resolve_in(reference, cx) else {
        return Some(Preview {
            title: Some(SharedString::from(reference.to_owned())),
            glyph: Some(MARK),
            ..Preview::default()
        });
    };
    let fallback = match resolved.kind {
        Kind::Session => "Untitled session",
        Kind::Article => "Untitled article",
        Kind::Board => "Untitled board",
        Kind::Table => "Untitled table",
    };
    let title = untitled(&resolved.title, fallback);
    let title = match resolved.part {
        Some(Part::Turns(turns)) if turns.from == turns.to => format!("{title} · turn {turns}"),
        Some(Part::Turns(turns)) => format!("{title} · turns {turns}"),
        Some(Part::Passage(passage)) => format!("{title} · {}", passage.label),
        Some(Part::Card(card)) => format!(
            "{} · {}",
            markdown::parse(&card.text).chip_title(),
            card.handle
        ),
        None => title,
    };
    Some(Preview {
        title: Some(title.into()),
        glyph: Some(MARK),
        ..Preview::default()
    })
}

#[cfg(test)]
#[path = "../../../../tests/unit/mention_rank.rs"]
mod rank_tests;

#[cfg(test)]
#[path = "../../../../tests/unit/mention_cards.rs"]
mod card_tests;
