use crate::{
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{self, PROJECT},
};
use artifact::{
    canvas::{Canvas, Edit},
    kind::Kind,
    project::{Project as _, fs},
};
use serde_json::json;

const CANVAS: Arg = Arg {
    name: "canvas",
    about: "The canvas's stable project reference, such as #12 or foo#12.",
};
const TITLE: Arg = Arg {
    name: "title",
    about: "What the new canvas is called.",
};
const VERSION: Arg = Arg {
    name: "version",
    about: "The exact version from canvas_read. A changed canvas refuses the edit.",
};
const EDIT: Arg = Arg {
    name: "edit",
    about: "Atomic edits: full nodes and edges to upsert, and remove_nodes/remove_edges id lists. Existing items keep their stacking position; new items append. Removing a node removes its incident edges.",
};

pub static TOOLS: [Tool; 3] = [
    Tool {
        name: "canvas_add",
        description: "Create an empty canvas entry and return its stable reference and version. Read the canvas format resource before editing it.",
        schema: |bound| tools::fields(bound, &[PROJECT, TITLE]),
        writes: true,
        deletes: false,
        call: add,
    },
    Tool {
        name: "canvas_read",
        description: "Read a canvas's complete JSON Canvas document and version by its stable entry reference.",
        schema: |bound| tools::fields(bound, &[PROJECT, CANVAS]),
        writes: false,
        deletes: false,
        call: read,
    },
    Tool {
        name: "canvas_edit",
        description: "Atomically upsert or remove canvas nodes and edges. Read first and pass its version. Invalid graphs or stale versions leave storage unchanged.",
        schema: |bound| {
            let mut schema = tools::fields(bound, &[PROJECT, CANVAS, VERSION, EDIT]);
            schema["properties"][EDIT.name] = json!({
                "type": "object", "description": EDIT.about, "additionalProperties": false,
                "properties": {
                    "nodes": {"type": "array", "items": {"type": "object"}},
                    "edges": {"type": "array", "items": {"type": "object"}},
                    "remove_nodes": {"type": "array", "items": {"type": "string"}},
                    "remove_edges": {"type": "array", "items": {"type": "string"}}
                }
            });
            schema
        },
        writes: true,
        deletes: false,
        call: edit,
    },
];

fn add(args: Args<'_>) -> Outcome {
    let store = fs::Project::new(tools::root(&args)?);
    let canvas = store.create_canvas(args.text(TITLE)?, &Default::default())?;
    Ok(answer(&canvas))
}

fn find(args: &Args<'_>) -> Result<(fs::Project, Canvas), Trouble> {
    let named = args.text(CANVAS)?;
    let (project, number, within) = tools::entry_of(args, named)?;
    if within.is_some() {
        return Err(tools::whole(named));
    }
    let entry = tools::entry_numbered(&project, number, named)?;
    if entry.kind != Kind::Canvas {
        return Err(Trouble::Refused(format!(
            "{named} is a {}, not a canvas",
            entry.kind
        )));
    }
    let store = fs::Project::new(project);
    let canvas = store
        .canvas(&entry.id)
        .ok_or_else(|| Trouble::Refused(format!("{named} is no longer available")))?;
    Ok((store, canvas))
}

fn read(args: Args<'_>) -> Outcome {
    let (_, canvas) = find(&args)?;
    Ok(answer(&canvas))
}

fn edit(args: Args<'_>) -> Outcome {
    let edit: Edit = args.decode(EDIT)?;
    let (store, mut canvas) = find(&args)?;
    if canvas.version.as_deref() != Some(args.text(VERSION)?) {
        return Err(Trouble::Refused(
            "canvas changed; read it again before editing".into(),
        ));
    }
    edit.apply(&mut canvas).map_err(Trouble::Refused)?;
    store.save_canvas(&mut canvas)?;
    Ok(answer(&canvas))
}

fn answer(canvas: &Canvas) -> Answer {
    let data = json!({
        "number": canvas.number, "id": canvas.id, "title": canvas.label(),
        "version": canvas.version, "document": canvas.document,
    });
    Answer::said(serde_json::to_string_pretty(&data).expect("canvas data serializes")).with(data)
}
