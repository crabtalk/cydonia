//! How an arrangement of panes behaves: splitting, walking across the seams,
//! moving panes about, and losing one.
//!
//! The tree is generic over what a pane is on, so these work it with plain
//! numbers. What a member really is, and where a space is kept, is the app's
//! — see `cydonia`'s own space tests.

use cydonia_artifact::space::{Axis, Node, Side};
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
    assert!(tree.insert(&1, &2, Side::Right));
    assert_eq!(tree.entries(), vec![1, 2]);

    let mut tree = Node::leaf(1);
    assert!(tree.insert(&1, &2, Side::Left));
    assert_eq!(tree.entries(), vec![2, 1], "dropped left, so it goes left");

    let mut tree = Node::leaf(1);
    assert!(tree.insert(&1, &2, Side::Above));
    assert_eq!(tree.entries(), vec![2, 1]);
}

/// The side decides the axis: left and right divide the width, above and
/// below the height.
#[test]
fn the_side_decides_the_axis() {
    let mut tree = Node::leaf(1);
    tree.insert(&1, &2, Side::Below);
    assert!(matches!(
        tree,
        Node::Split {
            axis: Axis::Vertical,
            ..
        }
    ));

    let mut tree = Node::leaf(1);
    tree.insert(&1, &2, Side::Right);
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
    assert!(tree.insert(&2, &3, Side::Right));

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
    assert!(tree.insert(&2, &3, Side::Below));

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
                tabs: Vec::new(),
            },
            Node::Leaf {
                ratio: 0.2,
                entry: 2,
                tabs: Vec::new(),
            },
        ],
    };
    tree.insert(&1, &3, Side::Right);

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
    assert!(tree.remove(&2));

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
    assert!(tree.relocate(&3, &1, Side::Left));

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

    assert!(!tree.relocate(&1, &1, Side::Right), "onto itself");
    assert!(!tree.relocate(&9, &1, Side::Right), "not a member");
    assert!(!tree.relocate(&1, &9, Side::Right), "no such target");
    assert_eq!(tree, before);
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
    assert!(tree.insert(&4, &5, Side::Right));
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
                assert!(tree.insert(&target, &next, side), "step {step}");
                live.push(next);
                next += 1;
            }
            3 if live.len() > 2 => {
                let moving = live[(step as usize * 3) % live.len()];
                tree.relocate(&moving, &target, side);
            }
            _ if live.len() > 1 => {
                let going = live[(step as usize * 5) % live.len()];
                assert!(tree.remove(&going), "step {step}");
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
fn check(node: &Node<u64>, step: u64) {
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
    assert_eq!(tree.share_of(&1, Axis::Horizontal), Some(0.5));
    assert_eq!(
        tree.share_of(&1, Axis::Vertical),
        Some(1.),
        "side by side takes nothing off the height"
    );

    let mut tree = Node::split(Axis::Vertical, vec![Node::leaf(1), Node::leaf(2)]);
    tree.even();
    assert_eq!(tree.share_of(&1, Axis::Horizontal), Some(1.));
    assert_eq!(tree.share_of(&1, Axis::Vertical), Some(0.5));
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
                tabs: Vec::new(),
            },
            Node::Split {
                ratio: 0.5,
                axis: Axis::Vertical,
                children: vec![
                    Node::Leaf {
                        ratio: 0.5,
                        entry: 2,
                        tabs: Vec::new(),
                    },
                    Node::Split {
                        ratio: 0.5,
                        axis: Axis::Horizontal,
                        children: vec![
                            Node::Leaf {
                                ratio: 0.5,
                                entry: 3,
                                tabs: Vec::new(),
                            },
                            Node::Leaf {
                                ratio: 0.5,
                                entry: 4,
                                tabs: Vec::new(),
                            },
                        ],
                    },
                ],
            },
        ],
    };

    // 2 is in the right half, stacked — so half the width, and the vertical
    // split takes a quarter of the height off it.
    assert_eq!(tree.share_of(&2, Axis::Horizontal), Some(0.5));
    assert_eq!(tree.share_of(&2, Axis::Vertical), Some(0.5));
    // 3 sits in the bottom half of that column, divided across — so its
    // width halves again while its height is the half it already had.
    assert_eq!(tree.share_of(&3, Axis::Horizontal), Some(0.25));
    assert_eq!(tree.share_of(&3, Axis::Vertical), Some(0.5));
}

/// A pane on its own is the whole of it, and an entry no pane is on has no
/// share at all.
#[test]
fn a_lone_pane_is_the_whole_window() {
    let tree = Node::leaf(1);
    assert_eq!(tree.share_of(&1, Axis::Horizontal), Some(1.));
    assert_eq!(tree.share_of(&9, Axis::Horizontal), None);
}

// ── across the seams ─────────────────────────────────────────────

/// Two panes side by side are each other's neighbour, and neither has one
/// past the window's edge.
#[test]
fn a_pane_knows_what_is_across_the_seam() {
    let tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    assert_eq!(tree.neighbour(&1, Side::Right), Some(2));
    assert_eq!(tree.neighbour(&2, Side::Left), Some(1));
    assert_eq!(tree.neighbour(&1, Side::Left), None, "the window's edge");
    assert_eq!(tree.neighbour(&2, Side::Right), None);
    // Nothing divides them the other way.
    assert_eq!(tree.neighbour(&1, Side::Below), None);
    assert_eq!(tree.neighbour(&1, Side::Above), None);
}

/// A pane crossing into a divided neighbour lands on the one against the seam
/// it crossed, not on whatever happens to be first.
#[test]
fn crossing_into_a_split_lands_against_the_seam() {
    // 1 | (2 over 3)
    let tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(1),
            Node::split(Axis::Vertical, vec![Node::leaf(2), Node::leaf(3)]),
        ],
    );
    // Going right from 1 enters the column at its top.
    assert_eq!(tree.neighbour(&1, Side::Right), Some(2));
    // Coming back left from either of them is 1.
    assert_eq!(tree.neighbour(&2, Side::Left), Some(1));
    assert_eq!(tree.neighbour(&3, Side::Left), Some(1));
    // And within the column.
    assert_eq!(tree.neighbour(&2, Side::Below), Some(3));
    assert_eq!(tree.neighbour(&3, Side::Above), Some(2));
}

/// The walk goes up past splits dividing the other way: a pane at the bottom
/// of one column still has the column beside it.
#[test]
fn the_walk_climbs_past_splits_the_other_way() {
    // (1 over 2) | (3 over 4)
    let tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::split(Axis::Vertical, vec![Node::leaf(1), Node::leaf(2)]),
            Node::split(Axis::Vertical, vec![Node::leaf(3), Node::leaf(4)]),
        ],
    );
    assert_eq!(tree.neighbour(&2, Side::Right), Some(3), "up, across, down");
    assert_eq!(tree.neighbour(&4, Side::Left), Some(1));
    assert_eq!(tree.neighbour(&2, Side::Left), None);
}

/// Three across: the middle has a neighbour both ways, the ends only one.
#[test]
fn the_ends_of_a_row_have_one_neighbour() {
    let tree = Node::split(
        Axis::Horizontal,
        vec![Node::leaf(1), Node::leaf(2), Node::leaf(3)],
    );
    assert_eq!(tree.neighbour(&2, Side::Left), Some(1));
    assert_eq!(tree.neighbour(&2, Side::Right), Some(3));
    assert_eq!(tree.neighbour(&1, Side::Left), None);
    assert_eq!(tree.neighbour(&3, Side::Right), None);
}

/// A pane on its own has no neighbours, and an entry no pane is on has no
/// place to walk from.
#[test]
fn a_lone_pane_has_no_neighbours() {
    let tree = Node::leaf(1);
    assert_eq!(tree.neighbour(&1, Side::Right), None);
    assert_eq!(tree.neighbour(&9, Side::Right), None);
}

/// Swapping exchanges what two panes are on and leaves the arrangement alone
/// — so doing it twice puts everything back.
#[test]
fn swapping_keeps_the_shape_and_the_sizes() {
    let mut tree = Node::Split {
        ratio: 1.,
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                ratio: 0.7,
                entry: 1,
                tabs: Vec::new(),
            },
            Node::Leaf {
                ratio: 0.3,
                entry: 2,
                tabs: Vec::new(),
            },
        ],
    };
    let before = tree.clone();

    assert!(tree.swap(&1, &2));
    assert_eq!(tree.entries(), vec![2, 1], "they changed places");
    let Node::Split { children, .. } = &tree else {
        panic!("a split");
    };
    assert!(
        (children[0].ratio() - 0.7).abs() < f64::EPSILON,
        "the wide side is still the wide side"
    );

    assert!(tree.swap(&1, &2));
    assert_eq!(tree, before, "twice is where it started");
}

/// A swap reaches across the tree, not just between siblings.
#[test]
fn swapping_reaches_across_the_tree() {
    let mut tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(1),
            Node::split(Axis::Vertical, vec![Node::leaf(2), Node::leaf(3)]),
        ],
    );
    assert!(tree.swap(&1, &3));
    assert_eq!(tree.entries(), vec![3, 2, 1]);
}

/// Swapping something for itself, or for a pane that is not here, changes
/// nothing.
#[test]
fn a_swap_that_means_nothing_does_nothing() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    let before = tree.clone();
    assert!(!tree.swap(&1, &1));
    assert!(!tree.swap(&1, &9));
    assert!(!tree.swap(&9, &1));
    assert_eq!(tree, before);
}

/// A path names the pane it leads to, both ways round.
#[test]
fn a_path_leads_to_the_pane_it_names() {
    let tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(1),
            Node::split(Axis::Vertical, vec![Node::leaf(2), Node::leaf(3)]),
        ],
    );
    let path = tree.path_to(&3).expect("a path");
    assert_eq!(path, vec![1, 1]);
    assert!(matches!(
        tree.at_path(&path),
        Some(Node::Leaf { entry: 3, .. })
    ));
    assert_eq!(tree.path_to(&9), None);
}

// ── tabs ─────────────────────────────────────────────────────────

/// An entry dropped on a pane's bar joins its strip at the end, and the pane
/// stays one pane.
#[test]
fn stacking_puts_an_entry_behind_the_one_it_was_dropped_on() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);

    assert!(tree.stack_onto(&1, &3));
    assert!(tree.stack_onto(&1, &4));

    assert_eq!(tree.leaves(), 2, "still two panes");
    assert_eq!(tree.entries(), vec![1, 3, 4, 2]);
    let pane = tree.at_path(&[0]).expect("the first pane");
    assert_eq!(pane.stack(), vec![1, 3, 4], "in the order they arrived");
    // A tab names its pane as well as the pane's own first entry does.
    assert!(tree.stack_onto(&4, &5));
    assert_eq!(
        tree.at_path(&[0]).expect("the first pane").stack(),
        vec![1, 3, 4, 5],
    );
}

/// An entry the pane already holds is not taken twice, wherever it is named
/// from.
#[test]
fn stacking_what_a_pane_already_holds_changes_nothing() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    tree.stack_onto(&1, &3);

    assert!(tree.stack_onto(&1, &3));
    assert!(tree.stack_onto(&3, &1));

    assert_eq!(tree.at_path(&[0]).expect("a pane").stack(), vec![1, 3]);
}

/// Closing a tab loses the tab; closing the last of them loses the pane.
#[test]
fn a_pane_outlives_its_tabs_until_the_last_one() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    tree.stack_onto(&1, &3);
    tree.stack_onto(&1, &4);

    assert!(tree.remove(&3), "a tab in the middle");
    assert_eq!(tree.at_path(&[0]).expect("a pane").stack(), vec![1, 4]);
    assert_eq!(tree.leaves(), 2);

    // The pane's own name goes: the tab behind it takes the name, and the
    // pane stays where it was.
    assert!(tree.remove(&1));
    assert_eq!(tree.at_path(&[0]).expect("a pane").stack(), vec![4]);
    assert_eq!(tree.leaves(), 2);

    // Nothing left to hold the pane up, so the split collapses to its other
    // child — the rule a pane closed by hand has always followed.
    assert!(tree.remove(&4));
    assert_eq!(tree.leaves(), 1);
    assert_eq!(tree.entries(), vec![2]);
}

/// A pane divides whole: a drop on its edge leaves its tabs on the side they
/// were, rather than scattering them across the new seam.
#[test]
fn splitting_a_pane_keeps_its_tabs_together() {
    let mut tree = Node::leaf(1);
    tree.stack_onto(&1, &2);

    assert!(tree.insert(&1, &3, Side::Right));

    assert_eq!(tree.leaves(), 2);
    assert_eq!(tree.at_path(&[0]).expect("a pane").stack(), vec![1, 2]);
    assert_eq!(tree.at_path(&[1]).expect("a pane").stack(), vec![3]);
}

/// A pane crossing a seam takes its tabs with it, and going back puts
/// everything where it was.
#[test]
fn swapping_carries_whole_panes() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    tree.stack_onto(&1, &3);

    // Named by a tab rather than by the pane's own first entry: either says
    // which pane is meant.
    assert!(tree.swap(&3, &2));
    assert_eq!(tree.at_path(&[0]).expect("a pane").stack(), vec![2]);
    assert_eq!(tree.at_path(&[1]).expect("a pane").stack(), vec![1, 3]);

    assert!(tree.swap(&1, &2));
    assert_eq!(tree.at_path(&[0]).expect("a pane").stack(), vec![1, 3]);
    assert_eq!(tree.at_path(&[1]).expect("a pane").stack(), vec![2]);
}

/// Two entries of one pane are one pane, so there is no seam between them to
/// walk across and no way to swap them with each other.
#[test]
fn tabs_of_one_pane_are_not_neighbours() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    tree.stack_onto(&1, &3);

    assert_eq!(tree.neighbour(&1, Side::Right), Some(2));
    assert_eq!(tree.neighbour(&3, Side::Right), Some(2), "same pane");
    assert_eq!(tree.neighbour(&1, Side::Left), None);
    assert!(!tree.swap(&1, &3), "one pane cannot cross itself");
}

/// Dragging a tab onto another pane's edge takes it out of the strip it was in
/// and gives it a pane of its own.
#[test]
fn relocating_a_tab_pulls_it_out_of_its_strip() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    tree.stack_onto(&1, &3);

    assert!(tree.relocate(&3, &2, Side::Right));

    assert_eq!(tree.leaves(), 3);
    assert_eq!(tree.entries(), vec![1, 2, 3]);
    assert_eq!(tree.at_path(&[0]).expect("a pane").stack(), vec![1]);
}

/// A tab let go over the bar it is already in changes nothing. Taking it out
/// and putting it back would send it to the end of a strip nobody asked to
/// reorder.
#[test]
fn stacking_a_tab_onto_its_own_strip_is_a_no_op() {
    let mut tree = Node::split(Axis::Horizontal, vec![Node::leaf(1), Node::leaf(2)]);
    tree.stack_onto(&1, &3);
    tree.stack_onto(&1, &4);

    // Named from the pane, and from a sibling tab: neither reorders it.
    assert!(tree.stack_onto(&1, &3));
    assert!(tree.stack_onto(&4, &3));

    assert_eq!(tree.at_path(&[0]).expect("a pane").stack(), vec![1, 3, 4]);
}
