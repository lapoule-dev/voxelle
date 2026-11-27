<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
<!-- Copyright (c) lapoule.dev -->

# voxelle core: design

The `voxelle` crate is the sparse core: a tree of voxel values over the whole
`i32` index space, a random-access accessor, iteration over active values,
and topology operations (prune, clear). It has no GPU, no time dimension, no
level of detail and no world transform; this page says what each of those
will need from it.

## Tree shape

Three node levels under a root, the shape proven by sparse volume libraries
over a decade of production use:

| level | children per axis | edge in voxels | slots |
|---|---|---|---|
| root | unbounded (ordered map) | — | — |
| upper internal | 2^5 = 32 | 4096 | 32 768 |
| lower internal | 2^4 = 16 | 128 | 4 096 |
| leaf | 2^3 = 8 | 8 | 512 voxels |

- The **root** is a `BTreeMap<Coord, slot>` keyed by the origin of each
  4096³ region. Ordered rather than hashed: deterministic iteration (tests,
  serialization) and no hasher choice in a wasm build. With an accessor the
  root is almost never consulted, so its `O(log n)` is off the hot path.
- Every **internal slot** is either a child or a *tile*: one value and one
  active bit standing for the whole child region. A uniform region costs one
  slot, not a node.
- A **leaf** holds 512 values and a 512-bit active mask (8 words of `u64`).
- The branching factors are **const generics** (`Tree<V, L2 = 5, L1 = 4,
  L0 = 3>`): the default is the 5-4-3 shape (the shape of files written by
  other tools, which the reader maps one to one), and tests use a tiny 2-2-2
  shape to drive every level, the root included, with a few thousand writes.
  A compile-time assertion keeps each factor ≥ 2 (whole mask words) and the
  total ≤ 30 bits.
- Linear order inside a node is x-major, z fastest
  (`offset = x·n² + y·n + z`), the order of the file format, so leaf buffers
  are copied without reordering.

## Memory layout: arenas of indices, not pointers

Each level is **one arena, a struct of arrays**: all leaves' masks in one
`Vec<u64>`, all leaves' values in one `Vec<V>`, node `n` owning the stride
`n·SIZE .. (n+1)·SIZE`; internal levels likewise (child mask, value mask,
tile values, child indices as `u32`). A child is a `u32` index into the arena
below, never a pointer.

Why:

- **No `unsafe`.** The crate is `#![forbid(unsafe_code)]`. An accessor that
  caches pointers into a mutable pointer tree needs raw pointers and an
  aliasing argument; caching `(key, index)` pairs needs neither.
- **The relocatable, pointer-free form is already there.** Flat arrays with
  `u32` offsets are what a GPU buffer or a memory-mapped file wants (the idea
  of the read-only GPU variant of the reference library). A later
  `compact()` — drop free slots, order nodes breadth-first — turns these
  arenas into one upload without changing the tree's logic.
- **Cheap clone.** `Tree::clone` is a handful of `memcpy`s of `Copy` data.
- Child slots are 4 bytes instead of 8: an upper node is 32 768 × (4 + 4)
  bytes for `f32`, the size of the pointer-union layout, without the union.

Freed nodes (prune) go to a per-level free list and are reused by the next
allocation; their stale contents are overwritten on reuse.

`Arc` subtrees for structural sharing were considered and **not** taken: a
copy-on-write check (`Arc::make_mut`) on every write descent costs the
insertion path, and sharing belongs to the future versioning layer, which can
share at the granularity it needs (chunks of an arena) once it is measured.

## Values

`V: Value` — `Copy + Default + Debug + Send + Sync`, with `same()`: equality
used by prune. For floats it compares **bits**, so prune is lossless
(`NaN` leaves collapse, `-0.0` never merges with `0.0`). Implemented for
`f32`, `f64`, `bool` and the integers. Tolerance-based pruning (lossy) is not
in this version.

## Access and mutation

- `get`, `is_active`, `probe` (both) on the tree walk root → upper → lower →
  leaf. Every step is masks and shifts on the coordinate.
- `Accessor` (on `&Tree`, many at once) and `AccessorMut` (on `&mut Tree`)
  cache the `(origin, index)` of the last leaf, lower and upper node visited.
  A query first compares the coordinate's leaf origin with the cached one
  (three integer compares), then the lower, then the upper, then the root:
  coherent queries never touch the map.
- The cache is valid because nodes are only freed by `prune`,
  `prune_inactive` and `clear`, which take `&mut Tree` — impossible while an
  accessor borrows the tree.
- Every write is one function `(value, active) → (value, active)` applied at
  a voxel. Descending into a tile, the function is first tried on the tile's
  own state: if nothing would change (setting an active tile's value to
  itself, deactivating the background), **nothing is allocated**. Otherwise
  the tile is densified into a child filled with its state, and the descent
  goes on.
- `set` writes and activates, `set_inactive` writes and deactivates,
  `set_active` changes only the state.

## Topology operations

- `prune()`: bottom-up, a leaf or internal node whose voxels all share one
  value (bitwise) and one state becomes a tile in its parent; root tiles equal
  to the inactive background are removed. **Lossless**: every `get` and
  `is_active` answer is unchanged.
- `prune_inactive()`: a node holding no active voxel becomes an inactive
  background tile. Active voxels are untouched; inactive values inside a
  removed node now read as the background.
- `clear()`: back to the empty tree.

## Iteration

`iter_active()` yields active items in tree order: a voxel, or a tile with its
extent (`log2_dim`), so a 128³ active tile is one item.
`iter_active_voxels()` expands tiles voxel by voxel. Masks are scanned a word
at a time with `trailing_zeros`, with a fixed-depth cursor (no allocation, no
recursion). `active_voxel_count()` counts by popcount and tile volumes;
`active_bbox()` is the bounding box of the active voxels.

## What later layers need from this core

- **Tiles with a local frame**: a tile owns a `Tree` and an `f64` origin; the
  core stays in integer index space. Needs: an empty tree that allocates
  nothing (true), `Send + Sync` (true), no global state (true).
- **Time and immutable versions**: needs cheap snapshots. Today a snapshot is
  a clone of flat arrays; structural sharing would chunk the arenas behind
  `Arc` without touching the node logic, since nodes are addressed by index.
- **Level of detail**: per-node statistics (min, max, average) as an optional
  arena parallel to each internal level, filled by a reduction pass — not paid
  by the access path.
- **GPU**: `compact()` into one linear buffer (above), read by a shader with
  the same offset arithmetic.

## Compared with the reference libraries

| | OpenVDB core | NanoVDB | voxelle (this version) |
|---|---|---|---|
| shape | root map + 5-4-3, configurable | the same, fixed per build | root map + 5-4-3, const generics |
| child link | pointer in a union with the tile value | byte offset in one buffer | `u32` index into a level arena |
| mutable | yes | no (read-only, built once) | yes |
| accessor | cached node pointers | cached node pointers | cached `(origin, index)` |
| value types | many, incl. vectors, bool bit-packed, point data | many | scalars (`f32`…), `bool` not bit-packed |
| prune | with tolerance | — | exact only |
| threading | TBB-parallel tools | GPU | single-threaded (`Send + Sync`) |
| out-of-core, delayed loading | yes | no | no |
| tools (level sets, filters, CSG, meshing) | yes | some | no |
| world transform, metadata | in the grid | in the grid | not in the core (tile layer) |
| file I/O | yes | own format | `.vdb` reader in `voxelle-vdb` (next step) |
| GPU, time, LOD | no | GPU only | not yet; designed for (above) |
