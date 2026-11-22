use crate::{
    model::{project::ProjectIds, workspace::Showing},
    view::sidebar::{self, Dragged, Group, Heading, Place, Row},
};
use std::{collections::HashMap, path::Path};

#[test]
fn an_agent_can_land_last_when_the_last_session_also_appears_in_a_space() {
    let moving = Row::Group(Group::Agent("moving".into()));
    let other = Row::Group(Group::Agent("other".into()));
    let last_session = Row::Entry {
        project: ProjectIds::default().intern(Path::new("workspace")),
        showing: Showing::Session(1),
    };
    let boundary = Row::Heading(Heading::Spaces);
    let space = Row::Group(Group::Space("space".into()));
    let places = HashMap::from([
        (
            moving.clone(),
            Place {
                kind: "agents".into(),
                section: Heading::Agents,
                section_above: Some(Heading::Agents),
            },
        ),
        (
            other.clone(),
            Place {
                kind: "agents".into(),
                section: Heading::Agents,
                section_above: Some(Heading::Agents),
            },
        ),
        (
            last_session.clone(),
            Place {
                kind: "space-space".into(),
                section: Heading::Spaces,
                section_above: Some(Heading::Spaces),
            },
        ),
        (
            boundary.clone(),
            Place {
                kind: "heading".into(),
                section: Heading::Spaces,
                section_above: Some(Heading::Agents),
            },
        ),
        (
            space.clone(),
            Place {
                kind: "spaces".into(),
                section: Heading::Spaces,
                section_above: Some(Heading::Spaces),
            },
        ),
    ]);
    let moving = Dragged::Row(moving);
    assert!(sidebar::lands(
        &places,
        &moving,
        Some(&Dragged::Row(last_session)),
        Some(&Dragged::Row(boundary.clone())),
    ));
    assert!(!sidebar::lands(
        &places,
        &moving,
        Some(&Dragged::Row(boundary)),
        Some(&Dragged::Row(space)),
    ));
    assert!(sidebar::lands(
        &places,
        &moving,
        None,
        Some(&Dragged::Row(other)),
    ));
}
