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

use crate::tool::{Arg, Args, Trouble};
use serde_json::{Value, json};
use std::path::Path;

/// What every tool takes first. One server answers for the whole app, so which
/// directory a call is about is the call's to say.
pub(crate) const PROJECT: Arg = Arg {
    name: "project",
    about: "The project: the path of the directory the work is in.",
};

/// The directory a call is about: the one the caller was opened in, or the one
/// it named.
///
/// The binding wins, and there is no argument to override it with — a session
/// is a project's, and a client that could reach past its own would be one
/// mistake away from writing to somebody else's work.
///
/// Either way it has to be a directory. A path with a typo in it would
/// otherwise read as a project with nothing in it, which is a thing a model
/// would believe.
pub(crate) fn root<'a>(args: &Args<'a>) -> Result<&'a Path, Trouble> {
    let path = match args.at() {
        Some(at) => at,
        None => Path::new(args.text(PROJECT)?),
    };
    match path.is_dir() {
        true => Ok(path),
        false => Err(Trouble::Refused(format!(
            "no directory at {}",
            path.display()
        ))),
    }
}

/// An object schema over required strings. Tools can add optional fields.
///
/// `bound` is whether the caller already has a project, in which case the
/// argument that names one is left out: an argument a model must supply and
/// the server will ignore is an argument that costs a turn to get wrong.
pub(crate) fn fields(bound: bool, args: &[Arg]) -> Value {
    let args: Vec<Arg> = match bound {
        true => args
            .iter()
            .filter(|arg| arg.name != PROJECT.name)
            .copied()
            .collect(),
        false => args.to_vec(),
    };
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
        "required": args.iter().map(|arg| arg.name).collect::<Vec<_>>(),
    })
}
