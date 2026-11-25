use crate::{
    tool::{Arg, Args, Trouble},
    tools::{self, PROJECT},
};
use serde_json::Value;

const COMMENT: Arg = Arg {
    name: "comment",
    about: "The note's id, as the follow-up names it.",
};
pub(super) const ANSWER: Arg = Arg {
    name: "answer",
    about: "The answer, in Cydonia Markdown. Added to the note's thread.",
};

pub(super) fn id(args: &Args<'_>) -> Result<u64, Trouble> {
    match args.value(COMMENT) {
        Some(Value::Number(id)) => id.as_u64(),
        Some(Value::String(id)) => id.trim().parse().ok(),
        _ => None,
    }
    .ok_or_else(|| Trouble::Invalid("comment is required, as the note's id".to_owned()))
}

pub(super) fn reply_schema(bound: bool, entry: Arg) -> Value {
    let mut schema = tools::fields(bound, &[PROJECT, entry, ANSWER]);
    tools::required(&mut schema, COMMENT, "integer");
    schema
}
