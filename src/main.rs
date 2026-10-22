use glfw::{Action, ClientApiHint, Glfw, GlfwReceiver, Key, PWindow, WindowEvent, WindowHint, fail_on_errors};
mod renderer;
use renderer::renderer::State;
mod model;
use model::world::{World, SpawnPattern};

use std::time::Instant;
const DT: f32 = 1.0 / 120.0;
const START_BODIES: usize = 15000;
const TIME_SCALE: f32 = 0.25;
const FULLSCREEN: bool = true;
const WINDOW_TITLE: &str = "RUST_SIM";

type Events = GlfwReceiver<(f64, WindowEvent)>;

fn get_centered_window(glfw: &mut Glfw) -> (PWindow, Events) {
    // Stay hidden until positioned, so the window doesn't flash at its default spot.
    glfw.window_hint(WindowHint::Visible(false));
    let (mut window, events) = glfw
        .create_window(800, 600, WINDOW_TITLE, glfw::WindowMode::Windowed)
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
fn get_fs_window(glfw: &mut Glfw) -> (PWindow, Events) {
    glfw.with_primary_monitor(|glfw, monitor| {
        let monitor = monitor.expect("no primary monitor");
        let mode = monitor.get_video_mode().expect("no video mode");
        glfw.create_window(
            mode.width,
            mode.height,
            WINDOW_TITLE,
            glfw::WindowMode::FullScreen(monitor),
        )
    })
    .unwrap()
}

async fn run() {
    let mut glfw = glfw::init(fail_on_errors!()).unwrap();
    glfw.window_hint(WindowHint::ClientApi(ClientApiHint::NoApi));
    let (mut window, events) = if FULLSCREEN {
        get_fs_window(&mut glfw)
    } else {
        get_centered_window(&mut glfw)
    };

    let mut state = State::new(&mut window).await;

    state.window.set_framebuffer_size_polling(true);
    state.window.set_key_polling(true);
    state.window.set_mouse_button_polling(true);
    state.window.set_pos_polling(true);

    let object_count: usize = START_BODIES;
    let mut world = World::new(object_count, SpawnPattern::Disc);
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
                _ => {}
            }
        }

        match state.render(world.bodies()) {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                state.update_surface();
                state.resize(state.size);
            }
            Err(e) => eprintln!("{:?}", e),
        }
    }
}

fn main() {
    pollster::block_on(run());
}
