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
pub mod project;

use crate::{
    rail,
    tool::{Arg, Args, Trouble},
};
use serde_json::{Value, json};
use std::path::Path;

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
fn on_the_rail(path: &Path) -> Result<&Path, Trouble> {
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
