// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! Topology operations: collapsing uniform nodes into tiles.

use crate::mask;
use crate::tree::{RootSlot, Tree};
use crate::value::Value;

impl<V: Value, const L2: u32, const L1: u32, const L0: u32> Tree<V, L2, L1, L0> {
    /// Replaces every node whose voxels all have the same value and state
    /// by a tile in its parent, bottom-up, and drops root tiles equal to the
    /// inactive background.
    ///
    /// Lossless: every read returns what it returned before. Values compare
    /// with [`Value::same`] (bitwise for floats).
    pub fn prune(&mut self) {
        for (k, u) in self.root_children() {
            if let Some((value, active)) = self.prune_upper(u) {
                self.upper.release(u);
                self.root.insert(k, RootSlot::Tile { value, active });
            }
        }
        let bg = self.background;
        self.root
            .retain(|_, s| !matches!(s, RootSlot::Tile { value, active: false } if value.same(&bg)));
    }

    /// Replaces every node that holds no active voxel by an inactive
    /// background tile, and drops the upper nodes left with nothing else.
    ///
    /// Active voxels keep their values; an inactive voxel inside a removed
    /// node reads the background afterwards. Inactive tiles are kept.
    pub fn prune_inactive(&mut self) {
        for (k, u) in self.root_children() {
            if self.prune_inactive_upper(u) {
                self.upper.release(u);
                self.root.remove(&k);
            }
        }
    }

    fn root_children(&self) -> Vec<(crate::Coord, u32)> {
        self.root
            .iter()
            .filter_map(|(k, s)| match *s {
                RootSlot::Child(u) => Some((*k, u)),
                RootSlot::Tile { .. } => None,
            })
            .collect()
    }

    fn prune_upper(&mut self, u: u32) -> Option<(V, bool)> {
        let mut from = 0;
        while let Some(i) = mask::next_on(self.upper.child_mask(u), from) {
            let l = self.upper.child(u, i);
            if let Some((v, a)) = self.prune_lower(l) {
                self.lower.release(l);
                self.upper.set_tile(u, i, v, a);
            }
            from = i + 1;
        }
        self.upper.uniform(u)
    }

    fn prune_lower(&mut self, l: u32) -> Option<(V, bool)> {
        let mut from = 0;
        while let Some(i) = mask::next_on(self.lower.child_mask(l), from) {
            let n = self.lower.child(l, i);
            if let Some((v, a)) = self.leaves.uniform(n) {
                self.leaves.release(n);
                self.lower.set_tile(l, i, v, a);
            }
            from = i + 1;
        }
        self.lower.uniform(l)
    }

    /// Whether upper node `u` is left with no child and no active tile.
    fn prune_inactive_upper(&mut self, u: u32) -> bool {
        let bg = self.background;
        let mut from = 0;
        while let Some(i) = mask::next_on(self.upper.child_mask(u), from) {
            let l = self.upper.child(u, i);
            if self.prune_inactive_lower(l) {
                self.lower.release(l);
                self.upper.set_tile(u, i, bg, false);
            }
            from = i + 1;
        }
        mask::all(self.upper.child_mask(u), false) && mask::all(self.upper.value_mask(u), false)
    }

    fn prune_inactive_lower(&mut self, l: u32) -> bool {
        let bg = self.background;
        let mut from = 0;
        while let Some(i) = mask::next_on(self.lower.child_mask(l), from) {
            let n = self.lower.child(l, i);
            if mask::all(self.leaves.mask(n), false) {
                self.leaves.release(n);
                self.lower.set_tile(l, i, bg, false);
            }
            from = i + 1;
        }
        mask::all(self.lower.child_mask(l), false) && mask::all(self.lower.value_mask(l), false)
    }
}
