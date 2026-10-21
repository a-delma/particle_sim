use glfw::{Action, ClientApiHint, Key, WindowHint, fail_on_errors};
mod renderer;
use renderer::renderer::State;
mod model;
use model::world::{World, SpawnPattern};

use std::time::Instant;
const DT: f32 = 1.0 / 60.0;

async fn run() {
    let mut glfw = glfw::init(fail_on_errors!()).unwrap();
    glfw.window_hint(WindowHint::ClientApi(ClientApiHint::NoApi));
    let (mut window, events) = glfw
        .create_window(800, 600, "It's WGPU time.", glfw::WindowMode::Windowed)
        .unwrap();

    let mut state = State::new(&mut window).await;

    state.window.set_framebuffer_size_polling(true);
    state.window.set_key_polling(true);
    state.window.set_mouse_button_polling(true);
    state.window.set_pos_polling(true);

    let object_count: usize = 1000;
    let mut world = World::new(object_count, SpawnPattern::Disc);
    state.build_ibo_for_object(world.bodies().len());
    state.build_ubos_for_objects(1);

    let mut last = Instant::now();
    let mut accumulator = 0.0;

    while !state.window.should_close() {
        let now = Instant::now();
        accumulator += (now - last).as_secs_f32().min(0.25);
        last = now;
        while accumulator >= DT {
            world.update(DT);
            accumulator -= DT;
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
