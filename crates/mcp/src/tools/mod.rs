//! The tool sets, one per surface a project holds, and the two things every
//! tool in them has in common: the project it is about, and the shape of a
//! schema over arguments that are all required strings.

pub mod article;
pub mod board;

use crate::tool::{Args, Trouble};
use serde_json::{Value, json};
use std::path::Path;

/// What every tool takes first. One server answers for the whole app, so which
/// directory a call is about is the call's to say.
pub(crate) const PROJECT: &str = "The project: the path of the directory the work is in.";

/// The directory a call is about, and it has to be one — a path with a typo in
/// it would otherwise read as a project with nothing in it, which is a thing a
/// model would believe.
pub(crate) fn root<'a>(args: &Args<'a>) -> Result<&'a Path, Trouble> {
    let path = Path::new(args.text("project")?);
    match path.is_dir() {
        true => Ok(path),
        false => Err(Trouble::Refused(format!(
            "no directory at {}",
            path.display()
        ))),
    }
}

/// An object schema over the named arguments. Every one this surface takes is
/// a required string — an address, or a line of text — so there is nothing
/// else for a schema here to say.
pub(crate) fn fields(args: &[(&str, &str)]) -> Value {
    let properties = args
        .iter()
        .map(|(name, about)| {
            (
                (*name).to_owned(),
                json!({ "type": "string", "description": about }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "type": "object",
        "properties": properties,
        "required": args.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
    })
}
