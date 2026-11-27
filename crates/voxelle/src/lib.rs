// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! A sparse hierarchical voxel tree.
//!
//! A [`Tree`] stores one value and one active bit for every voxel of the
//! `i32` index space, in a root map of internal nodes over leaf bricks of
//! 8³ voxels. Uniform regions are *tiles*: one value standing for a whole
//! node. See `docs/00-design.md` for the layout and the reasons behind it.
//!
//! ```
//! use voxelle::{Coord, Tree};
//!
//! let mut tree = Tree::<f32>::new(0.0);
//! tree.set(Coord::new(1, -2, 3), 0.5);
//! assert_eq!(tree.get(Coord::new(1, -2, 3)), 0.5);
//! assert!(tree.is_active(Coord::new(1, -2, 3)));
//! assert_eq!(tree.get(Coord::new(100, 0, 0)), 0.0); // the background
//!
//! let mut acc = tree.accessor();
//! assert_eq!(acc.get(Coord::new(1, -2, 3)), 0.5);
//! assert_eq!(tree.active_voxel_count(), 1);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod accessor;
mod coord;
mod iter;
mod mask;
mod node;
mod prune;
mod tree;
mod value;

pub use accessor::{Accessor, AccessorMut};
pub use coord::{Coord, CoordBBox};
pub use iter::{Active, ActiveIter};
pub use tree::{Stats, Tree};
pub use value::Value;
