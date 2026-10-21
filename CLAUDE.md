# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A 2D particle simulation in Rust, rendered with wgpu in a GLFW window. Per the README, it is meant as a base visualization repo for later simulations; the renderer backend closely follows [this tutorial series](https://www.youtube.com/playlist?list=PLn3eTxaOtL2PNbW4ou-APMV9W9m6nppYl).

## Commands

```
cargo build
cargo run            # must be run from the repo root (see below)
cargo run --release  # smoother with many particles
cargo clippy
cargo test           # unit tests (currently only src/model/quad_tree.rs)
cargo test quad_tree::tests::collision_pairs_match_brute_force   # a single test
```

Escape closes the window.

Shaders and textures are loaded at runtime relative to the **current working directory** (`shaders/<name>` in `pipeline.rs`, `img/<name>` in `material.rs`), not relative to the binary, so launching from another directory panics on file read.

## Architecture

Single binary crate with two modules, wired together in `src/main.rs`:

- `model/`: simulation state, with no wgpu dependency.
  - `World` owns every particle (`game_object::Object`: position, velocity, size) inside a `QuadTreeContainer<Object>`. `World::update(dt)` integrates velocity and bounces particles off the `[-1, 1]` box. `SpawnPattern` selects the initial layout.
  - `quad_tree.rs`: a Barnes-Hut quad tree rebuilt from scratch every step with `build(&[Body])`. It owns no bodies: `nodes: Vec<Node>` is an arena (a node's four children are consecutive, found via `first_child`; `LEAF` = 0), and `order: Vec<u32>` is a permutation of body indices in which every node covers the contiguous range `order[start..end]`, sorted into quadrants by in-place `partition`. After subdividing, nodes are aggregated bottom-up (reverse index order, since children are always pushed after parents) into `mass`, `center_of_mass` and `max_radius`. Queries take `&self`: `acceleration` (gravity, without `G`; θ test uses `bounds.size`) and `collision_pairs` (pruned by distance to the square vs `radius + max_radius`, pairs reported once as `i < j`). Both take caller-owned scratch `Vec`s so nothing is allocated per call.
- `renderer/`: `renderer.rs` holds `State` (surface, device, queue, pipeline). `backend/` contains small builder-style wrappers (pipeline, bind group, bind group layout, UBO, IBO, material, mesh).

Frame loop (`main.rs`): `world.update(fixed dt)` → poll GLFW events → `state.render(world.get_items())`. Window move or resize recreates the surface (`update_surface` + `resize`).

Rendering is a single instanced draw:
- Vertex buffer slot 0 is a unit quad (`mesh_builder::make_quad`), with vertices and indices in one buffer split at `mesh.offset`.
- Slot 1 is the instance buffer (`IBO<ParticleInstance>`), rebuilt from the world every frame in `update_transformations`.
- Bind group 0 is the texture material, bind group 1 is the view-projection matrix UBO.
- The glow look comes entirely from the fragment shader in `shaders/shader.wgsl`, based on distance from the quad center.

When adding per-particle data, keep three things in sync: `ParticleInstance` (`definitions.rs`, `#[repr(C)]` field order), its `vertex_attr_array!` locations, and the `Vertex` input struct in `shader.wgsl` (locations 2–4 are instance data).

The instance buffer is sized once with `state.build_ibo_for_object(count)`, which must match the world's particle count.

## Current state and quirks

- The quad tree's root area is `(0,0)` to `(10,10)`, while particles live in `[-1, 1]`. `World::update` mutates positions without calling `relocate`, so the tree's spatial data goes stale after spawn. Spatial queries are not used yet.
- `UBOGroup` (per-object UBOs) is allocated but unused; particles go through the instance buffer instead.
- The particle size and color are hardcoded in `update_transformations`, ignoring `Object::size`.
- Both `glm` (wildcard version) and `nalgebra-glm` are dependencies, and the code uses both.
