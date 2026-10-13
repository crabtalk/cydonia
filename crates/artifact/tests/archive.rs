//! Which entries a list or a search lets through.

use cydonia_artifact::archive::Archived;

#[test]
fn each_filter_keeps_what_it_says() {
    assert!(Archived::Exclude.keeps(false) && !Archived::Exclude.keeps(true));
    assert!(Archived::Include.keeps(false) && Archived::Include.keeps(true));
    assert!(!Archived::Only.keeps(false) && Archived::Only.keeps(true));
    assert_eq!(Archived::default(), Archived::Exclude);
}

#[test]
fn every_filter_round_trips_through_its_word() {
    for filter in Archived::ALL {
        assert_eq!(Archived::parse(filter.key()), Some(filter));
    }
    assert_eq!(Archived::parse(" ONLY "), Some(Archived::Only));
    assert_eq!(Archived::parse("sometimes"), None);
}
