use glfw::{Action, ClientApiHint, Glfw, GlfwReceiver, Key, PWindow, WindowEvent, WindowHint, fail_on_errors};
mod renderer;
use glm::{Vec2};
use renderer::renderer::State;
mod model;
use model::world::{World, SpawnPattern};

use std::time::Instant;
const DT: f32 = 1.0 / 960.0;
const START_BODIES: usize = 5000;
/// Asteroid belt size for SpawnPattern::Solar.
const SOLAR_ASTEROIDS: usize = 2000;
const TIME_SCALE: f32 = 1.0 / 1024.0;
// const TIME_SCALE: f32 = 1.0 / 4.0;
const FULLSCREEN: bool = false;
/// Default window title. The first command-line argument overrides it, so several instances
/// can be told apart (hyprwinwrap matches windows by exact title).
const WINDOW_TITLE: &str = "RUST_SIM";

type Events = GlfwReceiver<(f64, WindowEvent)>;

fn get_centered_window(glfw: &mut Glfw, title: &str) -> (PWindow, Events) {
    // Stay hidden until positioned, so the window doesn't flash at its default spot.
    glfw.window_hint(WindowHint::Visible(false));
    let (mut window, events) = glfw
        .create_window(800, 600, title, glfw::WindowMode::Windowed)
        .unwrap();

    let (win_w, win_h) = window.get_size();
    let center = glfw.with_primary_monitor(|_, monitor| {
        let monitor = monitor?;
        let (mon_x, mon_y) = monitor.get_pos();
        let mode = monitor.get_video_mode()?;
        Some((
            mon_x + (mode.width as i32 - win_w) / 2,
            mon_y + (mode.height as i32 - win_h) / 2,
        ))
    });
    if let Some((x, y)) = center {
        window.set_pos(x, y);
    }
    window.show();

    (window, events)
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
    let title = std::env::args().nth(1).unwrap_or_else(|| WINDOW_TITLE.to_string());
    let (mut window, events) = if FULLSCREEN {
        get_fs_window(&mut glfw, &title)
    } else {
        get_centered_window(&mut glfw, &title)
    };

    let mut state = State::new(&mut window).await;

    state.window.set_framebuffer_size_polling(true);
    state.window.set_key_polling(true);
    state.window.set_mouse_button_polling(true);
    state.window.set_pos_polling(true);

    // let mut world = World::new(SOLAR_ASTEROIDS, SpawnPattern::Solar);
    let mut world = World::new(START_BODIES, SpawnPattern::Encounter);
    state.build_ibo_for_object(world.bodies().len());
    state.build_ubos_for_objects(1);

    let mut last = Instant::now();
    let mut accumulator = 0.0;
    let mut sim_time = 0.0;
    let mut sim_steps = 0;

    while !state.window.should_close() {
        let now = Instant::now();
        accumulator += (now - last).as_secs_f32().min(0.25) * TIME_SCALE;
        last = now;
        while accumulator >= DT {
            let t = Instant::now();
            world.update(DT);
            sim_time += t.elapsed().as_secs_f32();
            sim_steps += 1;
            accumulator -= DT;
        }
        let alpha = accumulator / DT;
        if sim_steps >= 120 {
            eprintln!("{} bodies, {:.2} ms/update", world.bodies().len(), 1000.0 * sim_time / sim_steps as f32);
            (sim_time, sim_steps) = (0.0, 0);
        }
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
                    let start = offscreen_start(target, half, 0.2);
                    
                    world.add_body(
                        start,   // start: off-screen, top right
                        target,  // target: just below the center, so it curves past instead of plunging in
                        5.0,                   // speed, world units per sim second
                        0.02,                  // mass
                        10000.0,               // density: radius 0.01 at this mass
                        [1.0, 0.9, 0.8],       // color
                    );

                }

                glfw::WindowEvent::Key(Key::Space, _, Action::Press, _) => {
                    world.add_body(
                        Vec2::new(2.2, 1.4),   // start: off-screen, top right
                        Vec2::new(0.0, -0.4),  // target: just below the center, so it curves past instead of plunging in
                        1.5,                   // speed, world units per sim second
                        0.02,                  // mass
                        20000.0,               // density: radius 0.01 at this mass
                        [1.0, 0.9, 0.8],       // color
                    );
                }
                _ => {}
            }
        }

        match state.render(world.bodies(), alpha) {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                state.update_surface();
                state.resize(state.size);
            }
            Err(e) => eprintln!("{:?}", e),
        }
    }
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
