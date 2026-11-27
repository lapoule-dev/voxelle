// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! Behaviours of the tree the model test cannot see: what is allocated,
//! what collapses, at every level.

use voxelle::{Active, Coord, CoordBBox, Tree};

fn c(x: i32, y: i32, z: i32) -> Coord {
    Coord::new(x, y, z)
}

#[test]
fn an_empty_tree_reads_the_inactive_background_and_holds_nothing() {
    let tree = Tree::<f32>::new(-1.0);
    assert_eq!(tree.probe(c(5, -7, 1 << 30)), (-1.0, false));
    assert!(tree.is_empty());
    assert_eq!(tree.stats().heap_bytes, 0);
    assert_eq!(tree.iter_active().count(), 0);
    assert_eq!(tree.active_bbox(), None);
}

#[test]
fn set_writes_and_activates_set_inactive_and_set_active_change_one_thing_each() {
    let mut tree = Tree::<f32>::new(0.0);
    tree.set(c(1, 2, 3), 4.0);
    assert_eq!(tree.probe(c(1, 2, 3)), (4.0, true));
    tree.set_active(c(1, 2, 3), false);
    assert_eq!(tree.probe(c(1, 2, 3)), (4.0, false));
    tree.set_inactive(c(1, 2, 4), 9.0);
    assert_eq!(tree.probe(c(1, 2, 4)), (9.0, false));
    tree.set_active(c(1, 2, 4), true);
    assert_eq!(tree.probe(c(1, 2, 4)), (9.0, true));
    // The neighbours in the same leaf are untouched background.
    assert_eq!(tree.probe(c(1, 2, 5)), (0.0, false));
}

#[test]
fn a_write_that_changes_nothing_allocates_nothing() {
    let mut tree = Tree::<f32>::new(0.0);
    tree.set_active(c(10, 10, 10), false);
    tree.set_inactive(c(-3, 0, 9), 0.0);
    assert!(tree.is_empty());
    assert_eq!(tree.stats().leaf_nodes, 0);

    // Inside an active tile of 1.0, writing 1.0 again keeps the tile.
    let mut tiny = Tree::<f32, 2, 2, 2>::new(0.0);
    for p in CoordBBox::new(c(0, 0, 0), c(3, 3, 3)).coords() {
        tiny.set(p, 1.0);
    }
    tiny.prune();
    assert_eq!(tiny.stats().leaf_nodes, 0);
    tiny.set(c(1, 1, 1), 1.0);
    tiny.set_active(c(2, 2, 2), true);
    assert_eq!(tiny.stats().leaf_nodes, 0);
}

#[test]
fn the_ends_of_the_index_space_are_addressable() {
    let mut tree = Tree::<i32>::new(0);
    let corners = [
        c(i32::MIN, i32::MIN, i32::MIN),
        c(i32::MAX, i32::MAX, i32::MAX),
        c(i32::MIN, 0, i32::MAX),
    ];
    for (k, &p) in corners.iter().enumerate() {
        tree.set(p, k as i32 + 1);
    }
    for (k, &p) in corners.iter().enumerate() {
        assert_eq!(tree.probe(p), (k as i32 + 1, true));
    }
    assert_eq!(tree.stats().root_entries, 3);
    let mut listed: Vec<_> = tree.iter_active_voxels().collect();
    listed.sort();
    let mut expected: Vec<_> = corners
        .iter()
        .enumerate()
        .map(|(k, &p)| (p, k as i32 + 1))
        .collect();
    expected.sort();
    assert_eq!(listed, expected);
    assert_eq!(
        tree.active_bbox(),
        Some(CoordBBox::new(
            c(i32::MIN, i32::MIN, i32::MIN),
            c(i32::MAX, i32::MAX, i32::MAX)
        ))
    );
}

#[test]
fn prune_collapses_a_uniform_leaf_and_a_later_write_densifies_it_again() {
    let mut tree = Tree::<f32>::new(0.0);
    let leaf = CoordBBox::new(c(8, 16, -8), c(15, 23, -1));
    for p in leaf.coords() {
        tree.set(p, 2.0);
    }
    assert_eq!(tree.stats().leaf_nodes, 1);
    tree.prune();
    let s = tree.stats();
    assert_eq!((s.leaf_nodes, s.lower_nodes, s.active_tiles), (0, 1, 1));
    let items: Vec<_> = tree.iter_active().collect();
    assert_eq!(
        items,
        [Active {
            origin: c(8, 16, -8),
            log2_dim: 3,
            value: 2.0
        }]
    );
    assert_eq!(tree.active_voxel_count(), 512);

    tree.set(c(9, 17, -7), 5.0);
    assert_eq!(tree.stats().leaf_nodes, 1);
    assert_eq!(tree.get(c(9, 17, -7)), 5.0);
    assert_eq!(tree.probe(c(8, 16, -8)), (2.0, true));
    assert_eq!(tree.active_voxel_count(), 512);
}

#[test]
fn prune_climbs_every_level_up_to_a_root_tile() {
    // 2-2-2: a root region is 64³ voxels.
    let mut tree = Tree::<f32, 2, 2, 2>::new(0.0);
    let region = CoordBBox::new(c(-64, 0, 64), c(-1, 63, 127));
    {
        let mut acc = tree.accessor_mut();
        for p in region.coords() {
            acc.set(p, 3.0);
        }
    }
    tree.prune();
    let s = tree.stats();
    assert_eq!(
        (s.upper_nodes, s.lower_nodes, s.leaf_nodes, s.root_entries),
        (0, 0, 0, 1)
    );
    assert_eq!(
        tree.iter_active().collect::<Vec<_>>(),
        [Active {
            origin: c(-64, 0, 64),
            log2_dim: 6,
            value: 3.0
        }]
    );
    assert_eq!(tree.active_voxel_count(), 64 * 64 * 64);
    assert_eq!(tree.active_bbox(), Some(region));
    assert_eq!(tree.probe(c(-1, 63, 127)), (3.0, true));
    assert_eq!(tree.probe(c(0, 63, 127)), (0.0, false));
}

#[test]
fn prune_drops_what_equals_the_inactive_background() {
    let mut tree = Tree::<f32>::new(0.0);
    tree.set(c(0, 0, 0), 1.0);
    tree.set_inactive(c(0, 0, 0), 0.0);
    assert_eq!(tree.stats().leaf_nodes, 1);
    tree.prune();
    assert!(tree.is_empty());
    assert_eq!(tree.stats().upper_nodes, 0);
}

#[test]
fn prune_keeps_bitwise_different_values_apart() {
    let mut tree = Tree::<f32, 2, 2, 2>::new(0.0);
    for p in CoordBBox::new(c(0, 0, 0), c(3, 3, 3)).coords() {
        tree.set(p, 0.0);
    }
    tree.set(c(3, 3, 3), -0.0);
    tree.prune();
    assert_eq!(tree.stats().leaf_nodes, 1);
    assert_eq!(tree.get(c(3, 3, 3)).to_bits(), (-0.0f32).to_bits());
}

#[test]
fn prune_inactive_removes_nodes_with_no_active_voxel_only() {
    let mut tree = Tree::<f32>::new(0.0);
    tree.set(c(0, 0, 0), 1.0);
    tree.set_inactive(c(1, 0, 0), 7.0); // same leaf as an active voxel
    tree.set_inactive(c(500, 0, 0), 7.0); // a leaf of its own
    tree.set_inactive(c(10_000, 0, 0), 7.0); // an upper node of its own
    assert_eq!(tree.stats().leaf_nodes, 3);
    tree.prune_inactive();
    let s = tree.stats();
    assert_eq!((s.leaf_nodes, s.upper_nodes, s.root_entries), (1, 1, 1));
    assert_eq!(tree.probe(c(0, 0, 0)), (1.0, true));
    assert_eq!(tree.probe(c(1, 0, 0)), (7.0, false));
    assert_eq!(tree.probe(c(500, 0, 0)), (0.0, false));
    assert_eq!(tree.probe(c(10_000, 0, 0)), (0.0, false));
}

#[test]
fn freed_nodes_are_reused_with_fresh_contents() {
    let mut tree = Tree::<f32>::new(0.0);
    for p in CoordBBox::new(c(0, 0, 0), c(7, 7, 7)).coords() {
        tree.set(p, 1.0);
    }
    tree.set(c(64, 0, 0), 4.0);
    tree.prune();
    assert_eq!(tree.stats().leaf_nodes, 1);
    let before = tree.stats().heap_bytes;
    // A new leaf takes the freed slot: no growth, and none of its old
    // values or active bits leak through.
    tree.set(c(0, 0, 16), 2.0);
    assert_eq!(tree.stats().leaf_nodes, 2);
    assert_eq!(tree.stats().heap_bytes, before);
    assert_eq!(tree.probe(c(0, 0, 17)), (0.0, false));
    assert_eq!(tree.active_voxel_count(), 512 + 2);
}

#[test]
fn clear_and_clone() {
    let mut tree = Tree::<f32>::new(0.5);
    tree.set(c(1, 1, 1), 1.0);
    let copy = tree.clone();
    tree.clear();
    assert!(tree.is_empty());
    assert_eq!(tree.get(c(1, 1, 1)), 0.5);
    assert_eq!(copy.probe(c(1, 1, 1)), (1.0, true));
}

#[test]
fn accessors_read_across_nodes_and_follow_their_own_writes() {
    let mut tree = Tree::<u8>::new(0);
    {
        let mut acc = tree.accessor_mut();
        for x in -300..300 {
            acc.set(c(x, x / 3, -x), (x & 0x7f) as u8);
            assert_eq!(acc.get(c(x, x / 3, -x)), (x & 0x7f) as u8);
        }
    }
    let mut acc = tree.accessor();
    for x in (-300..300).rev() {
        assert_eq!(acc.probe(c(x, x / 3, -x)), ((x & 0x7f) as u8, true));
        assert_eq!(acc.probe(c(x, x / 3 + 1, -x)), (0, false));
    }
    assert_eq!(tree.active_voxel_count(), 600);
}

#[test]
fn a_tree_is_send_sync_and_readable_from_many_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Tree<f32>>();
    let mut tree = Tree::<f32>::new(0.0);
    for x in 0..1000 {
        tree.set(c(x, 0, 0), x as f32);
    }
    std::thread::scope(|s| {
        for t in 0..4 {
            let tree = &tree;
            s.spawn(move || {
                let mut acc = tree.accessor();
                for x in (t..1000).step_by(4) {
                    assert_eq!(acc.get(c(x, 0, 0)), x as f32);
                }
            });
        }
    });
}
