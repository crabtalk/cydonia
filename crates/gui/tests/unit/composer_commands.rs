use crate::{model::session::Command, view::component::composer::commands::Commands};
use bezel::ui::menu::Item;

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
    Commands::new(
        &[
            command("support:draft", "(support) Draft a reply"),
            command("review", "Review the diff"),
            command("plugins:make", "(plugins) Make a plugin"),
            command("support:triage", "(support) Triage a ticket"),
            command("compact", ""),
        ],
        &[],
    )
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

#[test]
fn a_query_shows_sources_without_headings() {
    let mut commands = agent();
    commands.refilter("supp");
    assert_eq!(labels(&commands), ["support: draft", "support: triage"]);
    commands.refilter("tri");
    assert_eq!(labels(&commands), ["support: triage"]);
    assert_eq!(commands.item_of(0), Some(0));
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

#[test]
fn recent_commands_are_not_duplicated() {
    let sent = ["support:triage", "gone", "review"].map(str::to_owned);
    let mut commands = Commands::new(
        &[
            command("review", "Review the diff"),
            command("support:triage", "(support) Triage a ticket"),
        ],
        &sent,
    );
    assert_eq!(labels(&commands), ["/review", "# support", "triage"]);
    assert_eq!(commands.typed(0).unwrap(), "/review");
    commands.refilter("rev");
    assert_eq!(labels(&commands), ["/review"]);
}

#[test]
fn search_ranks_across_sources() {
    let mut commands = Commands::new(
        &[
            command("first:pre-review", ""),
            command("second:review", ""),
        ],
        &[],
    );
    commands.refilter("review");
    assert_eq!(labels(&commands), ["second: review", "first: pre-review"]);
    assert_eq!(commands.typed(0).unwrap(), "/second:review");
}

#[test]
fn codex_namespace_keeps_invocation_intact() {
    let mut commands = Commands::new(&[command("$openai-templates:design", "")], &[]);
    assert_eq!(labels(&commands), ["# openai-templates", "design"]);
    commands.refilter("design");
    assert_eq!(labels(&commands), ["openai-templates: design"]);
    assert_eq!(commands.typed(0).unwrap(), "/$openai-templates:design");
}
