//! The browser tabs in a project's right panel, worked in by an agent.
//!
//! Every call waits for the window: the page lives there and only answers on
//! the thread it is drawn on. See [`rail::browse`].

use crate::{
    rail::{self, Act},
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{PROJECT, fields, root},
};
use serde_json::{Value, json};

const TAB: Arg = Arg {
    name: "tab",
    about: "Tab id, from browser_tabs. Omit for the project's front browser tab.",
};

const URL: Arg = Arg {
    name: "url",
    about: "The URL to load.",
};

const ELEMENT: Arg = Arg {
    name: "element",
    about: "The element's number, from the last read of this tab.",
};

const TEXT: Arg = Arg {
    name: "text",
    about: "The text to put in the field, replacing what it holds.",
};

const ENTER: Arg = Arg {
    name: "enter",
    about: "Press Return after typing — what runs a search box.",
};

const PAGES: Arg = Arg {
    name: "pages",
    about: "Screenfuls to scroll down; negative goes up. Defaults to one.",
};

pub static TOOLS: [Tool; 6] = [
    Tool {
        name: "browser_tabs",
        description: "The browser tabs open in the project's right panel: id, address and title \
            of each, the front one marked.",
        schema: |bound| fields(bound, &[PROJECT]),
        writes: false,
        deletes: false,
        call: tabs,
    },
    Tool {
        name: "browser_open",
        description: "Load a URL in the project's in-app browser and read the page back. The \
            browser is the user's own, signed in where they are, and runs the page's scripts. \
            Without `tab` it opens a new tab in the project's right panel. A plain fetch is \
            cheaper for an ordinary public page.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, URL]);
            schema["properties"][TAB.name] = json!({ "type": "integer", "description": TAB.about });
            schema
        },
        writes: false,
        deletes: false,
        call: open,
    },
    Tool {
        name: "browser_read",
        description: "Read a browser tab as it stands: its text, and the numbered elements that \
            can be clicked or typed into. The numbers describe this read only; read again after \
            anything changes the page.",
        schema: |bound| with_tab(fields(bound, &[PROJECT])),
        writes: false,
        deletes: false,
        call: read,
    },
    Tool {
        name: "browser_click",
        description: "Click an element by its number from the last read of the tab, and read \
            back the page it leaves.",
        schema: |bound| {
            let mut schema = with_tab(fields(bound, &[PROJECT]));
            number(&mut schema, ELEMENT, "integer");
            schema
        },
        writes: false,
        deletes: false,
        call: click,
    },
    Tool {
        name: "browser_type",
        description: "Type into a field by its number from the last read of the tab, replacing \
            what it holds, and read back the page.",
        schema: |bound| {
            let mut schema = with_tab(fields(bound, &[PROJECT, TEXT]));
            number(&mut schema, ELEMENT, "integer");
            schema["properties"][ENTER.name] =
                json!({ "type": "boolean", "description": ENTER.about });
            schema
        },
        writes: false,
        deletes: false,
        call: type_text,
    },
    Tool {
        name: "browser_scroll",
        description: "Scroll a browser tab and read it back. A read already has the whole \
            page's text; this is for pages that load more as they are scrolled.",
        schema: |bound| {
            let mut schema = with_tab(fields(bound, &[PROJECT]));
            schema["properties"][PAGES.name] =
                json!({ "type": "number", "description": PAGES.about });
            schema
        },
        writes: false,
        deletes: false,
        call: scroll,
    },
];

fn with_tab(mut schema: Value) -> Value {
    schema["properties"][TAB.name] = json!({ "type": "integer", "description": TAB.about });
    schema
}

/// A required numeric argument.
fn number(schema: &mut Value, arg: Arg, kind: &str) {
    schema["properties"][arg.name] = json!({ "type": kind, "description": arg.about });
    if let Some(required) = schema["required"].as_array_mut() {
        required.push(json!(arg.name));
    }
}

fn tab(args: &Args<'_>) -> Result<Option<u64>, Trouble> {
    match args.value(TAB) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
            .map(Some)
            .ok_or_else(|| Trouble::Invalid("tab must be a tab id".to_owned())),
    }
}

fn element(args: &Args<'_>) -> Result<usize, Trouble> {
    args.value(ELEMENT)
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .ok_or_else(|| Trouble::Invalid("element is required, as a number".to_owned()))
}

fn ask(args: &Args<'_>, tab: Option<u64>, act: Act) -> Outcome {
    let project = root(args)?.to_path_buf();
    if !rail::is_open(&project) {
        return Err(Trouble::Refused(format!(
            "cydonia does not have {} open, so it has no browser there",
            project.display()
        )));
    }
    rail::browse(project, tab, act).map(Answer::said)
}

fn tabs(args: Args<'_>) -> Outcome {
    ask(&args, None, Act::Tabs)
}

fn open(args: Args<'_>) -> Outcome {
    let url = args.text(URL)?.trim().to_owned();
    if url.is_empty() {
        return Err(Trouble::Invalid("url is empty".to_owned()));
    }
    ask(&args, tab(&args)?, Act::Open(url))
}

fn read(args: Args<'_>) -> Outcome {
    ask(&args, tab(&args)?, Act::Read)
}

fn click(args: Args<'_>) -> Outcome {
    ask(&args, tab(&args)?, Act::Click(element(&args)?))
}

fn type_text(args: Args<'_>) -> Outcome {
    let act = Act::Type {
        element: element(&args)?,
        text: args.text(TEXT)?.to_owned(),
        enter: args.boolean(ENTER, false)?,
    };
    ask(&args, tab(&args)?, act)
}

fn scroll(args: Args<'_>) -> Outcome {
    let pages = match args.value(PAGES) {
        None | Some(Value::Null) => 1.,
        Some(value) => value
            .as_f64()
            .ok_or_else(|| Trouble::Invalid("pages must be a number".to_owned()))?,
    };
    ask(&args, tab(&args)?, Act::Scroll(pages))
}
