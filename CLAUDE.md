# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A 2D N-body gravity simulation in Rust, rendered with wgpu in a GLFW window. Per the README, it started as a base visualization repo for later simulations; the renderer backend closely follows [this tutorial series](https://www.youtube.com/playlist?list=PLn3eTxaOtL2PNbW4ou-APMV9W9m6nppYl). It is also run as an animated desktop wallpaper (one instance per monitor, placed behind other windows by the Hyprland plugin hyprwinwrap, which matches windows by exact title).

## Commands

```
cargo build
cargo run --release -- [title] [pattern]   # e.g. cargo run --release -- RUST_SIM solar
cargo clippy
cargo test
cargo test eccentric_orbit_keeps_its_shape   # a single test (name substring)
```

Use `--release` when judging performance: a debug build is many times slower and falls behind the fixed-step schedule.

GLFW is built from source with the `glfw` crate's `wayland` feature (bundled GLFW 3.3), which makes the binary **Wayland-only**: it won't start under plain X11. Building needs `extra-cmake-modules` (ECM), plus the `wayland`, `wayland-protocols` and `libxkbcommon` headers.

- `title` (first argument, default `RUST_SIM`) is the window title, so several instances can be told apart.
- `pattern` (second argument, default `encounter`) selects a preset in `main.rs`: `solar`, `encounter`, `galaxy`, `disc`, `random`, `grid` or `circle`. Each preset sets the `SpawnPattern`, body count, zoom (`view_scale`), `time_scale` and frame-rate cap (`max_fps`) together.

The shader is compiled into the binary (`include_str!` in `renderer.rs`), so the binary runs from any directory; editing `shaders/shader.wgsl` needs a rebuild. Escape closes the window; left click launches an extra body from the screen edge nearest the cursor towards it.

The `ms/update` line on stderr is printed every 120 steps, which for slow presets is a minute or more apart (`encounter` steps about twice a second).

## Architecture

Single binary crate, wired together in `src/main.rs`, which runs two threads:

- **Main thread** (must stay on main, since GLFW event polling isn't thread-safe): window, events, `State`, rendering, the `max_fps` frame cap.
- **Physics thread** (spawned in `run()`): owns `World` outright and steps it on its own schedule. After every step, and after every command, it publishes an `Arc<Snapshot>` (`bodies` cloned, plus `due`) into a shared `Arc<Mutex<Arc<Snapshot>>>` slot. The renderer clones the inner `Arc` under the lock and drops the lock before drawing, so neither thread ever waits on the other's work. A present that blocks (e.g. a covered window) doesn't stall physics.
- **Input goes through a channel.** Main can't touch `World`; event handlers send a `Command` over `mpsc`, and the physics thread applies it (`apply`) between steps. It waits for the next step with `recv_timeout` rather than `sleep`, so a command wakes it and is published straight away. New `Command` variants go in the enum and in `apply`.
- **Shutdown:** after the window closes, main drops `tx` then `join`s. Both `recv_timeout` and `try_recv` return `Disconnected` once every sender is gone, which is the thread's only stop signal (the order matters: joining first hangs). If the physics thread exits early (a panic), `is_finished()` closes the window.

- `model/`: simulation, with no wgpu dependency.
  - `quad_tree.rs`: `Body` (pos, prev_pos, vel, mass, density, radius derived from mass/density, color) and a Barnes-Hut `QuadTree` rebuilt from scratch every step with `build(&[Body])`, rooted on the bodies' bounding square. It owns no bodies: `nodes: Vec<Node>` is an arena (a node's four children are consecutive, found via `first_child`; `LEAF` = 0), and `order: Vec<u32>` is a permutation of body indices in which every node covers `order[start..end]`, sorted into quadrants by in-place `partition`. Nodes are aggregated bottom-up (reverse index order) into `mass`, `center_of_mass` and `max_radius`. Queries take `&self` and caller-owned scratch `Vec`s: `acceleration` (softened gravity, without `G`) and `collision_pairs` (pairs reported once as `i < j`).
  - `world.rs`: `World` owns `bodies: Vec<Body>` and the parallel `acc: Vec<Vec2>`. `update(dt)` is velocity Verlet (kick-drift-kick): half kick with last step's `acc`, drift, cull bodies beyond `CULL_RADIUS` of body 0, rebuild tree and `resolve_collisions`, `compute_accelerations` (rayon), half kick, then shift every position and velocity so body 0 sits at rest at the origin. Collisions merge (bound and deep overlap; mass, momentum and volume conserved), shatter into `FRAGMENTS` (unbound and energetic, never body 0), or bounce. Physics constants are at the top of the file.
  - `world/spawn.rs`: a child module with a second `impl World` holding every initial layout (`SpawnPattern`, `spawn_bodies`, the `create_*` functions, `kepler_state`, `set_circular_velocities`). Being a child module, it can use `World`'s private fields and constants; items `world.rs` calls are `pub(super)`.
- `renderer/`: `renderer.rs` holds `State` (surface, device, queue, two pipelines, instance buffer, projection UBO, `view_scale`). `backend/` has small builder-style wrappers (pipeline, bind group, bind group layout, UBO, IBO, mesh).

### Invariants that span files

- **`acc` lines up with `bodies`.** Forces are computed after collisions and culling, so they always match the current body list. Anything that adds a body between steps must also push to `acc` (see `add_body`).
- **Body 0 is the anchor.** The camera, culling and reframing all follow `bodies[0]`, so a pattern must push its central body first. Merges keep the lower index, so it stays body 0.
- **Interpolation.** `update` copies `pos` into `prev_pos` before the step; the renderer draws `prev_pos + (pos − prev_pos) · alpha`, with `alpha = (now − snap.due) / (DT / time_scale)`, capped at 1, so motion is smooth at any step rate. A snapshot published for a command reuses the current `due`, so it doesn't jump. `prev_pos` is deliberately *not* shifted by the reframe (that keeps drawing continuous across steps). Bodies created mid-step need a sensible `prev_pos`: `merge` mass-weights it, `Body::new` sets it to `pos`.
- **Softening must match spawn speeds.** `pull` adds `softening2` to r², so circular speeds computed at spawn must use the softened pull (`create_galaxy_inner` does), or inner orbits start too fast. Softening is per world: `SOLAR_SOFTENING2` for `Solar`, `SOFTENING2` otherwise.
- **Physics timing.** Fixed `DT` steps; step k is due at `start + k · DT / time_scale` real time, so steps per real second = `time_scale / DT`. A late thread runs steps back to back to catch up; past `MAX_LAG` (0.25 s) it drops the backlog and restarts the schedule from now, so an overloaded sim slows down rather than spiralling (e.g. `random` at 10k bodies needs 3–5 ms per step against a 3.1 ms budget until the bodies merge). Rendering never counts as lag.

### Rendering

Two instanced draws of the same instances in one pass: a glow pipeline (`vs_glow`/`fs_glow`, additive blending) then a core pipeline (`vs_core`/`fs_core`, premultiplied alpha, on a quad shrunk to the body) that draws dark discs over the glows.
- Vertex buffer slot 0 is a unit quad (`mesh_builder::make_quad`), vertices and indices in one buffer split at `mesh.offset`. Slot 1 is the instance buffer (`IBO<ParticleInstance>`), rebuilt every frame in `update_transformations` and grown with `reserve` as needed.
- Bind group 0 is the view-projection UBO (orthographic, ±`view_scale` on the short side). There are no textures.
- Instance size is `radius × GLOW_SCALE × view_scale × 0.5`, so bodies keep their on-screen size as the zoom changes. `BODY_RADIUS` in the shader must stay `2 / GLOW_SCALE`.
- The glow colour is the body's `color`, deepening to `color²` further out. Colours are additive, so keep their brightest channel around 0.6–0.85.

When adding per-particle data, keep three things in sync: `ParticleInstance` (`definitions.rs`, `#[repr(C)]` field order), its `vertex_attr_array!` locations, and the `Vertex` input struct in `shader.wgsl` (locations 2–4 are instance data).

## Quirks

- `game_object::Object`, `UBOGroup` (allocated by `build_ubos_for_objects` but unused) and `mesh_builder::make_triangle` are leftovers from the tutorial.
- The app never positions its window: Wayland doesn't allow it, GLFW reports `set_pos` as an error, and `fail_on_errors!()` turns any GLFW error into a panic. Avoid other X11-only GLFW calls for the same reason. The `Pos` event handler is kept but never fires.
- The surface uses `present_modes[0]`, whatever the driver lists first. On this machine (NVIDIA, Hyprland) a sim covered by a fullscreen window keeps rendering at its full `max_fps`; presents aren't throttled.
- Both `glm` (wildcard version) and `nalgebra-glm` are dependencies; the model uses `glm::Vec2`, the projection uses `nalgebra_glm::ortho`.
- `G = 1` and masses are chosen for it; galaxy and encounter seeds come from the clock in whole seconds and are printed to stderr, together with each galaxy's random parameters.
