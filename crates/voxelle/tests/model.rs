// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) lapoule.dev

//! Property tests: the tree against a trivially correct model, a `BTreeMap`
//! of every voxel ever written.
//!
//! Every sequence of writes (on the tree and through a long-lived mutable
//! accessor), fills of whole blocks, prunes and inactive prunes must leave
//! `get`, `is_active`, both accessors, the active iteration, the active
//! count and the active bounding box exactly as the model says. Two shapes:
//! the default 5-4-3 (coordinates around the node boundaries and the ends of
//! the `i32` range) and a tiny 2-2-2 (dense coordinates, so fills collapse
//! leaves and lower nodes, and the root map holds many entries).

use std::collections::BTreeMap;

use proptest::prelude::*;
use voxelle::{Coord, CoordBBox, Tree};

const BG: f32 = 0.0;

#[derive(Clone, Debug)]
enum Op {
    Set(Coord, f32, bool),
    SetInactive(Coord, f32, bool),
    SetActive(Coord, bool, bool),
    Fill {
        at: Coord,
        log2: u32,
        value: f32,
        active: bool,
    },
    Prune,
    PruneInactive,
}

impl Op {
    /// Whether the op goes through the mutable accessor.
    fn via_accessor(&self) -> bool {
        matches!(
            self,
            Op::Set(.., true) | Op::SetInactive(.., true) | Op::SetActive(.., true)
        )
    }
}

#[derive(Default)]
struct Model {
    voxels: BTreeMap<Coord, (f32, bool)>,
}

impl Model {
    fn probe(&self, c: Coord) -> (f32, bool) {
        self.voxels.get(&c).copied().unwrap_or((BG, false))
    }

    fn write(&mut self, c: Coord, f: impl Fn((f32, bool)) -> (f32, bool)) {
        let next = f(self.probe(c));
        self.voxels.insert(c, next);
    }

    fn active(&self) -> BTreeMap<Coord, u32> {
        self.voxels
            .iter()
            .filter(|(_, (_, a))| *a)
            .map(|(c, (v, _))| (*c, v.to_bits()))
            .collect()
    }
}

fn block(at: Coord, log2: u32) -> CoordBBox {
    let d = (1i32 << log2) - 1;
    let o = Coord::new(at.x & !d, at.y & !d, at.z & !d);
    CoordBBox::new(o, Coord::new(o.x + d, o.y + d, o.z + d))
}

fn same(a: (f32, bool), b: (f32, bool)) -> bool {
    a.0.to_bits() == b.0.to_bits() && a.1 == b.1
}

fn run<const L2: u32, const L1: u32, const L0: u32>(
    ops: &[Op],
    probes: &[Coord],
) -> Result<(), TestCaseError> {
    let mut tree = Tree::<f32, L2, L1, L0>::new(BG);
    let mut model = Model::default();
    let mut k = 0;
    while k < ops.len() {
        if ops[k].via_accessor() {
            // One accessor across a run of writes: its cache must follow
            // the nodes its own writes create.
            let mut acc = tree.accessor_mut();
            while k < ops.len() && ops[k].via_accessor() {
                let c = match ops[k] {
                    Op::Set(c, v, _) => {
                        acc.set(c, v);
                        model.write(c, |_| (v, true));
                        c
                    }
                    Op::SetInactive(c, v, _) => {
                        acc.set_inactive(c, v);
                        model.write(c, |_| (v, false));
                        c
                    }
                    Op::SetActive(c, a, _) => {
                        acc.set_active(c, a);
                        model.write(c, |(v, _)| (v, a));
                        c
                    }
                    _ => unreachable!(),
                };
                prop_assert!(same(acc.probe(c), model.probe(c)), "accessor read-back at {c:?}");
                k += 1;
            }
            continue;
        }
        match ops[k] {
            Op::Set(c, v, _) => {
                tree.set(c, v);
                model.write(c, |_| (v, true));
            }
            Op::SetInactive(c, v, _) => {
                tree.set_inactive(c, v);
                model.write(c, |_| (v, false));
            }
            Op::SetActive(c, a, _) => {
                tree.set_active(c, a);
                model.write(c, |(v, _)| (v, a));
            }
            Op::Fill {
                at,
                log2,
                value,
                active,
            } => {
                for c in block(at, log2).coords() {
                    if active {
                        tree.set(c, value);
                    } else {
                        tree.set_inactive(c, value);
                    }
                    model.write(c, |_| (value, active));
                }
            }
            Op::Prune => tree.prune(),
            Op::PruneInactive => {
                tree.prune_inactive();
                // An inactive voxel of a removed node now reads the
                // background; any other inactive voxel keeps its value.
                let inactive: Vec<_> = model
                    .voxels
                    .iter()
                    .filter(|(_, (_, a))| !a)
                    .map(|(c, s)| (*c, *s))
                    .collect();
                for (c, s) in inactive {
                    let t = tree.probe(c);
                    prop_assert!(
                        same(t, s) || same(t, (BG, false)),
                        "prune_inactive at {c:?}: {t:?}, model {s:?}"
                    );
                    model.voxels.insert(c, t);
                }
            }
        }
        k += 1;
    }

    // Point queries, on the tree and through a read accessor, over every
    // voxel written and the extra probes.
    let mut acc = tree.accessor();
    let points: Vec<Coord> = model
        .voxels
        .keys()
        .copied()
        .chain(probes.iter().copied())
        .collect();
    for &c in &points {
        let m = model.probe(c);
        prop_assert!(
            same(tree.probe(c), m),
            "probe at {c:?}: {:?}, model {m:?}",
            tree.probe(c)
        );
        prop_assert_eq!(tree.get(c).to_bits(), m.0.to_bits());
        prop_assert_eq!(tree.is_active(c), m.1);
        prop_assert!(same(acc.probe(c), m), "accessor at {c:?}");
    }
    // In reverse, so the accessor misses its cache in a different order.
    for &c in points.iter().rev() {
        prop_assert_eq!(acc.get(c).to_bits(), model.probe(c).0.to_bits());
        prop_assert_eq!(acc.is_active(c), model.probe(c).1);
    }

    // Iteration: every active voxel exactly once, with its value.
    let expected = model.active();
    let listed: Vec<(Coord, u32)> = tree.iter_active_voxels().map(|(c, v)| (c, v.to_bits())).collect();
    let as_map: BTreeMap<Coord, u32> = listed.iter().copied().collect();
    prop_assert_eq!(listed.len(), as_map.len(), "a voxel was listed twice");
    prop_assert_eq!(&as_map, &expected);
    prop_assert_eq!(tree.active_voxel_count(), expected.len() as u64);
    let bbox = expected
        .keys()
        .map(|&c| CoordBBox::voxel(c))
        .reduce(CoordBBox::union);
    prop_assert_eq!(tree.active_bbox(), bbox);
    Ok(())
}

fn value() -> impl Strategy<Value = f32> {
    prop::sample::select(vec![0.0f32, -0.0, 1.0, 2.5, f32::NAN])
}

/// Coordinates near the 5-4-3 node boundaries and the ends of `i32`.
fn wide_coord() -> impl Strategy<Value = Coord> {
    let anchor = prop::sample::select(vec![
        0i32,
        8,
        -8,
        128,
        -128,
        4096,
        -4096,
        1 << 20,
        i32::MAX - 10,
        i32::MIN + 10,
    ]);
    let axis = (anchor, -10i32..=10).prop_map(|(a, d)| a + d);
    (axis.clone(), axis.clone(), axis).prop_map(|(x, y, z)| Coord::new(x, y, z))
}

/// Dense coordinates for the 2-2-2 shape: across several 64³ root regions.
fn tiny_coord() -> impl Strategy<Value = Coord> {
    (-70i32..70, -70i32..70, -70i32..70).prop_map(|(x, y, z)| Coord::new(x, y, z))
}

fn ops(coord: BoxedStrategy<Coord>, fill_log2: Vec<u32>) -> impl Strategy<Value = Vec<Op>> {
    let op = prop_oneof![
        6 => (coord.clone(), value(), any::<bool>()).prop_map(|(c, v, a)| Op::Set(c, v, a)),
        2 => (coord.clone(), value(), any::<bool>()).prop_map(|(c, v, a)| Op::SetInactive(c, v, a)),
        2 => (coord.clone(), any::<bool>(), any::<bool>()).prop_map(|(c, s, a)| Op::SetActive(c, s, a)),
        1 => (coord, prop::sample::select(fill_log2), value(), any::<bool>())
            .prop_map(|(at, log2, value, active)| Op::Fill { at, log2, value, active }),
        1 => Just(Op::Prune),
        1 => Just(Op::PruneInactive),
    ];
    prop::collection::vec(op, 1..60)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn default_shape_matches_the_model(
        ops in ops(wide_coord().boxed(), vec![0, 3]),
        probes in prop::collection::vec(wide_coord(), 0..20),
    ) {
        run::<5, 4, 3>(&ops, &probes)?;
    }

    #[test]
    fn tiny_shape_matches_the_model(
        ops in ops(tiny_coord().boxed(), vec![0, 2, 4]),
        probes in prop::collection::vec(tiny_coord(), 0..20),
    ) {
        run::<2, 2, 2>(&ops, &probes)?;
    }
}
