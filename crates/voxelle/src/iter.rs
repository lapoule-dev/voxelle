// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! Iteration over active values.

use std::collections::btree_map;

use crate::coord::{Coord, CoordBBox};
use crate::mask;
use crate::tree::{RootSlot, Tree};
use crate::value::Value;

/// An active value: one voxel (`log2_dim == 0`) or a tile covering a cube
/// of `2^log2_dim` voxels per axis from `origin`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Active<V> {
    /// The smallest corner of the voxel or tile.
    pub origin: Coord,
    /// Log2 of the edge, in voxels: 0 for a voxel.
    pub log2_dim: u32,
    /// The value of every voxel covered.
    pub value: V,
}

impl<V> Active<V> {
    /// Whether this is a single voxel.
    pub fn is_voxel(&self) -> bool {
        self.log2_dim == 0
    }

    /// The voxels covered.
    pub fn bbox(&self) -> CoordBBox {
        let d = (1i32 << self.log2_dim) - 1;
        let o = self.origin;
        CoordBBox::new(o, Coord::new(o.x + d, o.y + d, o.z + d))
    }
}

#[derive(Clone, Copy)]
struct Cursor {
    node: u32,
    origin: Coord,
    next: usize,
}

/// The iterator of [`Tree::iter_active`]: active voxels and tiles in tree
/// order (root map order, then slot order, x-major), without allocating.
pub struct ActiveIter<'a, V, const L2: u32 = 5, const L1: u32 = 4, const L0: u32 = 3> {
    tree: &'a Tree<V, L2, L1, L0>,
    root: btree_map::Iter<'a, Coord, RootSlot<V>>,
    upper: Option<Cursor>,
    lower: Option<Cursor>,
    leaf: Option<Cursor>,
}

impl<'a, V: Value, const L2: u32, const L1: u32, const L0: u32> ActiveIter<'a, V, L2, L1, L0> {
    pub(crate) fn new(tree: &'a Tree<V, L2, L1, L0>) -> Self {
        Self {
            tree,
            root: tree.root.iter(),
            upper: None,
            lower: None,
            leaf: None,
        }
    }
}

impl<V: Value, const L2: u32, const L1: u32, const L0: u32> Iterator for ActiveIter<'_, V, L2, L1, L0> {
    type Item = Active<V>;

    fn next(&mut self) -> Option<Active<V>> {
        let tree = self.tree;
        loop {
            if let Some(cur) = &mut self.leaf {
                match mask::next_on(tree.leaves.mask(cur.node), cur.next) {
                    Some(i) => {
                        cur.next = i + 1;
                        return Some(Active {
                            origin: Tree::<V, L2, L1, L0>::slot_origin(cur.origin, i, L0, 0),
                            log2_dim: 0,
                            value: tree.leaves.value(cur.node, i),
                        });
                    }
                    None => self.leaf = None,
                }
                continue;
            }
            if let Some(cur) = &mut self.lower {
                let (n, lv) = (cur.node, &tree.lower);
                match mask::next_on_either(lv.child_mask(n), lv.value_mask(n), cur.next) {
                    Some(i) => {
                        cur.next = i + 1;
                        let origin = Tree::<V, L2, L1, L0>::slot_origin(cur.origin, i, L1, L0);
                        if lv.is_child(n, i) {
                            self.leaf = Some(Cursor {
                                node: lv.child(n, i),
                                origin,
                                next: 0,
                            });
                        } else {
                            return Some(Active {
                                origin,
                                log2_dim: L0,
                                value: lv.tile(n, i).0,
                            });
                        }
                    }
                    None => self.lower = None,
                }
                continue;
            }
            if let Some(cur) = &mut self.upper {
                let (n, uv) = (cur.node, &tree.upper);
                match mask::next_on_either(uv.child_mask(n), uv.value_mask(n), cur.next) {
                    Some(i) => {
                        cur.next = i + 1;
                        let origin = Tree::<V, L2, L1, L0>::slot_origin(cur.origin, i, L2, L0 + L1);
                        if uv.is_child(n, i) {
                            self.lower = Some(Cursor {
                                node: uv.child(n, i),
                                origin,
                                next: 0,
                            });
                        } else {
                            return Some(Active {
                                origin,
                                log2_dim: L0 + L1,
                                value: uv.tile(n, i).0,
                            });
                        }
                    }
                    None => self.upper = None,
                }
                continue;
            }
            match self.root.next()? {
                (&origin, &RootSlot::Tile { value, active: true }) => {
                    return Some(Active {
                        origin,
                        log2_dim: L0 + L1 + L2,
                        value,
                    });
                }
                (_, RootSlot::Tile { .. }) => {}
                (&origin, &RootSlot::Child(node)) => {
                    self.upper = Some(Cursor {
                        node,
                        origin,
                        next: 0,
                    });
                }
            }
        }
    }
}
