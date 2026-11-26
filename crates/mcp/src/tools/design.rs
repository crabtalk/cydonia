use crate::{
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{self, PROJECT},
};
use artifact::{
    design::Design,
    kind::Kind,
    project::{Project as _, fs},
};
use serde_json::{Value, json};
use std::path::Path;

pub(super) const DESIGN: Arg = Arg {
    name: "design",
    about: "The design's stable project reference, such as #12.",
};
pub(super) const ARTBOARD: Arg = Arg {
    name: "artboard",
    about: "Artboard id from design_read.",
};
const TITLE: Arg = Arg {
    name: "title",
    about: "The name to show.",
};

pub static TOOLS: [Tool; 4] = [
    Tool {
        name: "design_add",
        description: "Create a Design entry for artboards to review.",
        schema: |bound| tools::fields(bound, &[PROJECT, TITLE]),
        writes: true,
        deletes: false,
        call: add,
    },
    Tool {
        name: "design_list",
        description: "List the project's designs and their stable references.",
        schema: |bound| tools::fields(bound, &[PROJECT]),
        writes: false,
        deletes: false,
        call: list,
    },
    Tool {
        name: "design_read",
        description: "Read a design's viewport, path and artboards.",
        schema: |bound| tools::fields(bound, &[PROJECT, DESIGN]),
        writes: false,
        deletes: false,
        call: read,
    },
    Tool {
        name: "design_add_artboard",
        description: "Add an artboard and return its project-relative index.html path. Write the artboard and its assets in that directory.",
        schema: |bound| tools::fields(bound, &[PROJECT, DESIGN, TITLE]),
        writes: true,
        deletes: false,
        call: add_artboard,
    },
];

fn add(args: Args<'_>) -> Outcome {
    let root = tools::root(&args)?;
    let store = fs::Project::new(root);
    let design = store.create_design(args.text(TITLE)?)?;
    Ok(answer(data(&store, root, &design)?))
}

fn list(args: Args<'_>) -> Outcome {
    let root = tools::root(&args)?;
    let store = fs::Project::new(root);
    let designs = store
        .designs()
        .into_iter()
        .map(|design| data(&store, root, &design))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(answer(json!({ "designs": designs })))
}

fn read(args: Args<'_>) -> Outcome {
    let (root, store, design) = find(&args)?;
    Ok(answer(data(&store, &root, &design)?))
}

fn add_artboard(args: Args<'_>) -> Outcome {
    let (root, store, mut design) = find(&args)?;
    let artboard = store.add_design_artboard(&mut design, args.text(TITLE)?)?;
    let dir = relative_dir(&store, &root, &design)?;
    Ok(answer(json!({
        "design": design.number, "id": artboard.id, "title": artboard.title,
        "path": dir.join(artboard.path()),
    })))
}

fn find(args: &Args<'_>) -> Result<(std::path::PathBuf, fs::Project, Design), Trouble> {
    let (root, entry) = tools::entry_kind(args, DESIGN, Kind::Design)?;
    let store = fs::Project::new(&root);
    let design = store.design(&entry.id).ok_or_else(|| {
        Trouble::Refused(format!("design #{} is no longer available", entry.number))
    })?;
    Ok((root, store, design))
}

fn relative_dir(
    store: &fs::Project,
    root: &Path,
    design: &Design,
) -> Result<std::path::PathBuf, Trouble> {
    Ok(store
        .design_dir(&design.id)
        .map_err(artifact::error::Error::from)?
        .strip_prefix(root)
        .map_err(|error| Trouble::Refused(error.to_string()))?
        .to_path_buf())
}

fn data(store: &fs::Project, root: &Path, design: &Design) -> Result<Value, Trouble> {
    let dir = relative_dir(store, root, design)?;
    let artboards: Vec<_> = design
        .artboards
        .iter()
        .map(|artboard| {
            json!({
                "id": artboard.id, "title": artboard.title, "path": dir.join(artboard.path()),
            })
        })
        .collect();
    Ok(json!({
        "number": design.number, "id": design.id, "title": design.label(),
        "path": dir, "archived": design.archived, "labels": design.labels,
        "viewport": design.viewport,
        "artboards": artboards,
    }))
}

fn answer(value: Value) -> Answer {
    Answer::said(serde_json::to_string_pretty(&value).expect("design data serializes")).with(value)
}
