// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! Accessors: random access that remembers the path of the last query.
//!
//! The cache holds `(origin, index)` of the last leaf, lower and upper node
//! visited. It stays valid for as long as the accessor lives: nodes are only
//! freed by operations that take `&mut Tree`, which the accessor's borrow
//! rules out.

use crate::coord::Coord;
use crate::tree::{Loc, Path, Tree};
use crate::value::Value;

#[derive(Clone, Copy, Default)]
struct PathCache {
    upper: Option<(Coord, u32)>,
    lower: Option<(Coord, u32)>,
    leaf: Option<(Coord, u32)>,
}

impl Path for PathCache {
    #[inline]
    fn upper(&mut self, key: Coord, n: u32) {
        self.upper = Some((key, n));
    }
    #[inline]
    fn lower(&mut self, key: Coord, n: u32) {
        self.lower = Some((key, n));
    }
    #[inline]
    fn leaf(&mut self, key: Coord, n: u32) {
        self.leaf = Some((key, n));
    }
}

impl PathCache {
    #[inline]
    fn locate<V: Value, const L2: u32, const L1: u32, const L0: u32>(
        &mut self,
        tree: &Tree<V, L2, L1, L0>,
        c: Coord,
    ) -> Loc<V> {
        if let Some((k, n)) = self.leaf
            && k == Tree::<V, L2, L1, L0>::key(c, L0)
        {
            return Loc::Leaf(n, Tree::<V, L2, L1, L0>::leaf_offset(c));
        }
        if let Some((k, n)) = self.lower
            && k == Tree::<V, L2, L1, L0>::key(c, Tree::<V, L2, L1, L0>::LOWER_LOG2)
        {
            return tree.locate_lower(n, c, self);
        }
        if let Some((k, n)) = self.upper
            && k == Tree::<V, L2, L1, L0>::key(c, Tree::<V, L2, L1, L0>::UPPER_LOG2)
        {
            return tree.locate_upper(n, c, self);
        }
        tree.locate(c, self)
    }

    #[inline]
    fn modify<V: Value, const L2: u32, const L1: u32, const L0: u32, F: Fn(V, bool) -> (V, bool)>(
        &mut self,
        tree: &mut Tree<V, L2, L1, L0>,
        c: Coord,
        f: &F,
    ) {
        if let Some((k, n)) = self.leaf
            && k == Tree::<V, L2, L1, L0>::key(c, L0)
        {
            return tree.modify_leaf(n, c, f);
        }
        if let Some((k, n)) = self.lower
            && k == Tree::<V, L2, L1, L0>::key(c, Tree::<V, L2, L1, L0>::LOWER_LOG2)
        {
            return tree.modify_lower(n, c, f, self);
        }
        if let Some((k, n)) = self.upper
            && k == Tree::<V, L2, L1, L0>::key(c, Tree::<V, L2, L1, L0>::UPPER_LOG2)
        {
            return tree.modify_upper(n, c, f, self);
        }
        tree.modify(c, f, self)
    }
}

/// Read access to a tree through a cache of the last path taken.
///
/// Many accessors can read one tree at once, on as many threads.
pub struct Accessor<'a, V, const L2: u32 = 5, const L1: u32 = 4, const L0: u32 = 3> {
    tree: &'a Tree<V, L2, L1, L0>,
    cache: PathCache,
}

impl<'a, V: Value, const L2: u32, const L1: u32, const L0: u32> Accessor<'a, V, L2, L1, L0> {
    pub(crate) fn new(tree: &'a Tree<V, L2, L1, L0>) -> Self {
        Self {
            tree,
            cache: PathCache::default(),
        }
    }

    /// The tree read.
    pub fn tree(&self) -> &'a Tree<V, L2, L1, L0> {
        self.tree
    }

    /// The value at `c`.
    #[inline]
    pub fn get(&mut self, c: Coord) -> V {
        let loc = self.cache.locate(self.tree, c);
        self.tree.read_value(loc)
    }

    /// Whether the voxel at `c` is active.
    #[inline]
    pub fn is_active(&mut self, c: Coord) -> bool {
        let loc = self.cache.locate(self.tree, c);
        self.tree.read_active(loc)
    }

    /// The value at `c` and whether it is active.
    #[inline]
    pub fn probe(&mut self, c: Coord) -> (V, bool) {
        let loc = self.cache.locate(self.tree, c);
        self.tree.read(loc)
    }
}

/// Read and write access to a tree through a cache of the last path taken.
pub struct AccessorMut<'a, V, const L2: u32 = 5, const L1: u32 = 4, const L0: u32 = 3> {
    tree: &'a mut Tree<V, L2, L1, L0>,
    cache: PathCache,
}

impl<'a, V: Value, const L2: u32, const L1: u32, const L0: u32> AccessorMut<'a, V, L2, L1, L0> {
    pub(crate) fn new(tree: &'a mut Tree<V, L2, L1, L0>) -> Self {
        Self {
            tree,
            cache: PathCache::default(),
        }
    }

    /// The value at `c`.
    #[inline]
    pub fn get(&mut self, c: Coord) -> V {
        let loc = self.cache.locate(self.tree, c);
        self.tree.read_value(loc)
    }

    /// Whether the voxel at `c` is active.
    #[inline]
    pub fn is_active(&mut self, c: Coord) -> bool {
        let loc = self.cache.locate(self.tree, c);
        self.tree.read_active(loc)
    }

    /// The value at `c` and whether it is active.
    #[inline]
    pub fn probe(&mut self, c: Coord) -> (V, bool) {
        let loc = self.cache.locate(self.tree, c);
        self.tree.read(loc)
    }

    /// Writes `value` at `c` and makes the voxel active.
    #[inline]
    pub fn set(&mut self, c: Coord, value: V) {
        self.cache.modify(self.tree, c, &move |_, _| (value, true));
    }

    /// Writes `value` at `c` and makes the voxel inactive.
    #[inline]
    pub fn set_inactive(&mut self, c: Coord, value: V) {
        self.cache.modify(self.tree, c, &move |_, _| (value, false));
    }

    /// Changes only the active state of the voxel at `c`.
    #[inline]
    pub fn set_active(&mut self, c: Coord, active: bool) {
        self.cache.modify(self.tree, c, &move |v, _| (v, active));
    }
}
