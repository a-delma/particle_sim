use glfw::{Action, ClientApiHint, Glfw, GlfwReceiver, Key, PWindow, WindowEvent, WindowHint, fail_on_errors};
mod renderer;
use glm::{Vec2};
use renderer::renderer::State;
mod model;
use model::world::{World, SpawnPattern};

use std::{sync::{Arc, Mutex, mpsc::{self, RecvTimeoutError, TryRecvError}}, time::{Duration, Instant}};

use crate::model::quad_tree::Body;
const DT: f32 = 1.0 / 960.0;
/// How far physics may fall behind real time before it drops the backlog instead of catching up.
const MAX_LAG: Duration = Duration::from_millis(250);
const START_BODIES: usize = 10000;
/// Asteroid belt size for SpawnPattern::Solar.
const SOLAR_ASTEROIDS: usize = 2000;
/// Spawn pattern used when the second command-line argument is missing or unknown.
const DEFAULT_PATTERN: &str = "encounter";
const FULLSCREEN: bool = false;
/// Default window title. The first command-line argument overrides it, so several instances
/// can be told apart (hyprwinwrap matches windows by exact title).
const WINDOW_TITLE: &str = "RUST_SIM";

type Events = GlfwReceiver<(f64, WindowEvent)>;

/// Requests from the main thread, applied by the physics thread between steps.
enum Command {
    AddBody { start: Vec2, target: Vec2, speed: f32, mass: f32, density: f32, color: [f32; 3] },
}

/// Everything that differs between spawn patterns, chosen together so each one runs at a
/// sensible zoom and speed.
struct Preset {
    pattern: SpawnPattern,
    /// Body count passed to `World::new`.
    count: usize,
    /// Half the view height in world units.
    view_scale: f32,
    /// Sim time per real second.
    time_scale: f32,
    /// Frame rate cap, or None to draw at the display's refresh rate. Interpolation keeps
    /// motion smooth at any rate, and slow wallpapers look the same at a fraction of the GPU cost.
    max_fps: Option<f32>,
}

struct Snapshot {
    bodies: Vec<Body>,
    due: Instant,
}


/// The preset named by a command-line argument, or None if the name is unknown.
fn preset(name: &str) -> Option<Preset> {
    let (pattern, count, view_scale, time_scale, max_fps) = match name {
        // Mercury orbits about once every 10 minutes, moving well under a pixel per frame
        "solar" => (SpawnPattern::Solar, SOLAR_ASTEROIDS, 1.0, 1.0 / 2600.0, Some(10.0)),
        // zoomed out to fit both galaxies and the wide pass between them
        "encounter" => (SpawnPattern::Encounter, START_BODIES, 3.0, 1.0 / 512.0, Some(30.0)),
        "galaxy" => (SpawnPattern::Galaxy, START_BODIES, 1.0, 1.0 / 1024.0, Some(30.0)),
        "disc" => (SpawnPattern::Disc, START_BODIES, 1.0, 1.0 / 1024.0, Some(30.0)),
        // the original demo patterns, at the original interactive speed
        "random" => (SpawnPattern::Random, START_BODIES, 1.0, 1.0 / 3.0, None),
        "grid" => (SpawnPattern::Grid, START_BODIES, 1.0, 1.0 / 3.0, None),
        "circle" => (SpawnPattern::Circle, START_BODIES, 1.0, 1.0 / 3.0, None),
        _ => return None,
    };
    Some(Preset { pattern, count, view_scale, time_scale, max_fps })
}

// Wayland doesn't let clients place their own windows (GLFW reports `set_pos` as an error, which
// `fail_on_errors!` turns into a panic), so the compositor picks the spot.
fn get_window(glfw: &mut Glfw, title: &str) -> (PWindow, Events) {
    glfw.create_window(800, 600, title, glfw::WindowMode::Windowed)
        .unwrap()
}

// Fullscreen on the primary monitor at its current video mode, so the display
// resolution doesn't change.
fn get_fs_window(glfw: &mut Glfw, title: &str) -> (PWindow, Events) {
    glfw.with_primary_monitor(|glfw, monitor| {
        let monitor = monitor.expect("no primary monitor");
        let mode = monitor.get_video_mode().expect("no video mode");
        glfw.create_window(
            mode.width,
            mode.height,
            title,
            glfw::WindowMode::FullScreen(monitor),
        )
    })
    .unwrap()
}

async fn run() {
    let mut glfw = glfw::init(fail_on_errors!()).unwrap();
    glfw.window_hint(WindowHint::ClientApi(ClientApiHint::NoApi));
    // usage: particle_sim [title] [pattern]
    let title = std::env::args().nth(1).unwrap_or_else(|| WINDOW_TITLE.to_string());
    let name = std::env::args().nth(2).unwrap_or_else(|| DEFAULT_PATTERN.to_string());
    let preset = preset(&name).unwrap_or_else(|| {
        eprintln!(
            "unknown pattern '{name}' (expected solar, encounter, galaxy, disc, random, grid or \
             circle), using {DEFAULT_PATTERN}"
        );
        preset(DEFAULT_PATTERN).unwrap()
    });
    let time_scale = preset.time_scale;
    let frame_interval = preset.max_fps.map(|fps| Duration::from_secs_f32(1.0 / fps));
    let (mut window, events) = if FULLSCREEN {
        get_fs_window(&mut glfw, &title)
    } else {
        get_window(&mut glfw, &title)
    };

    let mut state = State::new(&mut window, preset.view_scale).await;

    state.window.set_framebuffer_size_polling(true);
    state.window.set_key_polling(true);
    state.window.set_mouse_button_polling(true);
    state.window.set_pos_polling(true);

    let world = World::new(preset.count, preset.pattern);
    let start = Instant::now();
    let shared: Arc<Mutex<Arc<Snapshot>>> = Arc::new(Mutex::new(Arc::new(Snapshot { 
            bodies: world.bodies().to_vec(), 
            due: start })));

    let step_real = Duration::from_secs_f32(DT / time_scale);

    state.build_ibo_for_object(world.bodies().len());
    state.build_ubos_for_objects(1);

    let (tx, rx) = mpsc::channel::<Command>();

    let physics_shared = Arc::clone(&shared);
    let physics = std::thread::spawn(move || {
        let mut world = world;
        let mut due = start;
        let (mut busy, mut steps) = (Duration::ZERO, 0);
        loop {
            let mut next = due + step_real;

            while let Some(wait) = next.checked_duration_since(Instant::now()) {
                match rx.recv_timeout(wait) {
                    Ok(cmd) => {
                        apply(&mut world, cmd);
                        publish(&physics_shared, &world, due);
                    }
                    Err(RecvTimeoutError::Timeout) => break,          // step is due
                    Err(RecvTimeoutError::Disconnected) => return,    // main has exited (part 4)
                }
            }

            loop {
                match rx.try_recv() {
                    Ok(cmd) => apply(&mut world, cmd),
                    Err(TryRecvError::Empty) => break,          // step is due
                    Err(TryRecvError::Disconnected) => return,    // main has exited (part 4)
                }
            }

            let now = Instant::now();
            if now - next > MAX_LAG {
                next = now;
            }

            let t = Instant::now();
            world.update(DT);
            busy += t.elapsed();
            steps += 1;
            due = next;
            publish(&physics_shared, &world, due);

            if steps >= 120 {
                eprintln!("{} bodies, {:.2} ms/update", world.bodies().len(),
                          1000.0 * busy.as_secs_f32() / steps as f32);
                (busy, steps) = (Duration::ZERO, 0);
            }
        }
    });

    let mut next_frame = Instant::now();
    while !state.window.should_close() {
        glfw.poll_events();
        for (_, event) in glfw::flush_messages(&events) {
            match event {
                //Hit escape
                glfw::WindowEvent::Key(Key::Escape, _, Action::Press, _) => {
                    state.window.set_should_close(true)
                }

                //Window was moved
                glfw::WindowEvent::Pos(..) => {
                    state.update_surface();
                    state.resize(state.size);
                }

                //Window was resized
                glfw::WindowEvent::FramebufferSize(width, height) => {
                    state.update_surface();
                    state.resize((width, height));
                }

                glfw::WindowEvent::MouseButton(glfw::MouseButtonLeft, Action::Press, _) => {
                    let (px, py) = state.window.get_cursor_pos();
                    let (w, h) = state.window.get_size();
                    let (w, h) = (w as f32, h as f32);

                    let nx = 2.0 * px as f32 / w - 1.0;
                    let ny = 1.0 - 2.0 * py as f32 / h;

                    let aspect = w / h;
                    let target = if aspect >= 1.0 {
                        Vec2::new(nx * aspect, ny)
                    } else {
                        Vec2::new(nx, ny / aspect)
                    };

                    let half = if aspect >= 1.0 {
                        Vec2::new(aspect, 1.0)
                    } else {
                        Vec2::new(1.0, 1.0 / aspect)
                    };
                    let spawn_at = offscreen_start(target, half, 0.2);

                    let _ = tx.send(Command::AddBody { 
                        start: spawn_at,   // start: off-screen, top right
                        target,                     // target: just below the center, so it curves past instead of plunging in
                        speed: 5.0,                   // speed, world units per sim second
                        mass: 0.02,                  // mass
                        density: 10000.0,               // density: radius 0.01 at this mass
                        color: [1.0, 0.9, 0.8],       // color
                    });

                }

                _ => {}
            }
        }

        let snap = Arc::clone(&shared.lock().unwrap());
        let since = Instant::now().saturating_duration_since(snap.due);
        let alpha = (since.as_secs_f32() / step_real.as_secs_f32()).min(1.0);
        match state.render(&snap.bodies, alpha) {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                state.update_surface();
                state.resize(state.size);
            }
            Err(e) => eprintln!("{:?}", e),
        }

        // Sleep off the rest of this frame's time slot. Deadlines advance by a fixed interval
        // so the rate doesn't drift; after a slow frame the schedule restarts from now instead
        // of drawing several frames back to back to catch up.
        if let Some(interval) = frame_interval {
            next_frame += interval;
            let now = Instant::now();
            if next_frame > now {
                std::thread::sleep(next_frame - now);
            } else {
                next_frame = now;
            }
        }
        if physics.is_finished() {
            state.window.set_should_close(true);
        }
    }
    drop(tx);
    physics.join().unwrap();
}

fn apply(world: &mut World, cmd: Command) {
    match cmd {
        Command::AddBody { start, target, speed, mass, density, color } =>
            world.add_body(start, target, speed, mass, density, color),
    }
}

fn publish(slot: &Mutex<Arc<Snapshot>>, world: &World, due: Instant) {
    *slot.lock().unwrap() = Arc::new(Snapshot { bodies: world.bodies().to_vec(), due });
}

fn offscreen_start(target: Vec2, half: Vec2, margin: f32) -> Vec2 {
    let left = target.x + half.x;
    let right = half.x - target.x;
    let bottom = target.y + half.y;
    let top = half.y - target.y;

    let nearest = left.min(right).min(bottom).min(top);
    if nearest == left {
        Vec2::new(-half.x - margin, target.y)
    } else if nearest == right {
        Vec2::new(half.x + margin, target.y)
    } else if nearest == bottom {
        Vec2::new(target.x, -half.y - margin)
    } else {
        Vec2::new(target.x, half.y + margin)
    }
}

fn main() {
    pollster::block_on(run());
}
