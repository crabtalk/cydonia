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

const DESIGN: Arg = Arg {
    name: "design",
    about: "The design's stable project reference, such as #12.",
};
const TITLE: Arg = Arg {
    name: "title",
    about: "The name to show.",
};

pub static TOOLS: [Tool; 4] = [
    Tool {
        name: "design_add",
        description: "Create a Design entry for pages to review.",
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
        description: "Read a design's path, pages, review statuses and comment counts.",
        schema: |bound| tools::fields(bound, &[PROJECT, DESIGN]),
        writes: false,
        deletes: false,
        call: read,
    },
    Tool {
        name: "design_add_page",
        description: "Add a page and return its project-relative index.html path. Write the page and its assets in that directory.",
        schema: |bound| tools::fields(bound, &[PROJECT, DESIGN, TITLE]),
        writes: true,
        deletes: false,
        call: add_page,
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
        .iter()
        .map(|design| data(&store, root, design))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(answer(json!({ "designs": designs })))
}

fn read(args: Args<'_>) -> Outcome {
    let (root, store, design) = find(&args)?;
    Ok(answer(data(&store, &root, &design)?))
}

fn add_page(args: Args<'_>) -> Outcome {
    let (root, store, mut design) = find(&args)?;
    let page = store.add_design_page(&mut design, args.text(TITLE)?)?;
    let dir = relative_dir(&store, &root, &design)?;
    Ok(answer(json!({
        "design": design.number, "id": page.id, "title": page.title,
        "path": dir.join(page.path()), "status": page.status(),
    })))
}

fn find(args: &Args<'_>) -> Result<(std::path::PathBuf, fs::Project, Design), Trouble> {
    let named = args.text(DESIGN)?;
    let (root, number, within) = tools::entry_of(args, named)?;
    if within.is_some() {
        return Err(tools::whole(named));
    }
    let entry = tools::entry_numbered(&root, number, named)?;
    if entry.kind != Kind::Design {
        return Err(Trouble::Refused(format!("{named} is a {}", entry.kind)));
    }
    let store = fs::Project::new(&root);
    let design = store
        .design(&entry.id)
        .ok_or_else(|| Trouble::Refused(format!("{named} is no longer available")))?;
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
    let pages: Vec<_> = design
        .pages
        .iter()
        .map(|page| {
            json!({
                "id": page.id, "title": page.title, "path": dir.join(page.path()),
                "status": page.status(), "comment_count": page.comments.len(),
            })
        })
        .collect();
    Ok(json!({
        "number": design.number, "id": design.id, "title": design.label(),
        "path": dir, "archived": design.archived, "labels": design.labels,
        "pages": pages,
    }))
}

fn answer(value: Value) -> Answer {
    Answer::said(serde_json::to_string_pretty(&value).expect("design data serializes")).with(value)
}
