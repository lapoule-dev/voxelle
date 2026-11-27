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

Status: design phase, nothing implemented yet.

## License

Dual-licensed under MIT OR Apache-2.0, at your option.
Copyright lapoule.dev.
