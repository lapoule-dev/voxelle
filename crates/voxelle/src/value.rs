// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! The values a tree can hold.

/// A voxel value.
///
/// `same` is the equality pruning uses: two voxels collapse into one tile
/// only when their values are the same. For floats it compares bits, so a
/// prune never changes what a read returns (`NaN` is the same as itself,
/// `-0.0` is not the same as `0.0`).
pub trait Value: Copy + Default + core::fmt::Debug + Send + Sync + 'static {
    /// Whether `self` and `other` are the same value.
    fn same(&self, other: &Self) -> bool;
}

macro_rules! exact_value {
    ($($t:ty),*) => {$(
        impl Value for $t {
            #[inline]
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        }
    )*};
}

exact_value!(bool, u8, u16, u32, u64, i8, i16, i32, i64);

impl Value for f32 {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Value for f64 {
    #[inline]
    fn same(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn float_sameness_is_bitwise() {
        assert!(f32::NAN.same(&f32::NAN));
        assert!(!(-0.0f32).same(&0.0));
        assert!(1.5f64.same(&1.5));
        assert!(!f64::NAN.same(&1.0));
    }
}
