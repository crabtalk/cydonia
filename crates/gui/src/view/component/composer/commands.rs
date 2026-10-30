//! The agent's slash commands as `/` lists them: while nothing is typed, the
//! ones last sent; then those of no plugin, then one group per plugin under its
//! name, each command matched against its whole `plugin:skill` name and
//! against the skill alone.

use crate::model::session::Command;
use bezel::{
    gpui::SharedString,
    ui::{menu::Item, popover},
};

/// Between a Claude Code plugin's name and its skill's: `plugin:skill`.
const PLUGIN_SEPARATOR: char = ':';

/// What the commands last sent are listed under.
const RECENT: &str = "Recent";

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

/// The rows of one plugin, of none, or the ones last sent.
struct Group {
    /// The plugin's name, or [`RECENT`]. Nothing for the commands of no
    /// plugin.
    heading: Option<SharedString>,
    /// The commands last sent, listed while nothing is typed.
    recent: bool,
    rows: Vec<Row>,
}

/// The commands, what the typed query keeps of them, and the lit row.
#[derive(Default)]
pub(crate) struct Commands {
    /// The commands as the agent sent them.
    pub advertised: Vec<Command>,
    /// The names of the commands last sent to the agent, most recent first.
    pub sent: Vec<String>,
    /// The ones last sent, then the group of no plugin, then each plugin's in
    /// the order the agent first names it.
    groups: Vec<Group>,
    /// What the query keeps, in list order, as (group, row).
    shown: Vec<(usize, usize)>,
    /// A position in `shown`.
    active: Option<usize>,
}

impl Commands {
    pub fn new(commands: &[Command], sent: &[String]) -> Self {
        let mut groups = vec![Group {
            heading: None,
            recent: false,
            rows: Vec::new(),
        }];
        for command in commands {
            let plugin = command
                .name
                .split_once(PLUGIN_SEPARATOR)
                .map(|(plugin, _)| plugin.to_owned());
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
                    recent: false,
                    rows: vec![row],
                }),
            }
        }
        // A command last sent that the agent no longer offers is left out.
        let recent: Vec<Row> = sent
            .iter()
            .filter_map(|name| {
                let typed = format!("/{name}");
                let row = groups
                    .iter()
                    .flat_map(|group| &group.rows)
                    .find(|row| row.typed == typed)?;
                Some(Row {
                    label: row.typed.clone(),
                    ..row.clone()
                })
            })
            .collect();
        if !recent.is_empty() {
            groups.insert(
                0,
                Group {
                    heading: Some(RECENT.into()),
                    recent: true,
                    rows: recent,
                },
            );
        }
        let mut commands = Self {
            advertised: commands.to_vec(),
            sent: sent.to_vec(),
            groups,
            shown: Vec::new(),
            active: None,
        };
        commands.refilter("");
        commands
    }

    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(|group| group.rows.is_empty())
    }

    /// Keep what `query` matches, within each group prefix matches before
    /// the rest, and light the first. The ones last sent only while nothing
    /// is typed.
    pub fn refilter(&mut self, query: &str) {
        let typed = !query.trim().is_empty();
        self.shown = self
            .groups
            .iter()
            .enumerate()
            .filter(|(_, group)| !(typed && group.recent))
            .flat_map(|(at, group)| {
                let mut rows: Vec<(usize, usize)> = group
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(ix, row)| {
                        let whole = popover::match_rank(query, &row.typed[1..]);
                        let short = popover::match_rank(query, &row.short);
                        whole.into_iter().chain(short).min().map(|rank| (rank, ix))
                    })
                    .collect();
                rows.sort();
                rows.into_iter().map(move |(_, ix)| (at, ix))
            })
            .collect();
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
            if group != Some(at) {
                group = Some(at);
                if let Some(heading) = &self.groups[at].heading {
                    items.push(Item::Heading(heading.clone()));
                    positions.push(None);
                }
            }
            let row = &self.groups[at].rows[ix];
            let item = Item::action(row.label.clone());
            items.push(match row.description.as_str() {
                "" => item,
                description => item.with_description(description.to_owned()),
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
