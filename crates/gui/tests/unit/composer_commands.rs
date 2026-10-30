use super::*;

fn command(name: &str, description: &str) -> Command {
    Command {
        name: name.to_owned(),
        description: description.to_owned(),
    }
}

fn labels(commands: &Commands) -> Vec<String> {
    commands
        .items()
        .0
        .iter()
        .map(|item| match item {
            Item::Heading(heading) => format!("# {heading}"),
            Item::Action { label, .. } => label.to_string(),
            _ => String::new(),
        })
        .collect()
}

fn agent() -> Commands {
    Commands::new(&[
        command("support:draft", "(support) Draft a reply"),
        command("review", "Review the diff"),
        command("plugins:make", "(plugins) Make a plugin"),
        command("support:triage", "(support) Triage a ticket"),
        command("compact", ""),
    ])
}

/// Commands of no plugin lead, then each plugin's under its heading, in the
/// order the agent first names it.
#[test]
fn commands_are_grouped_by_plugin() {
    assert_eq!(
        labels(&agent()),
        [
            "/review",
            "/compact",
            "# support",
            "draft",
            "triage",
            "# plugins",
            "make"
        ]
    );
}

/// A plugin's skill reads without its plugin's name, and picking it types the
/// whole of it.
#[test]
fn a_plugin_skill_drops_its_plugin_from_its_row() {
    let commands = agent();
    let (items, positions) = commands.items();
    let draft = items
        .iter()
        .position(|item| matches!(item, Item::Action { label, .. } if label == "draft"))
        .unwrap();
    assert!(matches!(
        &items[draft],
        Item::Action { description: Some(description), .. } if description == "Draft a reply"
    ));
    let row = positions[draft].unwrap();
    assert_eq!(commands.typed(row).unwrap(), "/support:draft");
}

/// Typing matches the whole name and the skill alone, and drops a heading
/// with nothing left under it.
#[test]
fn a_query_keeps_headings_over_what_it_matches() {
    let mut commands = agent();
    commands.refilter("supp");
    assert_eq!(labels(&commands), ["# support", "draft", "triage"]);
    commands.refilter("tri");
    assert_eq!(labels(&commands), ["# support", "triage"]);
    assert_eq!(commands.item_of(0), Some(1));
}

/// The arrows walk the rows and pass over the headings.
#[test]
fn the_arrows_pass_over_headings() {
    let mut commands = agent();
    commands.step(1);
    commands.step(1);
    assert_eq!(
        commands.typed(commands.active().unwrap()).unwrap(),
        "/support:draft"
    );
    assert_eq!(commands.item_of(commands.active().unwrap()), Some(3));
}
