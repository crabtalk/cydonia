//! Part of an article a reference names, as the blocks its editor numbers,
//! the card a handle names, and the entry a number names.

use super::{Part, card, entry, passage};
use crate::data::{ColType, Column, Data};
use crate::model::{article::Disk, project::Project, workspace::Showing};
use artifact::project::{Project as _, fs};
use artifact::reference::{Span, Within};
use artifact::search::Kind;
use std::rc::Rc;

const TEXT: &str = "# Guide\n\nIntro.\n\n## Set up\n\nStep one.\n\n- a\n- b\n\n## Use\n\nGo.\n";

fn disk() -> Rc<Disk> {
    Rc::new(Disk {
        text: TEXT.to_owned(),
        blocks: markdown::parse_ranges(TEXT).block_ranges,
    })
}

#[test]
fn a_heading_covers_its_section_blocks() {
    let found = passage(disk(), Within::Heading("set-up"), "#1#set-up")
        .ok()
        .unwrap();
    assert_eq!(found.blocks, 2..6);
    assert_eq!(found.label, "Set up");
    assert_eq!(found.markdown(), "## Set up\n\nStep one.\n\n- a\n- b\n\n");
}

#[test]
fn lines_cover_every_block_they_touch() {
    let span = Span { from: 3, to: 7 };
    let found = passage(disk(), Within::Span(span), "#1:3-7").ok().unwrap();
    assert_eq!(found.blocks, 1..4);
    assert_eq!(found.label, "lines 3-7");
    let one = Span { from: 10, to: 10 };
    let found = passage(disk(), Within::Span(one), "#1:10").ok().unwrap();
    assert_eq!(found.blocks, 5..6);
    assert_eq!(found.label, "line 10");
}

#[test]
fn a_part_the_article_lacks_is_refused() {
    assert!(passage(disk(), Within::Heading("gone"), "#1#gone").is_err());
    let past = Span { from: 99, to: 99 };
    assert!(passage(disk(), Within::Span(past), "#1:99").is_err());
}

#[test]
fn a_handle_names_its_card_on_the_keyed_board() {
    let dir = std::env::temp_dir().join(format!("cydonia-references-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = fs::Project::new(&dir);
    let mut board = store.create_board("Work", "DEV").unwrap();
    let column = board.add_column("Todo").id.clone();
    board.add_card(&column, "First".into()).unwrap();
    let second = board.add_card(&column, "Second".into()).unwrap().clone();
    store.save_board(&mut board).unwrap();
    let handle = second.handle.unwrap();
    let project = Project::new(dir.clone());

    let found = card(&project, "dev", handle, "dev-2").ok().unwrap();
    let Some(Part::Card(named)) = found.part else {
        panic!("not a card");
    };
    assert_eq!(named.id, second.id);
    assert_eq!(named.handle, format!("DEV-{handle}"));
    assert!(card(&project, "DEV", 99, "DEV-99").is_err());
    assert!(card(&project, "ROAD", handle, "ROAD-2").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_number_names_a_table() {
    let dir = std::env::temp_dir().join(format!("cydonia-references-table-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let column = Column {
        name: "Name".to_owned(),
        kind: ColType::Text,
    };
    let key = Data::open(&dir)
        .unwrap()
        .create("Prices", None, &[column], None)
        .unwrap()
        .key;
    let project = Project::new(dir.clone());
    let number = project.tables[0].number.unwrap();

    let (kind, showing, title) = entry(&project, number).unwrap();
    assert_eq!(kind, Kind::Table);
    assert_eq!(showing, Showing::Table(key));
    assert_eq!(title, "Prices");
    assert!(entry(&project, number + 1).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
