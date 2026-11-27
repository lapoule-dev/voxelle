// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! The node arenas: every node of one level in one struct of arrays.
//!
//! Node `n` of a level with `SIZE` slots owns slots `n * SIZE .. (n + 1) *
//! SIZE` of each array; since `SIZE` is a multiple of 64, its mask bits are
//! bits `n * SIZE + i` of the level's mask words. Nodes are named by `u32`
//! index, never by pointer.

use crate::{mask, value::Value};

/// One level of internal nodes: `2^(3 * LOG2)` slots each, every slot a
/// child (an index into the level below) or a tile (a value and a state).
///
/// Invariant: a slot's value-mask bit is off while it holds a child.
#[derive(Clone)]
pub(crate) struct InternalLevel<V, const LOG2: u32> {
    child_mask: Vec<u64>,
    value_mask: Vec<u64>,
    tiles: Vec<V>,
    children: Vec<u32>,
    free: Vec<u32>,
}

impl<V: Value, const LOG2: u32> InternalLevel<V, LOG2> {
    pub(crate) const SIZE: usize = 1 << (3 * LOG2);
    pub(crate) const WORDS: usize = Self::SIZE / 64;

    pub(crate) const fn new() -> Self {
        Self {
            child_mask: Vec::new(),
            value_mask: Vec::new(),
            tiles: Vec::new(),
            children: Vec::new(),
            free: Vec::new(),
        }
    }

    /// A new node, every slot a tile of `(value, active)`.
    pub(crate) fn alloc(&mut self, value: V, active: bool) -> u32 {
        let fill = if active { !0 } else { 0 };
        if let Some(n) = self.free.pop() {
            let (s, w) = (n as usize * Self::SIZE, n as usize * Self::WORDS);
            self.tiles[s..s + Self::SIZE].fill(value);
            self.child_mask[w..w + Self::WORDS].fill(0);
            self.value_mask[w..w + Self::WORDS].fill(fill);
            return n;
        }
        let n = u32::try_from(self.child_mask.len() / Self::WORDS)
            .expect("more than u32::MAX nodes in one level");
        self.tiles.resize(self.tiles.len() + Self::SIZE, value);
        self.children.resize(self.children.len() + Self::SIZE, 0);
        self.child_mask.resize(self.child_mask.len() + Self::WORDS, 0);
        self.value_mask.resize(self.value_mask.len() + Self::WORDS, fill);
        n
    }

    /// Gives node `n` back. Its children must have been released or moved.
    pub(crate) fn release(&mut self, n: u32) {
        self.free.push(n);
    }

    #[inline]
    fn slot(n: u32, i: usize) -> usize {
        n as usize * Self::SIZE + i
    }

    #[inline]
    pub(crate) fn is_child(&self, n: u32, i: usize) -> bool {
        mask::get(&self.child_mask, Self::slot(n, i))
    }

    #[inline]
    pub(crate) fn child(&self, n: u32, i: usize) -> u32 {
        self.children[Self::slot(n, i)]
    }

    #[inline]
    pub(crate) fn tile(&self, n: u32, i: usize) -> (V, bool) {
        let g = Self::slot(n, i);
        (self.tiles[g], mask::get(&self.value_mask, g))
    }

    pub(crate) fn set_child(&mut self, n: u32, i: usize, child: u32) {
        let g = Self::slot(n, i);
        mask::put(&mut self.child_mask, g, true);
        mask::put(&mut self.value_mask, g, false);
        self.children[g] = child;
    }

    pub(crate) fn set_tile(&mut self, n: u32, i: usize, value: V, active: bool) {
        let g = Self::slot(n, i);
        mask::put(&mut self.child_mask, g, false);
        mask::put(&mut self.value_mask, g, active);
        self.tiles[g] = value;
    }

    pub(crate) fn child_mask(&self, n: u32) -> &[u64] {
        let w = n as usize * Self::WORDS;
        &self.child_mask[w..w + Self::WORDS]
    }

    pub(crate) fn value_mask(&self, n: u32) -> &[u64] {
        let w = n as usize * Self::WORDS;
        &self.value_mask[w..w + Self::WORDS]
    }

    /// `Some((value, active))` when node `n` has no child and every slot is
    /// the same tile.
    pub(crate) fn uniform(&self, n: u32) -> Option<(V, bool)> {
        if !mask::all(self.child_mask(n), false) {
            return None;
        }
        let vm = self.value_mask(n);
        let active = vm[0] & 1 != 0;
        if !mask::all(vm, active) {
            return None;
        }
        let s = n as usize * Self::SIZE;
        let tiles = &self.tiles[s..s + Self::SIZE];
        let first = tiles[0];
        tiles.iter().all(|v| v.same(&first)).then_some((first, active))
    }

    /// The nodes in use.
    pub(crate) fn live(&self) -> usize {
        self.child_mask.len() / Self::WORDS - self.free.len()
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        (self.child_mask.capacity() + self.value_mask.capacity()) * 8
            + self.tiles.capacity() * size_of::<V>()
            + (self.children.capacity() + self.free.capacity()) * 4
    }
}

/// The leaves: `2^(3 * LOG2)` values and an active mask each.
#[derive(Clone)]
pub(crate) struct LeafLevel<V, const LOG2: u32> {
    value_mask: Vec<u64>,
    values: Vec<V>,
    free: Vec<u32>,
}

impl<V: Value, const LOG2: u32> LeafLevel<V, LOG2> {
    pub(crate) const SIZE: usize = 1 << (3 * LOG2);
    pub(crate) const WORDS: usize = Self::SIZE / 64;

    pub(crate) const fn new() -> Self {
        Self {
            value_mask: Vec::new(),
            values: Vec::new(),
            free: Vec::new(),
        }
    }

    /// A new leaf, every voxel `(value, active)`.
    pub(crate) fn alloc(&mut self, value: V, active: bool) -> u32 {
        let fill = if active { !0 } else { 0 };
        if let Some(n) = self.free.pop() {
            let (s, w) = (n as usize * Self::SIZE, n as usize * Self::WORDS);
            self.values[s..s + Self::SIZE].fill(value);
            self.value_mask[w..w + Self::WORDS].fill(fill);
            return n;
        }
        let n = u32::try_from(self.value_mask.len() / Self::WORDS).expect("more than u32::MAX leaves");
        self.values.resize(self.values.len() + Self::SIZE, value);
        self.value_mask.resize(self.value_mask.len() + Self::WORDS, fill);
        n
    }

    pub(crate) fn release(&mut self, n: u32) {
        self.free.push(n);
    }

    #[inline]
    pub(crate) fn value(&self, n: u32, i: usize) -> V {
        self.values[n as usize * Self::SIZE + i]
    }

    #[inline]
    pub(crate) fn is_on(&self, n: u32, i: usize) -> bool {
        mask::get(&self.value_mask, n as usize * Self::SIZE + i)
    }

    #[inline]
    pub(crate) fn set(&mut self, n: u32, i: usize, value: V, active: bool) {
        let g = n as usize * Self::SIZE + i;
        self.values[g] = value;
        mask::put(&mut self.value_mask, g, active);
    }

    pub(crate) fn mask(&self, n: u32) -> &[u64] {
        let w = n as usize * Self::WORDS;
        &self.value_mask[w..w + Self::WORDS]
    }

    /// `Some((value, active))` when every voxel of leaf `n` is the same.
    pub(crate) fn uniform(&self, n: u32) -> Option<(V, bool)> {
        let m = self.mask(n);
        let active = m[0] & 1 != 0;
        if !mask::all(m, active) {
            return None;
        }
        let s = n as usize * Self::SIZE;
        let values = &self.values[s..s + Self::SIZE];
        let first = values[0];
        values.iter().all(|v| v.same(&first)).then_some((first, active))
    }

    pub(crate) fn live(&self) -> usize {
        self.value_mask.len() / Self::WORDS - self.free.len()
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.value_mask.capacity() * 8 + self.values.capacity() * size_of::<V>() + self.free.capacity() * 4
    }
}
