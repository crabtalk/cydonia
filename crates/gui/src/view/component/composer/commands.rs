//! Slash command browsing by source and search across sources.

use crate::model::session::Command;
use bezel::{
    gpui::SharedString,
    ui::{menu::Item, popover},
};

/// Between a Claude Code plugin's name and its skill's: `plugin:skill`.
const PLUGIN_SEPARATOR: char = ':';

/// One command as a row of the list.
#[derive(Clone)]
struct Row {
    /// What picking it types: `/` and the whole name.
    typed: SharedString,
    /// What the row reads: the whole name, or the skill alone under its
    /// plugin's heading.
    label: SharedString,
    /// The name after its plugin, matched beside the whole one.
    short: String,
    description: String,
}

/// Commands from one namespace.
struct Group {
    /// Nothing for commands without a namespace.
    heading: Option<SharedString>,
    rows: Vec<Row>,
}

/// The commands, what the typed query keeps of them, and the lit row.
#[derive(Default)]
pub(crate) struct Commands {
    /// The commands as the agent sent them.
    pub advertised: Vec<Command>,
    /// The names of the commands last sent to the agent, most recent first.
    pub sent: Vec<String>,
    /// Namespaces in advertised order.
    groups: Vec<Group>,
    /// What the query keeps, in list order, as (group, row).
    shown: Vec<(usize, usize)>,
    /// A position in `shown`.
    active: Option<usize>,
    searching: bool,
}

impl Commands {
    pub fn new(commands: &[Command], sent: &[String]) -> Self {
        let mut groups = vec![Group {
            heading: None,
            rows: Vec::new(),
        }];
        for command in commands {
            let plugin = command
                .name
                .split_once(PLUGIN_SEPARATOR)
                .map(|(plugin, _)| plugin.trim_start_matches('$').to_owned());
            let short = match command.name.split_once(PLUGIN_SEPARATOR) {
                Some((_, skill)) => skill.to_owned(),
                None => command.name.clone(),
            };
            // Claude Code opens a plugin skill's description with `(plugin)`.
            let description = command.description.trim();
            let description = plugin
                .as_deref()
                .and_then(|plugin| description.strip_prefix(&format!("({plugin})")))
                .unwrap_or(description)
                .trim()
                .to_owned();
            let typed: SharedString = format!("/{}", command.name).into();
            let label = match plugin {
                Some(_) => short.clone().into(),
                None => typed.clone(),
            };
            let row = Row {
                typed,
                label,
                short,
                description,
            };
            let heading = plugin.map(SharedString::from);
            match groups.iter_mut().find(|group| group.heading == heading) {
                Some(group) => group.rows.push(row),
                None => groups.push(Group {
                    heading,
                    rows: vec![row],
                }),
            }
        }
        for group in &mut groups {
            group.rows.sort_by_key(|row| {
                sent.iter()
                    .position(|name| row.typed[1..] == *name)
                    .unwrap_or(usize::MAX)
            });
        }
        let mut commands = Self {
            advertised: commands.to_vec(),
            sent: sent.to_vec(),
            groups,
            shown: Vec::new(),
            active: None,
            searching: false,
        };
        commands.refilter("");
        commands
    }

    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(|group| group.rows.is_empty())
    }

    pub fn refilter(&mut self, query: &str) {
        self.searching = !query.trim().is_empty();
        let mut ranked: Vec<_> = self
            .groups
            .iter()
            .enumerate()
            .flat_map(|(at, group)| {
                group.rows.iter().enumerate().filter_map(move |(ix, row)| {
                    popover::match_rank(query, &row.typed[1..])
                        .into_iter()
                        .chain(popover::match_rank(query, &row.short))
                        .min()
                        .map(|rank| (rank, at, ix))
                })
            })
            .collect();
        if self.searching {
            ranked.sort_by_key(|&(rank, at, ix)| (rank, at, ix));
        }
        self.shown = ranked.into_iter().map(|(_, at, ix)| (at, ix)).collect();
        self.active = (!self.shown.is_empty()).then_some(0);
    }

    /// The lit row's position in what is shown.
    pub fn active(&self) -> Option<usize> {
        self.active
    }

    pub fn step(&mut self, delta: isize) {
        self.active = popover::menu_step(self.active, self.shown.len(), delta);
    }

    pub fn set_active(&mut self, position: usize) {
        if position < self.shown.len() {
            self.active = Some(position);
        }
    }

    /// What picking the row at `position` types.
    pub fn typed(&self, position: usize) -> Option<SharedString> {
        let (at, ix) = self.shown.get(position)?;
        Some(self.groups[*at].rows[*ix].typed.clone())
    }

    /// The list as menu items, a plugin's heading over its rows, beside the
    /// position in what is shown each item is — `None` for a heading.
    pub fn items(&self) -> (Vec<Item>, Vec<Option<usize>>) {
        let mut items = Vec::new();
        let mut positions = Vec::new();
        let mut group = None;
        for (position, &(at, ix)) in self.shown.iter().enumerate() {
            if !self.searching && group != Some(at) {
                group = Some(at);
                if let Some(heading) = &self.groups[at].heading {
                    items.push(Item::Heading(heading.clone()));
                    positions.push(None);
                }
            }
            let row = &self.groups[at].rows[ix];
            let label = match (&self.groups[at].heading, self.searching) {
                (Some(source), true) => format!("{source}: {}", row.label).into(),
                _ => row.label.clone(),
            };
            let item = Item::action(label);
            items.push(match row.description.as_str() {
                "" => item,
                description => item.with_inline_description(description.to_owned()),
            });
            positions.push(Some(position));
        }
        (items, positions)
    }

    /// The item the row at `position` is in [`Self::items`].
    pub fn item_of(&self, position: usize) -> Option<usize> {
        self.items()
            .1
            .iter()
            .position(|shown| *shown == Some(position))
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/composer_commands.rs"]
mod tests;
