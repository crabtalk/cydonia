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
use artifact::reference::{Reference, Target, Within};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// What every tool takes first. One server answers for the whole app, so which
/// directory a call is about is the call's to say.
pub(crate) const PROJECT: Arg = Arg {
    name: "project",
    about: "The project: the path of the directory the work is in. A session \
already in a project may leave this out to mean that one, and must name a \
project cydonia has open to mean another.",
};

/// The directory a call is about: the one it named, or the one the caller was
/// opened in.
///
/// A named project wins over the binding, which is a default. A named one must
/// be on the rail; a binding is taken as given and is not checked against it.
///
/// Either way it has to be a directory. A path with a typo in it would
/// otherwise read as a project with nothing in it, which is a thing a model
/// would believe.
pub(crate) fn root<'a>(args: &Args<'a>) -> Result<&'a Path, Trouble> {
    let path = match (args.maybe(PROJECT), args.at()) {
        (Some(named), _) => on_the_rail(Path::new(named))?,
        (None, Some(at)) => at,
        (None, None) => Path::new(args.text(PROJECT)?),
    };
    match path.is_dir() {
        true => Ok(path),
        false => Err(Trouble::Refused(format!(
            "no directory at {}",
            path.display()
        ))),
    }
}

/// A named project, where cydonia has it open. Anywhere else is refused, and
/// no `.cydonia/` is made there.
pub(crate) fn on_the_rail(path: &Path) -> Result<&Path, Trouble> {
    if rail::is_open(path) {
        return Ok(path);
    }
    Err(Trouble::Refused(match held() {
        None => format!(
            "cydonia has no project open, so {} is not one to work in",
            path.display()
        ),
        Some(open) => format!(
            "cydonia does not have {} open — it has {open}",
            path.display()
        ),
    }))
}

/// The project a reference names: the one written before its `#`, else the
/// one the call is about — see [`artifact::reference`].
///
/// A written name is the last component of a project's directory, matched
/// against the projects cydonia has open. None open under it, or more than one,
/// is refused.
pub fn project_of(args: &Args<'_>, reference: &Reference<'_>) -> Result<PathBuf, Trouble> {
    let Some(name) = reference.project else {
        return root(args).map(Path::to_path_buf);
    };
    let open = rail::open();
    let mut named = open
        .iter()
        .filter(|path| path.file_name().is_some_and(|last| last == name));
    match (named.next(), named.next()) {
        (Some(one), None) => Ok(one.clone()),
        (Some(_), Some(_)) => Err(Trouble::Refused(format!(
            "more than one open project is named {name} — {}; name it by its whole path",
            open.iter()
                .filter(|path| path.file_name().is_some_and(|last| last == name))
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
        (None, _) => Err(Trouble::Refused(match held() {
            None => format!("no open project is named {name}, and cydonia has none open"),
            Some(open) => format!("no open project is named {name} — cydonia has {open}"),
        })),
    }
}

/// The entry number `needle` names in `project`, and the part of it named
/// after the number, or nothing when `needle` is not an entry reference. A
/// reference naming another project is refused.
pub(crate) fn part_in<'a>(
    project: &Path,
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
        && project.file_name().is_none_or(|last| last != name)
    {
        return Err(Trouble::Refused(format!(
            "{needle} is in project {name}, not {} — pass that project",
            project.display()
        )));
    }
    Ok(Some((number, within)))
}

/// [`part_in`] for a tool that takes a whole entry: a reference naming part
/// of one is refused.
pub(crate) fn number_in(project: &Path, needle: &str) -> Result<Option<u64>, Trouble> {
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
) -> Result<(PathBuf, u64, Option<Within<'a>>), Trouble> {
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
            .map(|path| path.display().to_string())
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
