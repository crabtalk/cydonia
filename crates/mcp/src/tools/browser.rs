//! The browser tabs in a project's right panel, worked in by an agent.
//!
//! Every call waits for the window: the page lives there and only answers on
//! the thread it is drawn on. See [`rail::browse`].

use crate::{
    rail::{self, Act, Level, Target},
    tool::{Arg, Args, Outcome, Tool, Trouble},
    tools,
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

const SCRIPT: Arg = Arg {
    name: "script",
    about: "JavaScript expression in the main frame. Promises are awaited. Return JSON-serializable data; use an IIFE for statements.",
};

const SELECTOR: Arg = Arg {
    name: "selector",
    about: "CSS selector for the first matching element, including hidden or disabled elements. Supply this or element, not both.",
};

const LEVELS: Arg = Arg {
    name: "levels",
    about: "Console levels to include. Omit or pass an empty list for all levels.",
};

const SINCE: Arg = Arg {
    name: "since",
    about: "Only messages after this id. Use the cursor from a previous console response. Defaults to zero.",
};

/// [`looking`] then [`acting`].
pub static TOOLS: [Tool; 10] = [
    Tool {
        name: "browser_tabs",
        description: "The browser tabs open in the project's right panel: id, address and title \
            of each, the front one marked.",
        schema: |bound| tools::fields(bound, &[tools::PROJECT]),
        writes: false,
        deletes: false,
        call: tabs,
    },
    Tool {
        name: "browser_open",
        description: "Load a URL in the project's in-app browser and read the page back. The \
            browser is the user's own, signed in where they are, and runs the page's scripts. \
            With `tab` it loads the URL in that tab. Without one it brings forward a tab \
            already at the URL, unreloaded, or else opens a new tab in the project's right \
            panel. A plain fetch is cheaper for an ordinary public page.",
        schema: |bound| with_tab(tools::fields(bound, &[tools::PROJECT, URL])),
        writes: false,
        deletes: false,
        call: open,
    },
    Tool {
        name: "browser_read",
        description: "Read a browser tab as it stands: its text, and the numbered elements that \
            can be clicked or typed into. The numbers describe this read only; read again after \
            anything changes the page.",
        schema: |bound| with_tab(tools::fields(bound, &[tools::PROJECT])),
        writes: false,
        deletes: false,
        call: read,
    },
    Tool {
        name: "browser_screenshot",
        description: "Capture a browser tab's current viewport as a PNG image. Scroll to see \
            another part of the page. The image contains the page only, without browser chrome.",
        schema: |bound| with_tab(tools::fields(bound, &[tools::PROJECT])),
        writes: false,
        deletes: false,
        call: screenshot,
    },
    Tool {
        name: "browser_scroll",
        description: "Scroll a browser tab and read it back. A read already has the whole \
            page's text; this is for pages that load more as they are scrolled.",
        schema: |bound| {
            let mut schema = with_tab(tools::fields(bound, &[tools::PROJECT]));
            schema["properties"][PAGES.name] =
                json!({ "type": "number", "description": PAGES.about });
            schema
        },
        writes: false,
        deletes: false,
        call: scroll,
    },
    Tool {
        name: "browser_console",
        description: "What a browser tab's page has logged: its console calls, uncaught errors \
            and unhandled rejections, oldest first, each with the frame that logged it. Failed \
            loads and other messages the engine writes itself are not in it.",
        schema: |bound| {
            let mut schema = with_tab(tools::fields(bound, &[tools::PROJECT]));
            schema["properties"][LEVELS.name] = json!({
                "type": "array", "items": {"type": "string", "enum": Level::ALL},
                "description": LEVELS.about
            });
            schema["properties"][SINCE.name] = json!({
                "type": "integer", "minimum": 0, "description": SINCE.about
            });
            schema
        },
        writes: false,
        deletes: false,
        call: console,
    },
    Tool {
        name: "browser_inspect",
        description: "Inspect an element's DOM attributes, bounds, computed styles, visibility, \
            ancestor constraints, scroll containers and overlap at its center. Accepts a number \
            from browser_read or a CSS selector, including non-interactive and hidden elements.",
        schema: |bound| {
            let mut schema = with_tab(tools::fields(bound, &[tools::PROJECT]));
            tools::optional(&mut schema, SELECTOR, "string");
            schema["properties"][ELEMENT.name] = json!({
                "type": "integer", "minimum": 0, "description": ELEMENT.about
            });
            schema["oneOf"] = json!([
                {"required": [ELEMENT.name], "not": {"required": [SELECTOR.name]}},
                {"required": [SELECTOR.name], "not": {"required": [ELEMENT.name]}}
            ]);
            schema
        },
        writes: false,
        deletes: false,
        call: inspect,
    },
    Tool {
        name: "browser_evaluate",
        description: "Evaluate JavaScript in a browser tab's main frame and return JSON. Awaits \
            promises and reports thrown errors. Can change the page or send requests using its \
            signed-in session. Results must fit in 16000 characters; return a focused selection.",
        schema: |bound| with_tab(tools::fields(bound, &[tools::PROJECT, SCRIPT])),
        writes: false,
        deletes: false,
        call: evaluate,
    },
    Tool {
        name: "browser_click",
        description: "Click an element by its number from the last read of the tab, and read \
            back the page it leaves.",
        schema: |bound| {
            let mut schema = with_tab(tools::fields(bound, &[tools::PROJECT]));
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
            let mut schema = with_tab(tools::fields(bound, &[tools::PROJECT, TEXT]));
            number(&mut schema, ELEMENT, "integer");
            tools::optional(&mut schema, ENTER, "boolean");
            schema
        },
        writes: false,
        deletes: false,
        call: type_text,
    },
];

/// The tools that read pages and move between them: tabs, open, read,
/// screenshot, scroll, console, inspect.
pub fn looking() -> &'static [Tool] {
    &TOOLS[..7]
}

/// The tools that execute page code or act on its controls.
pub fn acting() -> &'static [Tool] {
    &TOOLS[7..]
}

fn with_tab(mut schema: Value) -> Value {
    tools::optional(&mut schema, TAB, "integer");
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
    let project = tools::on_the_rail(tools::root(args)?)?.to_path_buf();
    rail::browse(project, tab, act)
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

fn screenshot(args: Args<'_>) -> Outcome {
    ask(&args, tab(&args)?, Act::Screenshot)
}

fn console(args: Args<'_>) -> Outcome {
    let levels = match args.value(LEVELS) {
        None => Vec::new(),
        Some(_) => args.decode::<Vec<Level>>(LEVELS)?,
    };
    let since = match args.value(SINCE) {
        None => 0,
        Some(value) => value.as_u64().ok_or_else(|| {
            Trouble::Invalid("since must be a non-negative message id".to_owned())
        })?,
    };
    ask(&args, tab(&args)?, Act::Console { levels, since })
}

fn evaluate(args: Args<'_>) -> Outcome {
    let script = args.text(SCRIPT)?;
    if script.trim().is_empty() {
        return Err(Trouble::Invalid("script is empty".to_owned()));
    }
    ask(&args, tab(&args)?, Act::Evaluate(script.to_owned()))
}

fn inspect(args: Args<'_>) -> Outcome {
    let target = match (args.value(ELEMENT), args.value(SELECTOR)) {
        (Some(_), None) => Target::Element(element(&args)?),
        (None, Some(_)) => {
            let selector = args.text(SELECTOR)?;
            if selector.trim().is_empty() {
                return Err(Trouble::Invalid("selector is empty".to_owned()));
            }
            Target::Selector(selector.to_owned())
        }
        _ => {
            return Err(Trouble::Invalid(
                "supply either element or selector".to_owned(),
            ));
        }
    };
    ask(&args, tab(&args)?, Act::Inspect(target))
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
