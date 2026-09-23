//! Talk to the agent sessions a project holds.

use crate::{
    rail::{self, Change},
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{PROJECT, fields, root},
};
use serde_json::json;
use std::path::Path;

const SESSION: Arg = Arg {
    name: "session",
    about: "The session's project entry reference, such as #43. Leave it out \
to start a new session on `agent` instead.",
};

const AGENT: Arg = Arg {
    name: "agent",
    about: "The agent to start a new session on, by its configured name or id. \
Used only when `session` is left out.",
};

const MESSAGE: Arg = Arg {
    name: "message",
    about: "What to send, as the session's next prompt.",
};

pub static TOOLS: [Tool; 1] = [Tool {
    name: "session_send",
    description: "Send a message to another agent session in the project, named by its entry \
        reference (#43), or start a new session on a named agent with the message as its first \
        prompt. Fire and forget: nothing is waited for or answered back.",
    schema: |bound| {
        let mut schema = fields(bound, &[PROJECT, SESSION, AGENT, MESSAGE]);
        schema["required"] = json!(match bound {
            true => vec![MESSAGE.name],
            false => vec![PROJECT.name, MESSAGE.name],
        });
        schema
    },
    writes: true,
    deletes: false,
    call: send,
}];

fn send(args: Args<'_>) -> Outcome {
    let project = root(&args)?;
    let message = args.text(MESSAGE)?;
    if message.trim().is_empty() {
        return Err(Trouble::Refused("message is empty".to_owned()));
    }
    // Sessions run in the app, and only for projects on the rail.
    if !rail::is_open(project) {
        return Err(Trouble::Refused(format!(
            "cydonia does not have {} open, so no session there can be sent to",
            project.display()
        )));
    }
    let Some(named) = args.maybe(SESSION) else {
        return start(project, args.maybe(AGENT), message);
    };
    let number = artifact::entry::reference(named)
        .ok_or_else(|| Trouble::Invalid("session must be a reference such as #43".to_owned()))?;
    let entry = artifact::entry::list(project)
        .map_err(|e| Trouble::Refused(e.to_string()))?
        .into_iter()
        .find(|entry| entry.number == number)
        .ok_or_else(|| Trouble::Refused(format!("no entry {named} in this project")))?;
    if entry.kind != "session" {
        return Err(Trouble::Refused(format!(
            "{named} is a {}, not a session",
            entry.kind
        )));
    }
    rail::ask(Change::Send {
        session: entry.id.clone(),
        message: message.to_owned(),
    })?;
    Ok(Answer::said(format!("sent to #{number} {}", entry.title)))
}

/// A new session on `agent`, seeded with `message`.
fn start(project: &Path, agent: Option<&str>, message: &str) -> Outcome {
    let agent = agent.ok_or_else(|| {
        Trouble::Invalid("session or agent is required, as a string".to_owned())
    })?;
    let project = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    rail::ask(Change::Start {
        project,
        agent: agent.to_owned(),
        message: message.to_owned(),
    })?;
    Ok(Answer::said(format!("sent to a new session on {agent}")))
}
