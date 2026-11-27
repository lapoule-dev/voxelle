# voxelle

**A sparse, tiled, time-aware volumetric grid in Rust. Not a triangle mesh.**

voxelle is a core for large volumetric data (clouds, fog, smoke, densities,
winds) that must be read in real time and in offline film from the same
representation:

- sparse storage in tiles, with progressive levels of detail;
- time as a native dimension, with immutable versions;
- a local frame per tile (f64 origin, f32 only after rebasing);
- a compact layout readable from a GPU shader (WGSL);
- adapters for its inputs (procedural fields, weather models, OpenVDB files,
  3D Tiles voxels) — no input is the core.

The core is render-agnostic, has no GPU, async-runtime or engine dependency,
and compiles to `wasm32-unknown-unknown`.

Status: the sparse core (`crates/voxelle`): a root map over two levels of
internal nodes and 8³ leaf bricks, accessors with a path cache, iteration
over active values, prune. See [`docs/00-design.md`](docs/00-design.md).
Tiles, time, levels of detail and the GPU layout are not there yet.

## Tests

Tests run on a remote BuildKit builder, not on the development machine:

```bash
VOXELLE_BUILDER=<buildx builder> scripts/test-on-builder.sh
```

CI (`.github/workflows/ci.yml`) runs the same steps: fmt, tests, clippy and
the `wasm32-unknown-unknown` check of the core.

## License

Dual-licensed under MIT OR Apache-2.0, at your option.
Copyright lapoule.dev.
