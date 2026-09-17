//! A layout comes back arranged as it was written, names its members by
//! number rather than by holding them, and survives one of them going away.

mod common;

use common::Scratch;
use cydonia_artifact::{
    entry,
    layout::{self, Axis, Layout, Node, Side},
    project::Project as _,
};
use std::collections::HashSet;

/// A layout is made from the entry already open, so it starts as one pane.
#[test]
fn a_new_layout_is_one_pane() {
    let layout = Layout::new("1757000000000".into(), "layout-1", 12);
    assert_eq!(layout.leaves(), 1);
    assert_eq!(layout.entries(), vec![12]);
}

/// The tree nests, and the whole of it survives a write and a read.
#[test]
fn an_arrangement_comes_back_as_it_went() {
    let scratch = Scratch::new("layout-roundtrip");
    let store = scratch.store();

    let mut layout = store.create_layout("", 1).expect("a layout");
    layout.tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::Leaf {
                ratio: 0.25,
                entry: 1,
            },
            Node::Split {
                ratio: 0.75,
                axis: Axis::Vertical,
                children: vec![Node::leaf(2), Node::leaf(3)],
            },
        ],
    );
    store.save_layout(&mut layout);

    let read = store.layout(&layout.id).expect("written");
    assert_eq!(read.tree, layout.tree);
    assert_eq!(read.entries(), vec![1, 2, 3]);
    assert_eq!(read.leaves(), 3);
}

/// Members are numbers, not copies. Nothing of the entry itself is in the file.
#[test]
fn a_layout_holds_numbers_and_not_entries() {
    let scratch = Scratch::new("layout-numbers");
    let store = scratch.store();

    let mut layout = store.create_layout("", 7).expect("a layout");
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(7), Node::leaf(9)]);
    store.save_layout(&mut layout);

    let body = std::fs::read_to_string(
        scratch
            .path()
            .join(".cydonia/layouts")
            .join(format!("{}.toml", layout.id)),
    )
    .expect("written");
    assert!(body.contains("entry = 7"), "{body}");
    assert!(body.contains("entry = 9"), "{body}");
}

/// A layout gets a project number of its own, so it is addressable as `#N` the
/// way every other entry is.
#[test]
fn a_layout_is_numbered_like_any_entry() {
    let scratch = Scratch::new("layout-numbered");
    let store = scratch.store();

    let layout = store.create_layout("", 1).expect("a layout");
    assert!(layout.number.is_some());
    assert_eq!(
        store.layout(&layout.id).and_then(|read| read.number),
        layout.number,
    );
}

/// Deleting a layout tombstones its number: a fresh one must not inherit it.
#[test]
fn a_deleted_layout_does_not_hand_its_number_on() {
    let scratch = Scratch::new("layout-tombstone");
    let store = scratch.store();

    let first = store.create_layout("", 1).expect("a layout");
    let number = first.number.expect("numbered");
    store.remove_layout(&first.id);

    let second = store.create_layout("", 2).expect("a layout");
    assert_ne!(second.number, Some(number));
}

/// Names climb past the highest taken rather than filling the gap a delete
/// left: `layout-2` must not come back meaning something new.
#[test]
fn names_climb_past_a_deleted_one() {
    let taken: HashSet<String> = ["layout-1", "layout-2"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(layout::next_name(&taken), "layout-3");

    let gapped: HashSet<String> = ["layout-1", "layout-3"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(layout::next_name(&gapped), "layout-4");

    assert_eq!(layout::next_name(&HashSet::new()), "layout-1");
}

/// A name that is not one of ours is not counted for the next one.
#[test]
fn a_renamed_layout_does_not_hold_a_number() {
    let taken: HashSet<String> = ["Auth work", "layout-1"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(layout::next_name(&taken), "layout-2");
}

/// A member that has been deleted leaves the layout, and the split it was the
/// second half of goes with it.
#[test]
fn a_member_that_is_gone_is_pruned() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);

    assert!(layout.prune(&HashSet::from([2])));
    assert_eq!(layout.entries(), vec![1]);
    // One pane left, so there is no split and no divider to catch.
    assert!(matches!(layout.tree, Node::Leaf { entry: 1, .. }));
}

/// Pruning reaches nested splits, and an emptied one does not stay as a seam.
#[test]
fn pruning_reaches_a_nested_split() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    layout.tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(1),
            Node::split(Axis::Vertical, vec![Node::leaf(2), Node::leaf(3)]),
        ],
    );

    assert!(layout.prune(&HashSet::from([2, 3])));
    assert_eq!(layout.entries(), vec![1]);
    assert_eq!(layout.leaves(), 1);
}

/// Nothing gone means nothing changed — a read that reports a change nobody
/// made would have the backend writing the file back on every load.
#[test]
fn pruning_nothing_changes_nothing() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    let before = layout.tree.clone();

    assert!(!layout.prune(&HashSet::from([99])));
    assert_eq!(layout.tree, before);
}

/// Evening a split shares the room out, and reaches the splits inside it.
#[test]
fn evening_shares_the_room() {
    let mut tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(1),
            Node::leaf(2),
            Node::split(Axis::Vertical, vec![Node::leaf(3), Node::leaf(4)]),
        ],
    );
    tree.even();

    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    for child in children {
        assert!(
            (child.ratio() - 1. / 3.).abs() < f64::EPSILON,
            "{}",
            child.ratio()
        );
    }
    let Node::Split {
        children: inner, ..
    } = &children[2]
    else {
        panic!("a split");
    };
    assert!((inner[0].ratio() - 0.5).abs() < f64::EPSILON);
}

/// A layout is listed as an entry of the project, beside the boards and
/// sessions it arranges.
#[test]
fn a_layout_is_listed_among_the_entries() {
    let scratch = Scratch::new("layout-listed");
    let store = scratch.store();
    let layout = store.create_layout("", 1).expect("a layout");

    let entries = entry::list(scratch.path()).expect("listed");
    let found = entries
        .iter()
        .find(|found| found.kind == "layout")
        .expect("the layout");
    assert_eq!(found.id, layout.id);
    assert_eq!(found.title, "layout-1");
}

/// An unnamed layout still has something to show in the sidebar.
#[test]
fn a_layout_with_no_name_is_still_labelled() {
    let mut layout = Layout::new("1757000000000".into(), "", 1);
    assert_eq!(layout.label(), layout::UNNAMED);
    layout.name = "Auth work".into();
    assert_eq!(layout.label(), "Auth work");
}

/// Shares are written as they were set. A share held as `f32` and widened on
/// the way into TOML writes `0.3` as `0.30000001192092896`, which is a diff
/// nobody can read and a number that is not the one anyone chose.
#[test]
fn a_share_is_written_as_it_was_set() {
    let scratch = Scratch::new("layout-share");
    let store = scratch.store();

    let mut layout = store.create_layout("", 1).expect("a layout");
    layout.tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::Leaf {
                ratio: 0.3,
                entry: 1,
            },
            Node::Leaf {
                ratio: 0.7,
                entry: 2,
            },
        ],
    );
    store.save_layout(&mut layout);

    let body = std::fs::read_to_string(
        scratch
            .path()
            .join(".cydonia/layouts")
            .join(format!("{}.toml", layout.id)),
    )
    .expect("written");
    assert!(body.contains("ratio = 0.3\n"), "{body}");
    assert!(body.contains("ratio = 0.7\n"), "{body}");
}

/// A layout reads back through the catalog, so an agent asking for `#N` gets
/// the arrangement rather than an unknown-kind error.
#[test]
fn a_layout_reads_back_through_the_catalog() {
    let scratch = Scratch::new("layout-read");
    let store = scratch.store();
    let mut layout = store.create_layout("", 1).expect("a layout");
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    store.save_layout(&mut layout);

    let entries = entry::list(scratch.path()).expect("listed");
    let found = entries
        .iter()
        .find(|found| found.kind == "layout")
        .expect("the layout");
    let value = entry::read(scratch.path(), found).expect("read");
    assert_eq!(value["name"], "layout-1");
    assert_eq!(value["tree"]["kind"], "split");
}

/// Splits nest, and each names its own axis: a column beside a pane beside a
/// column that itself holds a row. The direction is per split, so an
/// arrangement alternates as deep as it is built.
///
/// A pinwheel — four panes cyclically overlapping, with no straight line
/// dividing the window — has no spelling here.
#[test]
fn splits_nest_with_an_axis_each() {
    let scratch = Scratch::new("layout-nested-axes");
    let store = scratch.store();
    let mut layout = store.create_layout("", 1).expect("a layout");

    layout.tree = Node::Split {
        ratio: 1.,
        axis: Axis::Horizontal,
        children: vec![
            Node::Split {
                ratio: 0.25,
                axis: Axis::Vertical,
                children: vec![Node::leaf(2), Node::leaf(3)],
            },
            Node::Leaf {
                ratio: 0.4,
                entry: 1,
            },
            Node::Split {
                ratio: 0.35,
                axis: Axis::Vertical,
                children: vec![
                    Node::Split {
                        ratio: 0.6,
                        axis: Axis::Horizontal,
                        children: vec![Node::leaf(4), Node::leaf(5)],
                    },
                    Node::Leaf {
                        ratio: 0.4,
                        entry: 6,
                    },
                ],
            },
        ],
    };
    store.save_layout(&mut layout);

    let read = store.layout(&layout.id).expect("written");
    assert_eq!(read.tree, layout.tree);
    // Laid out left to right, and each column top to bottom.
    assert_eq!(read.entries(), vec![2, 3, 1, 4, 5, 6]);
    assert_eq!(read.leaves(), 6);
}

/// Three panes side by side are one split of three, not two nested ones — the
/// `Vec` is what keeps a third pane from deepening the tree.
#[test]
fn a_third_pane_joins_the_split_rather_than_nesting() {
    let tree = Node::split(
        Axis::Horizontal,
        vec![Node::leaf(1), Node::leaf(2), Node::leaf(3)],
    );
    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert_eq!(children.len(), 3);
    assert!(children.iter().all(|child| child.leaves() == 1));
}

// ── dropping and dragging ────────────────────────────────────────

/// A pane dropped on the right of the only pane divides it, and the arrival
/// takes the side it was dropped on.
#[test]
fn dropping_on_an_edge_divides_the_pane() {
    let mut tree = Node::leaf(1);
    assert!(tree.insert(1, 2, Side::Right));
    assert_eq!(tree.entries(), vec![1, 2]);

    let mut tree = Node::leaf(1);
    assert!(tree.insert(1, 2, Side::Left));
    assert_eq!(tree.entries(), vec![2, 1], "dropped left, so it goes left");

    let mut tree = Node::leaf(1);
    assert!(tree.insert(1, 2, Side::Above));
    assert_eq!(tree.entries(), vec![2, 1]);
}

/// The side decides the axis: left and right divide the width, above and
/// below the height.
#[test]
fn the_side_decides_the_axis() {
    let mut tree = Node::leaf(1);
    tree.insert(1, 2, Side::Below);
    assert!(matches!(
        tree,
        Node::Split {
            axis: Axis::Vertical,
            ..
        }
    ));

    let mut tree = Node::leaf(1);
    tree.insert(1, 2, Side::Right);
    assert!(matches!(
        tree,
        Node::Split {
            axis: Axis::Horizontal,
            ..
        }
    ));
}

/// A third pane dropped along the split's own direction joins it rather than
/// nesting inside its neighbour — otherwise every drop deepens the tree and
/// the seams stop lining up.
#[test]
fn a_third_pane_joins_the_split_it_was_dropped_along() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    assert!(tree.insert(2, 3, Side::Right));

    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert_eq!(children.len(), 3, "joined, not nested");
    assert_eq!(tree.entries(), vec![1, 2, 3]);
}

/// Dropped across the split's direction, the pane divides in place and nests.
#[test]
fn a_pane_dropped_across_the_split_nests() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    assert!(tree.insert(2, 3, Side::Below));

    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert_eq!(children.len(), 2, "still two across");
    assert!(matches!(
        children[1],
        Node::Split {
            axis: Axis::Vertical,
            ..
        }
    ));
    assert_eq!(tree.entries(), vec![1, 2, 3]);
}

/// The room for an arrival comes out of the pane it was dropped on. A
/// neighbour dragged to a width it was wanted at must not be moved by
/// somebody else's drop.
#[test]
fn an_arrival_takes_its_room_from_the_pane_it_landed_on() {
    let mut tree = Node::Split {
        ratio: 1.,
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                ratio: 0.8,
                entry: 1,
            },
            Node::Leaf {
                ratio: 0.2,
                entry: 2,
            },
        ],
    };
    tree.insert(1, 3, Side::Right);

    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert_eq!(children.len(), 3);
    assert!((children[0].ratio() - 0.4).abs() < 1e-9, "halved");
    assert!((children[1].ratio() - 0.4).abs() < 1e-9, "the arrival");
    assert!(
        (children[2].ratio() - 0.2).abs() < 1e-9,
        "the neighbour is left alone"
    );
}

/// Shares always cover the room. One pane closing hands what it held to what
/// is left, rather than leaving a strip of the window bare.
#[test]
fn closing_a_pane_shares_out_what_it_held() {
    let mut tree = Node::split(
        Axis::Horizontal,
        vec![Node::leaf(1), Node::leaf(2), Node::leaf(3)],
    );
    tree.even();
    assert!(tree.remove(2));

    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    let total: f64 = children.iter().map(Node::ratio).sum();
    assert!((total - 1.).abs() < 1e-9, "{total}");
}

/// A pane dragged out of a pair takes the seam with it: what is left is one
/// pane, not one pane beside nothing.
#[test]
fn dragging_a_pane_out_of_a_pair_collapses_the_seam() {
    let mut tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(1),
            Node::split(Axis::Vertical, vec![Node::leaf(2), Node::leaf(3)]),
        ],
    );
    // 3 goes to the far left, leaving 2 alone in what was a column.
    assert!(tree.relocate(3, 1, Side::Left));

    assert_eq!(tree.entries(), vec![3, 1, 2]);
    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert!(
        children.iter().all(|child| child.leaves() == 1),
        "the emptied column is gone"
    );
}

/// A pane cannot be dropped on itself, and a drop that names a pane which is
/// not here changes nothing.
#[test]
fn a_drag_that_means_nothing_does_nothing() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    let before = tree.clone();

    assert!(!tree.relocate(1, 1, Side::Right), "onto itself");
    assert!(!tree.relocate(9, 1, Side::Right), "not a member");
    assert!(!tree.relocate(1, 9, Side::Right), "no such target");
    assert_eq!(tree, before);
}

/// An entry already on screen is moved rather than shown twice — one entry,
/// one pane.
#[test]
fn dropping_an_entry_that_is_already_shown_moves_it() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    layout.tree = Node::split(
        Axis::Horizontal,
        vec![Node::leaf(1), Node::leaf(2), Node::leaf(3)],
    );

    assert!(layout.insert(3, 1, Side::Right));
    assert_eq!(layout.entries(), vec![2, 3, 1]);
    assert_eq!(layout.leaves(), 3, "still three panes, not four");
}

/// A drop deep in the tree finds its pane.
#[test]
fn a_drop_reaches_a_pane_inside_a_nested_split() {
    let mut tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(1),
            Node::split(
                Axis::Vertical,
                vec![
                    Node::leaf(2),
                    Node::split(Axis::Horizontal, vec![Node::leaf(3), Node::leaf(4)]),
                ],
            ),
        ],
    );
    assert!(tree.insert(4, 5, Side::Right));
    assert_eq!(tree.entries(), vec![1, 2, 3, 4, 5]);
    assert_eq!(tree.leaves(), 5);
}

/// Whatever the drags were, the tree is still one a renderer can draw: every
/// split covers its room, no split is left holding one child, and no entry is
/// in two panes at once.
///
/// Driven by a fixed sequence rather than a random one — a test that fails
/// only on some seeds reports a bug nobody can reproduce.
#[test]
fn any_run_of_drags_leaves_a_tree_that_can_be_drawn() {
    let sides = [Side::Left, Side::Right, Side::Above, Side::Below];
    let mut tree = Node::leaf(1);
    let mut live: Vec<u64> = vec![1];
    let mut next = 2;

    // A walk that drops, moves and closes in a repeating pattern, rather than
    // a run of one kind: the states that break an invariant are the ones a
    // close leaves behind for the next drop.
    for step in 0..60u64 {
        let side = sides[(step % 4) as usize];
        let target = live[(step as usize * 7) % live.len()];
        match step % 5 {
            0..=2 => {
                assert!(tree.insert(target, next, side), "step {step}");
                live.push(next);
                next += 1;
            }
            3 if live.len() > 2 => {
                let moving = live[(step as usize * 3) % live.len()];
                tree.relocate(moving, target, side);
            }
            _ if live.len() > 1 => {
                let going = live[(step as usize * 5) % live.len()];
                assert!(tree.remove(going), "step {step}");
                live.retain(|entry| *entry != going);
            }
            _ => {}
        }
        check(&tree, step);
        let mut seen = live.clone();
        seen.sort_unstable();
        let mut shown = tree.entries();
        shown.sort_unstable();
        assert_eq!(shown, seen, "step {step}: panes and members disagree");
    }
    assert!(tree.leaves() > 1, "the walk should have left a split");
}

/// Every split covers its room and holds more than one child, all the way
/// down.
fn check(node: &Node, step: u64) {
    let Node::Split { children, .. } = node else {
        return;
    };
    assert!(
        children.len() > 1,
        "step {step}: a split holding {} child",
        children.len()
    );
    let total: f64 = children.iter().map(Node::ratio).sum();
    assert!(
        (total - 1.).abs() < 1e-6,
        "step {step}: shares sum to {total}"
    );
    for child in children {
        check(child, step);
    }
}

// ── zoom ─────────────────────────────────────────────────────────

/// One pane stands over the rest, and zooming it again puts it back. The tree
/// is untouched throughout: unzooming restores the arrangement rather than
/// rebuilding it.
#[test]
fn zooming_stands_one_pane_over_the_rest() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    let before = layout.tree.clone();

    assert_eq!(layout.zoom(2), Some(2));
    assert_eq!(layout.tree, before, "the arrangement is kept");

    assert_eq!(layout.zoom(2), None, "zooming it again puts it back");
    assert_eq!(layout.tree, before);
}

/// Zooming another pane swaps to it: one is over the rest, or none is.
#[test]
fn zooming_another_pane_swaps_to_it() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);

    layout.zoom(1);
    assert_eq!(layout.zoom(2), Some(2));
}

/// A pane that is not here cannot be zoomed, and asking leaves nothing zoomed
/// rather than pointing at a pane that is not drawn.
#[test]
fn a_pane_that_is_not_here_does_not_zoom() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    assert_eq!(layout.zoom(9), None);
    assert_eq!(layout.zoomed(), None);
}

/// Closing the zoomed pane leaves the rest unzoomed.
#[test]
fn closing_the_zoomed_pane_unzooms() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);

    layout.zoom(2);
    layout.remove(2);
    assert_eq!(layout.zoomed(), None);
    assert_eq!(layout.entries(), vec![1]);
}

/// A member pruned away takes the zoom with it, even though pruning does not
/// go through `remove`.
#[test]
fn a_pruned_member_is_not_left_zoomed() {
    let mut layout = Layout::new("1757000000000".into(), "layout-1", 1);
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    layout.zoom(2);

    layout.prune(&HashSet::from([2]));
    assert_eq!(layout.zoomed(), None, "the pane it named has gone");
}

/// Zoom survives a write and a read: a window closed zoomed comes back zoomed.
///
/// `zoomed` is written before `tree`, which is a table — TOML reads a bare key
/// after a table as belonging to it, so the other order does not round-trip.
#[test]
fn zoom_survives_a_write_and_a_read() {
    let scratch = Scratch::new("layout-zoom");
    let store = scratch.store();

    let mut layout = store.create_layout("", 1).expect("a layout");
    layout.tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    layout.zoom(2);
    store.save_layout(&mut layout);

    let read = store.layout(&layout.id).expect("written");
    assert_eq!(read.zoomed(), Some(2));
}

/// Nothing zoomed writes no key at all, rather than a `zoomed` nobody set.
#[test]
fn nothing_zoomed_writes_no_key() {
    let scratch = Scratch::new("layout-unzoomed");
    let store = scratch.store();
    let mut layout = store.create_layout("", 1).expect("a layout");
    store.save_layout(&mut layout);

    let body = std::fs::read_to_string(
        scratch
            .path()
            .join(".cydonia/layouts")
            .join(format!("{}.toml", layout.id)),
    )
    .expect("written");
    assert!(!body.contains("zoomed"), "{body}");
}

// ── seams ────────────────────────────────────────────────────────

/// Dragging a seam moves the two panes either side of it and leaves the rest
/// at the widths they were put at.
#[test]
fn a_seam_moves_only_the_two_it_divides() {
    let mut tree = Node::split(
        Axis::Horizontal,
        vec![Node::leaf(1), Node::leaf(2), Node::leaf(3)],
    );
    tree.even();

    // The first seam sits at a third; drag it to a fifth.
    assert!(tree.resize(0, 0.2, 0.05));
    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert!((children[0].ratio() - 0.2).abs() < 1e-9);
    assert!(
        (children[1].ratio() - (2. / 3. - 0.2)).abs() < 1e-9,
        "its neighbour took the difference"
    );
    assert!(
        (children[2].ratio() - 1. / 3.).abs() < 1e-9,
        "the third is untouched"
    );
    let total: f64 = children.iter().map(Node::ratio).sum();
    assert!((total - 1.).abs() < 1e-9);
}

/// A seam dragged past its neighbour stops: a pane with no width is one
/// nothing can grab to bring back.
#[test]
fn a_seam_stops_rather_than_closing_a_pane() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    tree.even();

    tree.resize(0, 5.0, 0.1);
    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert!((children[0].ratio() - 0.9).abs() < 1e-9, "clamped");
    assert!((children[1].ratio() - 0.1).abs() < 1e-9);

    tree.resize(0, -5.0, 0.1);
    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert!(
        (children[0].ratio() - 0.1).abs() < 1e-9,
        "clamped the other way"
    );
}

/// There is no seam after the last pane.
#[test]
fn there_is_no_seam_past_the_end() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    assert!(!tree.resize(1, 0.5, 0.05));
    assert!(!tree.resize(9, 0.5, 0.05));
    assert!(!Node::leaf(1).resize(0, 0.5, 0.05), "a pane has no seams");
}

/// A path names a split inside the tree, so a seam deep in the arrangement is
/// the one that moves.
#[test]
fn a_path_reaches_the_split_it_names() {
    let mut tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(1),
            Node::split(Axis::Vertical, vec![Node::leaf(2), Node::leaf(3)]),
        ],
    );
    tree.even();

    let inner = tree.at_path_mut(&[1]).expect("the column");
    assert!(inner.resize(0, 0.25, 0.05));

    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    let Node::Split {
        children: inner, ..
    } = &children[1]
    else {
        panic!("a column");
    };
    assert!((inner[0].ratio() - 0.25).abs() < 1e-9);
    assert!((inner[1].ratio() - 0.75).abs() < 1e-9);
    assert!(
        (children[0].ratio() - 0.5).abs() < 1e-9,
        "the outer split is untouched"
    );
}

/// An empty path is the tree itself, and a path that names nothing answers
/// nothing rather than the nearest thing to it.
#[test]
fn a_path_that_names_nothing_finds_nothing() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    assert!(tree.at_path_mut(&[]).is_some());
    assert!(tree.at_path_mut(&[0]).is_some(), "a leaf is a node");
    assert!(
        tree.at_path_mut(&[0, 0]).is_none(),
        "a leaf has no children"
    );
    assert!(tree.at_path_mut(&[9]).is_none());
}

// ── how much of the window a pane is ─────────────────────────────

/// A pane beside another is half the width; stacked, it is the full width and
/// half the height. A split the other way does not narrow a pane.
#[test]
fn a_pane_knows_how_much_of_the_window_it_is() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    tree.even();
    assert_eq!(tree.share_of(1, Axis::Horizontal), Some(0.5));
    assert_eq!(
        tree.share_of(1, Axis::Vertical),
        Some(1.),
        "side by side takes nothing off the height"
    );

    let mut tree = Node::split(Axis::Vertical, vec![Node::leaf(1), Node::leaf(2)]);
    tree.even();
    assert_eq!(tree.share_of(1, Axis::Horizontal), Some(1.));
    assert_eq!(tree.share_of(1, Axis::Vertical), Some(0.5));
}

/// The shares of every split above a pane multiply, and only those dividing
/// the way asked about count.
#[test]
fn shares_multiply_down_the_tree() {
    let tree = Node::Split {
        ratio: 1.,
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                ratio: 0.5,
                entry: 1,
            },
            Node::Split {
                ratio: 0.5,
                axis: Axis::Vertical,
                children: vec![
                    Node::Leaf {
                        ratio: 0.5,
                        entry: 2,
                    },
                    Node::Split {
                        ratio: 0.5,
                        axis: Axis::Horizontal,
                        children: vec![
                            Node::Leaf {
                                ratio: 0.5,
                                entry: 3,
                            },
                            Node::Leaf {
                                ratio: 0.5,
                                entry: 4,
                            },
                        ],
                    },
                ],
            },
        ],
    };

    // 2 is in the right half, stacked — so half the width, and the vertical
    // split takes a quarter of the height off it.
    assert_eq!(tree.share_of(2, Axis::Horizontal), Some(0.5));
    assert_eq!(tree.share_of(2, Axis::Vertical), Some(0.5));
    // 3 sits in the bottom half of that column, divided across — so its
    // width halves again while its height is the half it already had.
    assert_eq!(tree.share_of(3, Axis::Horizontal), Some(0.25));
    assert_eq!(tree.share_of(3, Axis::Vertical), Some(0.5));
}

/// A pane on its own is the whole of it, and an entry no pane is on has no
/// share at all.
#[test]
fn a_lone_pane_is_the_whole_window() {
    let tree = Node::leaf(1);
    assert_eq!(tree.share_of(1, Axis::Horizontal), Some(1.));
    assert_eq!(tree.share_of(9, Axis::Horizontal), None);
}
