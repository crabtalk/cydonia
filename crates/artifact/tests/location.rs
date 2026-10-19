//! A local location reads and writes as the bare path files held before it.

use cydonia_artifact::{kind::Kind, location::Location, space::Member};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Serialize, Deserialize)]
struct Held<K: Ord> {
    projects: Vec<K>,
    last: BTreeMap<K, String>,
}

#[test]
fn a_local_location_is_written_as_its_path() {
    let path = PathBuf::from("/work/one");
    let paths = Held {
        projects: vec![path.clone()],
        last: BTreeMap::from([(path.clone(), "board".to_owned())]),
    };
    let locations = Held {
        projects: vec![Location::Local(path.clone())],
        last: BTreeMap::from([(Location::Local(path), "board".to_owned())]),
    };
    let written = toml::to_string(&paths).unwrap();
    assert_eq!(toml::to_string(&locations).unwrap(), written);

    let read: Held<Location> = toml::from_str(&written).unwrap();
    assert_eq!(read.projects, locations.projects);
    assert_eq!(read.last, locations.last);
}

#[test]
fn a_local_location_keys_json_as_its_path() {
    let path = PathBuf::from("/work/one");
    let paths = BTreeMap::from([(path.clone(), 1)]);
    let locations = BTreeMap::from([(Location::Local(path), 1)]);
    let written = serde_json::to_string(&paths).unwrap();
    assert_eq!(serde_json::to_string(&locations).unwrap(), written);
    let read: BTreeMap<Location, i32> = serde_json::from_str(&written).unwrap();
    assert_eq!(read, locations);
}

#[test]
fn a_member_reads_its_project_as_a_location() {
    let member: Member =
        toml::from_str("project = \"/work/one\"\nkind = \"board\"\nid = \"b1\"\n").unwrap();
    assert_eq!(
        member,
        Member::new(PathBuf::from("/work/one"), Kind::Board, "b1")
    );
    assert_eq!(
        member.project(),
        Some(&Location::Local(PathBuf::from("/work/one")))
    );
}
