// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! Integer voxel coordinates and boxes of them.

/// The integer coordinates of a voxel in index space.
///
/// Ordered x first, then y, then z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Coord {
    /// The x index.
    pub x: i32,
    /// The y index.
    pub y: i32,
    /// The z index.
    pub z: i32,
}

impl Coord {
    /// The coordinates `(x, y, z)`.
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Each component bitwise-and `mask`.
    #[inline]
    pub(crate) const fn masked(self, mask: i32) -> Self {
        Self::new(self.x & mask, self.y & mask, self.z & mask)
    }

    /// Componentwise minimum.
    pub fn min(self, o: Self) -> Self {
        Self::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }

    /// Componentwise maximum.
    pub fn max(self, o: Self) -> Self {
        Self::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
}

impl From<[i32; 3]> for Coord {
    fn from([x, y, z]: [i32; 3]) -> Self {
        Self::new(x, y, z)
    }
}

/// An axis-aligned box of voxels, both corners included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CoordBBox {
    /// The smallest corner, included.
    pub min: Coord,
    /// The largest corner, included.
    pub max: Coord,
}

impl CoordBBox {
    /// The box from `min` to `max`, both included.
    pub const fn new(min: Coord, max: Coord) -> Self {
        Self { min, max }
    }

    /// The box of the single voxel `c`.
    pub const fn voxel(c: Coord) -> Self {
        Self { min: c, max: c }
    }

    /// The smallest box holding both.
    pub fn union(self, o: Self) -> Self {
        Self::new(self.min.min(o.min), self.max.max(o.max))
    }

    /// Whether `c` is inside the box.
    pub fn contains(&self, c: Coord) -> bool {
        self.min.x <= c.x
            && c.x <= self.max.x
            && self.min.y <= c.y
            && c.y <= self.max.y
            && self.min.z <= c.z
            && c.z <= self.max.z
    }

    /// The number of voxels in the box (0 when `min > max` on any axis).
    pub fn volume(&self) -> u64 {
        let d = |a: i32, b: i32| (i64::from(b) - i64::from(a) + 1).max(0) as u64;
        d(self.min.x, self.max.x) * d(self.min.y, self.max.y) * d(self.min.z, self.max.z)
    }

    /// Every voxel of the box, x outermost, z fastest.
    pub fn coords(&self) -> impl Iterator<Item = Coord> + use<> {
        let (min, max) = (self.min, self.max);
        (min.x..=max.x).flat_map(move |x| {
            (min.y..=max.y).flat_map(move |y| (min.z..=max.z).map(move |z| Coord::new(x, y, z)))
        })
    }
}
