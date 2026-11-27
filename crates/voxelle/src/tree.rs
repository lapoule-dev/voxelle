// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! The tree: a root map over upper and lower internal nodes and leaves.

use std::collections::BTreeMap;

use crate::accessor::{Accessor, AccessorMut};
use crate::coord::{Coord, CoordBBox};
use crate::iter::ActiveIter;
use crate::mask;
use crate::node::{InternalLevel, LeafLevel};
use crate::value::Value;

/// What the root holds for one upper-node region.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RootSlot<V> {
    Tile { value: V, active: bool },
    Child(u32),
}

/// Where a voxel's value lives: in a tile (copied out) or in a leaf.
#[derive(Clone, Copy)]
pub(crate) enum Loc<V> {
    Tile(V, bool),
    Leaf(u32, usize),
}

/// Receives the nodes a descent goes through (the accessor's cache).
pub(crate) trait Path {
    fn upper(&mut self, key: Coord, n: u32);
    fn lower(&mut self, key: Coord, n: u32);
    fn leaf(&mut self, key: Coord, n: u32);
}

/// A descent that remembers nothing.
pub(crate) struct NoPath;

impl Path for NoPath {
    #[inline(always)]
    fn upper(&mut self, _: Coord, _: u32) {}
    #[inline(always)]
    fn lower(&mut self, _: Coord, _: u32) {}
    #[inline(always)]
    fn leaf(&mut self, _: Coord, _: u32) {}
}

/// A sparse tree of voxel values over the whole `i32` index space.
///
/// Every voxel has a value and an active state; a voxel never written reads
/// as the *background*, inactive. The shape is a root map over upper nodes
/// of `2^L2` children per axis, lower nodes of `2^L1`, and leaves of `2^L0`
/// voxels per axis; the default 5-4-3 makes leaves of 8³ voxels, lower
/// nodes of 128³ and upper nodes of 4096³.
#[derive(Clone)]
pub struct Tree<V, const L2: u32 = 5, const L1: u32 = 4, const L0: u32 = 3> {
    pub(crate) background: V,
    pub(crate) root: BTreeMap<Coord, RootSlot<V>>,
    pub(crate) upper: InternalLevel<V, L2>,
    pub(crate) lower: InternalLevel<V, L1>,
    pub(crate) leaves: LeafLevel<V, L0>,
}

/// Node counts and memory of a tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Entries of the root map (upper nodes and root tiles).
    pub root_entries: usize,
    /// Upper internal nodes in use.
    pub upper_nodes: usize,
    /// Lower internal nodes in use.
    pub lower_nodes: usize,
    /// Leaves in use.
    pub leaf_nodes: usize,
    /// Active tiles, at every level.
    pub active_tiles: u64,
    /// Active voxels, tiles counted by their volume.
    pub active_voxels: u64,
    /// Bytes of heap the tree holds, freed nodes included (an estimate for
    /// the root map).
    pub heap_bytes: usize,
}

impl<V: Value, const L2: u32, const L1: u32, const L0: u32> Tree<V, L2, L1, L0> {
    const SHAPE_OK: () = assert!(
        L0 >= 2 && L1 >= 2 && L2 >= 2 && L0 + L1 + L2 <= 30,
        "each level needs log2 >= 2 (whole mask words) and the total must fit 30 bits"
    );

    /// Log2 of a leaf's edge in voxels.
    pub const LEAF_LOG2: u32 = L0;
    /// Log2 of a lower node's edge in voxels.
    pub const LOWER_LOG2: u32 = L0 + L1;
    /// Log2 of an upper node's edge in voxels.
    pub const UPPER_LOG2: u32 = L0 + L1 + L2;

    /// An empty tree whose every voxel reads `background`, inactive.
    /// Allocates nothing.
    pub fn new(background: V) -> Self {
        let () = Self::SHAPE_OK;
        Self {
            background,
            root: BTreeMap::new(),
            upper: InternalLevel::new(),
            lower: InternalLevel::new(),
            leaves: LeafLevel::new(),
        }
    }

    /// The value of every voxel never written.
    pub fn background(&self) -> V {
        self.background
    }

    /// Whether no voxel differs from the inactive background *structurally*:
    /// the root map is empty.
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    // --- coordinate arithmetic ------------------------------------------

    #[inline]
    pub(crate) fn key(c: Coord, log2: u32) -> Coord {
        c.masked(!((1i32 << log2) - 1))
    }

    #[inline]
    pub(crate) fn leaf_offset(c: Coord) -> usize {
        let m = (1i32 << L0) - 1;
        (((c.x & m) as usize) << (2 * L0)) | (((c.y & m) as usize) << L0) | ((c.z & m) as usize)
    }

    #[inline]
    fn internal_offset(c: Coord, log2: u32, child: u32) -> usize {
        let m = (1i32 << (log2 + child)) - 1;
        ((((c.x & m) >> child) as usize) << (2 * log2))
            | ((((c.y & m) >> child) as usize) << log2)
            | (((c.z & m) >> child) as usize)
    }

    #[inline]
    pub(crate) fn lower_offset(c: Coord) -> usize {
        Self::internal_offset(c, L1, L0)
    }

    #[inline]
    pub(crate) fn upper_offset(c: Coord) -> usize {
        Self::internal_offset(c, L2, L0 + L1)
    }

    /// The origin of slot `i` of a node at `origin`, `log2` slots per axis,
    /// each `2^child` voxels wide.
    #[inline]
    pub(crate) fn slot_origin(origin: Coord, i: usize, log2: u32, child: u32) -> Coord {
        let m = (1usize << log2) - 1;
        let at = |s: u32| (((i >> s) & m) as i32) << child;
        Coord::new(origin.x + at(2 * log2), origin.y + at(log2), origin.z + at(0))
    }

    // --- reading --------------------------------------------------------

    pub(crate) fn locate<P: Path>(&self, c: Coord, p: &mut P) -> Loc<V> {
        let k = Self::key(c, Self::UPPER_LOG2);
        match self.root.get(&k) {
            None => Loc::Tile(self.background, false),
            Some(&RootSlot::Tile { value, active }) => Loc::Tile(value, active),
            Some(&RootSlot::Child(u)) => {
                p.upper(k, u);
                self.locate_upper(u, c, p)
            }
        }
    }

    #[inline]
    pub(crate) fn locate_upper<P: Path>(&self, u: u32, c: Coord, p: &mut P) -> Loc<V> {
        let i = Self::upper_offset(c);
        if self.upper.is_child(u, i) {
            let l = self.upper.child(u, i);
            p.lower(Self::key(c, Self::LOWER_LOG2), l);
            self.locate_lower(l, c, p)
        } else {
            let (v, a) = self.upper.tile(u, i);
            Loc::Tile(v, a)
        }
    }

    #[inline]
    pub(crate) fn locate_lower<P: Path>(&self, l: u32, c: Coord, p: &mut P) -> Loc<V> {
        let i = Self::lower_offset(c);
        if self.lower.is_child(l, i) {
            let f = self.lower.child(l, i);
            p.leaf(Self::key(c, L0), f);
            Loc::Leaf(f, Self::leaf_offset(c))
        } else {
            let (v, a) = self.lower.tile(l, i);
            Loc::Tile(v, a)
        }
    }

    #[inline]
    pub(crate) fn read(&self, loc: Loc<V>) -> (V, bool) {
        match loc {
            Loc::Tile(v, a) => (v, a),
            Loc::Leaf(n, i) => (self.leaves.value(n, i), self.leaves.is_on(n, i)),
        }
    }

    #[inline]
    pub(crate) fn read_value(&self, loc: Loc<V>) -> V {
        match loc {
            Loc::Tile(v, _) => v,
            Loc::Leaf(n, i) => self.leaves.value(n, i),
        }
    }

    #[inline]
    pub(crate) fn read_active(&self, loc: Loc<V>) -> bool {
        match loc {
            Loc::Tile(_, a) => a,
            Loc::Leaf(n, i) => self.leaves.is_on(n, i),
        }
    }

    /// The value at `c`.
    pub fn get(&self, c: Coord) -> V {
        self.read_value(self.locate(c, &mut NoPath))
    }

    /// Whether the voxel at `c` is active.
    pub fn is_active(&self, c: Coord) -> bool {
        self.read_active(self.locate(c, &mut NoPath))
    }

    /// The value at `c` and whether it is active.
    pub fn probe(&self, c: Coord) -> (V, bool) {
        self.read(self.locate(c, &mut NoPath))
    }

    /// An accessor caching the path of its last query, for coherent reads.
    pub fn accessor(&self) -> Accessor<'_, V, L2, L1, L0> {
        Accessor::new(self)
    }

    /// An accessor caching the path of its last query, for coherent reads
    /// and writes.
    pub fn accessor_mut(&mut self) -> AccessorMut<'_, V, L2, L1, L0> {
        AccessorMut::new(self)
    }

    // --- writing --------------------------------------------------------

    #[inline]
    fn unchanged<F: Fn(V, bool) -> (V, bool)>(f: &F, v: V, a: bool) -> bool {
        let (nv, na) = f(v, a);
        na == a && nv.same(&v)
    }

    /// Applies `f` to the voxel at `c`. A tile on the way is densified only
    /// when `f` would change its state.
    pub(crate) fn modify<P: Path, F: Fn(V, bool) -> (V, bool)>(&mut self, c: Coord, f: &F, p: &mut P) {
        let k = Self::key(c, Self::UPPER_LOG2);
        let (t, a) = match self.root.get(&k).copied() {
            Some(RootSlot::Child(u)) => {
                p.upper(k, u);
                return self.modify_upper(u, c, f, p);
            }
            Some(RootSlot::Tile { value, active }) => (value, active),
            None => (self.background, false),
        };
        if Self::unchanged(f, t, a) {
            return;
        }
        let u = self.upper.alloc(t, a);
        self.root.insert(k, RootSlot::Child(u));
        p.upper(k, u);
        self.modify_upper(u, c, f, p);
    }

    pub(crate) fn modify_upper<P: Path, F: Fn(V, bool) -> (V, bool)>(
        &mut self,
        u: u32,
        c: Coord,
        f: &F,
        p: &mut P,
    ) {
        let i = Self::upper_offset(c);
        let l = if self.upper.is_child(u, i) {
            self.upper.child(u, i)
        } else {
            let (t, a) = self.upper.tile(u, i);
            if Self::unchanged(f, t, a) {
                return;
            }
            let l = self.lower.alloc(t, a);
            self.upper.set_child(u, i, l);
            l
        };
        p.lower(Self::key(c, Self::LOWER_LOG2), l);
        self.modify_lower(l, c, f, p);
    }

    pub(crate) fn modify_lower<P: Path, F: Fn(V, bool) -> (V, bool)>(
        &mut self,
        l: u32,
        c: Coord,
        f: &F,
        p: &mut P,
    ) {
        let i = Self::lower_offset(c);
        let n = if self.lower.is_child(l, i) {
            self.lower.child(l, i)
        } else {
            let (t, a) = self.lower.tile(l, i);
            if Self::unchanged(f, t, a) {
                return;
            }
            let n = self.leaves.alloc(t, a);
            self.lower.set_child(l, i, n);
            n
        };
        p.leaf(Self::key(c, L0), n);
        self.modify_leaf(n, c, f);
    }

    #[inline]
    pub(crate) fn modify_leaf<F: Fn(V, bool) -> (V, bool)>(&mut self, n: u32, c: Coord, f: &F) {
        let i = Self::leaf_offset(c);
        let (v, a) = f(self.leaves.value(n, i), self.leaves.is_on(n, i));
        self.leaves.set(n, i, v, a);
    }

    /// Writes `value` at `c` and makes the voxel active.
    pub fn set(&mut self, c: Coord, value: V) {
        self.modify(c, &move |_, _| (value, true), &mut NoPath);
    }

    /// Writes `value` at `c` and makes the voxel inactive.
    pub fn set_inactive(&mut self, c: Coord, value: V) {
        self.modify(c, &move |_, _| (value, false), &mut NoPath);
    }

    /// Changes only the active state of the voxel at `c`.
    pub fn set_active(&mut self, c: Coord, active: bool) {
        self.modify(c, &move |v, _| (v, active), &mut NoPath);
    }

    /// Removes every node: the tree reads the inactive background everywhere.
    pub fn clear(&mut self) {
        *self = Self::new(self.background);
    }

    // --- whole-tree queries ---------------------------------------------

    /// Every active value, a voxel or a tile, in tree order.
    pub fn iter_active(&self) -> ActiveIter<'_, V, L2, L1, L0> {
        ActiveIter::new(self)
    }

    /// Every active voxel and its value, tiles expanded voxel by voxel.
    ///
    /// A root tile is `2^(3 * UPPER_LOG2)` voxels: prefer
    /// [`iter_active`](Self::iter_active) where tiles can be large.
    pub fn iter_active_voxels(&self) -> impl Iterator<Item = (Coord, V)> + '_ {
        self.iter_active()
            .flat_map(|a| a.bbox().coords().map(move |c| (c, a.value)))
    }

    /// The bounding box of the active voxels, `None` when there is none.
    pub fn active_bbox(&self) -> Option<CoordBBox> {
        self.iter_active().map(|a| a.bbox()).reduce(CoordBBox::union)
    }

    /// The number of active voxels, tiles counted by their volume.
    pub fn active_voxel_count(&self) -> u64 {
        self.stats().active_voxels
    }

    /// Node counts, active counts and memory.
    pub fn stats(&self) -> Stats {
        let vol = |log2: u32| 1u64 << (3 * log2);
        let mut s = Stats {
            root_entries: self.root.len(),
            upper_nodes: self.upper.live(),
            lower_nodes: self.lower.live(),
            leaf_nodes: self.leaves.live(),
            ..Stats::default()
        };
        for slot in self.root.values() {
            match *slot {
                RootSlot::Tile { active: true, .. } => {
                    s.active_tiles += 1;
                    s.active_voxels += vol(Self::UPPER_LOG2);
                }
                RootSlot::Tile { .. } => {}
                RootSlot::Child(u) => {
                    let t = mask::count(self.upper.value_mask(u));
                    s.active_tiles += t;
                    s.active_voxels += t * vol(Self::LOWER_LOG2);
                    each_on(self.upper.child_mask(u), |i| {
                        let l = self.upper.child(u, i);
                        let t = mask::count(self.lower.value_mask(l));
                        s.active_tiles += t;
                        s.active_voxels += t * vol(L0);
                        each_on(self.lower.child_mask(l), |j| {
                            s.active_voxels += mask::count(self.leaves.mask(self.lower.child(l, j)));
                        });
                    });
                }
            }
        }
        s.heap_bytes = self.upper.heap_bytes()
            + self.lower.heap_bytes()
            + self.leaves.heap_bytes()
            + self.root.len() * (size_of::<Coord>() + size_of::<RootSlot<V>>() + 2 * size_of::<usize>());
        s
    }
}

/// Calls `f` on every bit on of `words`.
pub(crate) fn each_on(words: &[u64], mut f: impl FnMut(usize)) {
    let mut from = 0;
    while let Some(i) = mask::next_on(words, from) {
        f(i);
        from = i + 1;
    }
}

impl<V: Value, const L2: u32, const L1: u32, const L0: u32> Default for Tree<V, L2, L1, L0> {
    fn default() -> Self {
        Self::new(V::default())
    }
}

impl<V: Value, const L2: u32, const L1: u32, const L0: u32> core::fmt::Debug for Tree<V, L2, L1, L0> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = self.stats();
        f.debug_struct("Tree")
            .field("shape", &(L2, L1, L0))
            .field("background", &self.background)
            .field("root_entries", &s.root_entries)
            .field("upper_nodes", &s.upper_nodes)
            .field("lower_nodes", &s.lower_nodes)
            .field("leaf_nodes", &s.leaf_nodes)
            .finish()
    }
}
