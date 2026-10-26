//! What a session's agent can be switched between — its mode and its config
//! options — in one shape, for the composer's agent menu and the agent modal.

use crate::model::session_preferences::{self, Offers};
use bezel::gpui::SharedString;
use cydonia_acp::proto::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionModeState,
};

/// Which request a [`Switch`] is, since the agent offers two shapes of the
/// same idea.
#[derive(Clone, PartialEq, Eq)]
pub enum SwitchId {
    /// `session/set_mode`. A session has one mode, so it carries no id.
    Mode,
    /// `session/set_config_option`, by the option's own id — which is where
    /// the model lives.
    Config(SharedString),
    /// The agent an article's session opens on, offered before there is a
    /// session to switch. Its options are the configured agents by index.
    Agent,
}

/// One value a switch can be set to.
#[derive(Clone, PartialEq)]
pub struct SwitchOption {
    pub id: SharedString,
    pub name: SharedString,
}

/// One switchable thing the session offers: the agent's mode, or a config
/// option. One shape for both — the composer shows a value and reports a pick,
/// and which request that is stays the session's business.
#[derive(Clone, PartialEq)]
pub struct Switch {
    pub id: SwitchId,
    /// What the thing is called, shown when it is on a value the agent no
    /// longer offers — which happens when an update lands mid-pick.
    pub name: SharedString,
    pub current: Option<SharedString>,
    pub options: Vec<SwitchOption>,
}

impl Switch {
    /// What `offers` can be switched between, flattened to the one shape the
    /// composer draws: the config options, then the modes. Every composer's agent
    /// menu is built here.
    ///
    /// Config first, because the model is a config option and it is the one people
    /// came for. Booleans are left out — a menu of two rows is a toggle wearing a
    /// menu, and the agent menu has nowhere to put a real one yet.
    pub(crate) fn of(offers: &Offers) -> Vec<Self> {
        let (modes, config) = (offers.modes.as_ref(), offers.config.as_slice());
        let mut switches: Vec<Switch> = config
            .iter()
            .filter_map(|option| {
                let SessionConfigKind::Select(select) = &option.kind else {
                    return None;
                };
                let options = select_options(&select.options);
                // A select with nothing to select is a menu with no rows.
                (!options.is_empty()).then(|| Switch {
                    id: SwitchId::Config(option.id.to_string().into()),
                    name: option.name.clone().into(),
                    current: Some(select.current_value.to_string().into()),
                    options,
                })
            })
            .collect();
        if let Some(modes) = modes
            .filter(|modes| !modes.available_modes.is_empty())
            .filter(|modes| !covered_by_config(config, modes))
        {
            switches.push(Switch {
                id: SwitchId::Mode,
                name: "Mode".into(),
                current: Some(modes.current_mode_id.to_string().into()),
                options: modes
                    .available_modes
                    .iter()
                    .map(|mode| SwitchOption {
                        id: mode.id.to_string().into(),
                        name: mode.name.clone().into(),
                    })
                    .collect(),
            });
        }
        switches
    }
}

/// Whether the agent is already offering these modes as a config option.
///
/// Some agents report their modes twice — once through `session/new`'s `modes`
/// and again as a config option — and two switches onto one piece of state is
/// two ways to disagree about it. The config option wins: it is the general
/// mechanism, and its update is what confirms a change.
///
/// Matched on the values as well as on the category, because the category is
/// optional and an agent that leaves it off still sends the same list twice.
fn covered_by_config(config: &[SessionConfigOption], modes: &SessionModeState) -> bool {
    config.iter().any(|option| {
        if option.category == Some(SessionConfigOptionCategory::Mode) {
            return true;
        }
        let SessionConfigKind::Select(select) = &option.kind else {
            return false;
        };
        let values = select_options(&select.options);
        modes
            .available_modes
            .iter()
            .all(|mode| values.iter().any(|value| value.id.as_ref() == &*mode.id))
    })
}

/// A select's values, with a group's rows folded in beside the ungrouped ones.
/// A switch gets one flat card, and a group is a heading it has nowhere to put.
fn select_options(options: &SessionConfigSelectOptions) -> Vec<SwitchOption> {
    session_preferences::select_values(options)
        .into_iter()
        .map(|option| SwitchOption {
            id: option.value.to_string().into(),
            name: option.name.clone().into(),
        })
        .collect()
}
