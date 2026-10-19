//! The tool sets, one per surface a project holds, and the two things every
//! tool in them has in common: the project it is about, and the shape of a
//! schema over required string arguments.
//!
//! A tool is named `<surface>_<verb>[_<noun>]` — `board_list`, `article_read`,
//! `board_move_card`. The prefix is the module, which is what makes the flat
//! list MCP insists on read as the set of sets it actually is: an agent holding
//! forty tools from four servers can see at a glance which are ours and which
//! of ours are about what. It is also the only thing a list can be grouped on
//! later, since a name is all the wire carries — `../desktop` groups its own
//! `brain_*` and `radar_*` that way.
//!
//! The noun is left off where the module already said it: `board_read` reads a
//! board, and only `board_add_card` has to say what it adds.

pub mod article;
pub mod board;
pub mod browser;
pub mod project;
pub mod session;
pub mod workspace;

use crate::{
    rail,
    tool::{Arg, Args, Trouble},
};
use artifact::archive::Archived;
use artifact::error::{Act, Error};
use artifact::location::Location;
use artifact::project::{fs, store::Store};
use artifact::reference::{Reference, Target, Within};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// What every tool takes first. One server answers for the whole app, so which
/// directory a call is about is the call's to say.
pub(crate) const PROJECT: Arg = Arg {
    name: "project",
    about: "The project: the path of the directory the work is in, or the \
cupola:// location cydonia lists for one on another machine. A session \
already in a project may leave this out to mean that one, and must name a \
project cydonia has open to mean another.",
};
/// What a search tool looks for.
pub(crate) const QUERY: Arg = Arg {
    name: "query",
    about: "The text to find, matched case-insensitively.",
};
/// Which entries a list or a search lets through — see
/// [`artifact::archive::Archived`].
pub(crate) const ARCHIVED_FILTER: Arg = Arg {
    name: "archived",
    about: "Archived ones: exclude (the default), include, or only.",
};

/// How many hits a search answers before it says there are more.
pub(crate) const HITS: usize = 20;

/// Offer [`ARCHIVED_FILTER`], its words as the schema's `enum`.
pub(crate) fn offer_archived(schema: &mut Value) {
    let words: Vec<&str> = Archived::ALL.iter().map(|filter| filter.key()).collect();
    schema["properties"][ARCHIVED_FILTER.name] = json!({
        "type": "string",
        "description": ARCHIVED_FILTER.about,
        "enum": words,
        "default": Archived::default().key(),
    });
}

/// The [`ARCHIVED_FILTER`] a call asked for, [`Archived::Exclude`] where it
/// asked for none.
pub(crate) fn archived(args: &Args<'_>) -> Result<Archived, Trouble> {
    let Some(word) = args.maybe(ARCHIVED_FILTER) else {
        return Ok(Archived::default());
    };
    Archived::parse(word).ok_or_else(|| {
        let words: Vec<&str> = Archived::ALL.iter().map(|filter| filter.key()).collect();
        Trouble::Refused(format!("{word} is not a filter — say {}", words.join(", ")))
    })
}

/// A project a call is about: where it is, and the store the app keeps it in.
#[derive(Clone)]
pub struct Project {
    pub location: Location,
    pub store: Store,
}

impl Project {
    /// The project at `location`, through the store the rail hands over for
    /// it. Refused where there is none.
    pub(crate) fn at(location: Location) -> Result<Self, Trouble> {
        match rail::store(&location) {
            Some(store) => Ok(Self { location, store }),
            None => Err(Error::ProjectClosed {
                location,
                open: rail::open(),
            }
            .into()),
        }
    }

    /// The project's directory, on whichever machine holds it.
    pub fn root(&self) -> &Path {
        self.location.root()
    }

    /// Whether `name`, as written before a reference's `#`, names this
    /// project.
    pub(crate) fn names(&self, name: &str) -> bool {
        artifact::reference::names(self.root(), name)
    }

    /// Whether the rail is holding this project.
    pub(crate) fn is_open(&self) -> bool {
        rail::is_open(&self.location)
    }

    /// `path`, a path on the project's machine, relative to its directory
    /// where it is under it.
    pub(crate) fn within(&self, path: &Path) -> PathBuf {
        path.strip_prefix(self.root()).unwrap_or(path).to_path_buf()
    }

    /// The project's files, where they are on this machine.
    pub(crate) fn files(&self) -> Option<&fs::Project> {
        match &self.store {
            Store::Fs(files) => Some(files),
            _ => None,
        }
    }

    /// The refusal for `what`, which only a project kept in files on this
    /// machine can do.
    pub(crate) fn not_here(&self, what: &str) -> Trouble {
        Trouble::Refused(format!(
            "{what} is not done for {}, which is not kept on this machine",
            self.location
        ))
    }
}

/// The project a call is about: the one it named, or the one the caller was
/// opened in.
///
/// A named project wins over the binding, which is a default. A named one must
/// be on the rail; a binding is taken as given and is not checked against it.
/// Either way the app must hold a store for it.
///
/// One on this machine has to be a directory. A path with a typo in it would
/// otherwise read as a project with nothing in it, which is a thing a model
/// would believe.
pub(crate) fn root(args: &Args<'_>) -> Result<Project, Trouble> {
    let location = match (args.maybe(PROJECT), args.at()) {
        (Some(named), _) => on_the_rail(Location::parse(named))?,
        (None, Some(at)) => at.clone(),
        (None, None) => Location::parse(args.text(PROJECT)?),
    };
    if let Some(path) = location.local()
        && !path.is_dir()
    {
        return Err(Trouble::Refused(format!(
            "no directory at {}",
            path.display()
        )));
    }
    Project::at(location)
}

/// A named project, where cydonia has it open. Anywhere else is refused, and
/// no `.cydonia/` is made there.
pub(crate) fn on_the_rail(location: Location) -> Result<Location, Trouble> {
    if rail::is_open(&location) {
        return Ok(location);
    }
    Err(Error::ProjectClosed {
        location,
        open: rail::open(),
    }
    .into())
}

/// The project a reference names: the one written before its `#`, else the
/// one the call is about — see [`artifact::reference`].
///
/// A written name is the last component of a project's directory, matched
/// against the projects cydonia has open. None open under it, or more than one,
/// is refused.
pub fn project_of(args: &Args<'_>, reference: &Reference<'_>) -> Result<Project, Trouble> {
    match reference.project {
        Some(name) => project_named(name),
        None => root(args),
    }
}

/// The open project `name`, as written before a reference's `#`, names.
pub(crate) fn project_named(name: &str) -> Result<Project, Trouble> {
    let open = rail::open();
    let mut named = open
        .iter()
        .filter(|location| artifact::reference::names(location.root(), name));
    match (named.next(), named.next()) {
        (Some(one), None) => Project::at(one.clone()),
        (Some(_), Some(_)) => Err(Trouble::Refused(format!(
            "more than one open project is named {name} — {}; name it by its whole path",
            open.iter()
                .filter(|open| open.root().file_name().is_some_and(|last| last == name))
                .map(Location::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        (None, _) => Err(Error::NoProjectNamed {
            name: name.to_owned(),
            open,
        }
        .into()),
    }
}

/// Entry `number` of `project`. `named` is the reference as written, for the
/// refusal.
pub(crate) fn entry_numbered(
    project: &Project,
    number: u64,
    named: &str,
) -> Result<artifact::entry::Entry, Trouble> {
    project
        .store
        .entries()
        .map_err(|e| Error::failed("this project's entries", Act::Read, e))?
        .into_iter()
        .find(|entry| entry.number == number)
        .ok_or_else(|| {
            Error::NotFound {
                what: "entry",
                named: named.to_owned(),
            }
            .into()
        })
}

/// The entry number `needle` names in `project`, and the part of it named
/// after the number, or nothing when `needle` is not an entry reference. A
/// reference naming another project is refused.
pub(crate) fn part_in<'a>(
    project: &Project,
    needle: &'a str,
) -> Result<Option<(u64, Option<Within<'a>>)>, Trouble> {
    let Some(Reference {
        project: named,
        target: Target::Entry { number, within },
    }) = artifact::reference::parse(needle)
    else {
        return Ok(None);
    };
    if let Some(name) = named
        && !project.names(name)
    {
        return Err(Trouble::Refused(format!(
            "{needle} is in project {name}, not {} — pass that project",
            project.location
        )));
    }
    Ok(Some((number, within)))
}

/// [`part_in`] for a tool that takes a whole entry: a reference naming part
/// of one is refused.
pub(crate) fn number_in(project: &Project, needle: &str) -> Result<Option<u64>, Trouble> {
    match part_in(project, needle)? {
        Some((_, Some(_))) => Err(whole(needle)),
        found => Ok(found.map(|(number, _)| number)),
    }
}

/// The refusal for a reference naming part of an entry where a whole one is
/// wanted.
pub(crate) fn whole(needle: &str) -> Trouble {
    Trouble::Invalid(format!(
        "{needle} names part of an entry, and this takes a whole one such as #12 — \
read a session's turns with session_read, an article's lines or section with article_read"
    ))
}

/// The project, entry number and part `named` refers to, reaching another
/// open project when it names one.
pub(crate) fn entry_of<'a>(
    args: &Args<'_>,
    named: &'a str,
) -> Result<(Project, u64, Option<Within<'a>>), Trouble> {
    let invalid = || Trouble::Invalid(format!("{named} is not an entry reference such as #12"));
    let reference = artifact::reference::parse(named).ok_or_else(invalid)?;
    let Target::Entry { number, within } = reference.target else {
        return Err(invalid());
    };
    Ok((project_of(args, &reference)?, number, within))
}

/// The rail, for a refusal to name — a model that named the wrong directory
/// can see the right one without a second call.
pub(crate) fn held() -> Option<String> {
    let open = rail::open();
    (!open.is_empty()).then(|| {
        open.iter()
            .map(Location::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// Say that one argument takes either a string or a list of them.
///
/// The `anyOf` rather than an array alone: naming one thing is what most calls
/// do, and a client that had to wrap every one of them in brackets would be
/// paying for the list on every call that is not one. [`Args::list`] reads
/// both shapes back.
///
/// The description is the argument's own. A tool that takes several says so in
/// the [`Arg`] it was built with — see `board::CARDS`, which is [`board::CARD`]
/// with a line about lists on it.
pub(crate) fn many(schema: &mut Value, arg: Arg) {
    schema["properties"][arg.name] = json!({
        "description": arg.about,
        "anyOf": [
            { "type": "string" },
            { "type": "array", "items": { "type": "string" } },
        ],
    });
}

/// Offer `arg` as an optional property of JSON type `kind`.
pub(crate) fn optional(schema: &mut Value, arg: Arg, kind: &str) {
    schema["properties"][arg.name] = json!({ "type": kind, "description": arg.about });
}

/// [`optional`], with the value a call that leaves it out gets.
pub(crate) fn defaulted(schema: &mut Value, arg: Arg, kind: &str, default: Value) {
    optional(schema, arg, kind);
    schema["properties"][arg.name]["default"] = default;
}

/// An object schema over required strings. Tools can add optional fields.
///
/// `bound` is whether the caller already has a project, in which case the
/// argument that names one is offered but not required — a model that says
/// nothing gets its own project, and can still name another.
pub(crate) fn fields(bound: bool, args: &[Arg]) -> Value {
    let required: Vec<&str> = args
        .iter()
        .map(|arg| arg.name)
        .filter(|name| !(bound && *name == PROJECT.name))
        .collect();
    let properties = args
        .iter()
        .map(|arg| {
            (
                arg.name.to_owned(),
                json!({ "type": "string", "description": arg.about }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

/// `names` joined for a refusal to list, or `none at all`.
pub(crate) fn listing(names: impl Iterator<Item = String>) -> String {
    let names: Vec<String> = names.collect();
    match names.is_empty() {
        true => "none at all".to_owned(),
        false => names.join(", "),
    }
}
