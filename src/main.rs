use glfw::{Action, ClientApiHint, Key, WindowHint, fail_on_errors};
mod renderer;
use renderer::renderer::State;
mod model;
use model::world::{World, SpawnPattern};

use model::quad_tree::{QuadTreeContainer, Rect};

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
    let mut world = World::new(object_count, SpawnPattern::Random);
    state.build_ibo_for_object(object_count);
    state.build_ubos_for_objects(1);

    while !state.window.should_close() {
        world.update(0.1667);
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

        match state.render(&world.quads) {
            Ok(_) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                state.update_surface();
                state.resize(state.size);
            }
            Err(e) => eprintln!("{:?}", e),
        }
    }
}

use glm::Vec2;

fn main() {
    // pollster::block_on(run());
    let world = Rect {
        pos: Vec2::new(0.0, 0.0),
        size: Vec2::new(10.0, 10.0),
    };

    let mut world_tree: QuadTreeContainer<usize> = QuadTreeContainer::new(world, 3);

    let item1 = 1;
    let area1 = Rect {
        pos: Vec2::new(2.0, 2.0),
        size: Vec2::new(1.0, 1.0),
    };
 
    let item2 = 2;
    let area2 = Rect {
        pos: Vec2::new(6.0, 6.0),
        size: Vec2::new(1.0, 1.0),
    };
    
    world_tree.insert(item1, area1);
    world_tree.insert(item2, area2);
    //
    // // let mut item_list: Vec<&usize> = Vec::new();
    // // world_tree.list_items(&mut item_list);
    // //
    // // for i in &item_list {
    // //     println!("{}", i);
    // // }
    //
    // // let mut search_list: Vec<&usize> = Vec::new();
    let search_area = Rect {
        pos: Vec2::new(5.0, 5.0),
        size: Vec2::new(2.0, 2.0),
    };
    let search_list = world_tree.search(&search_area);

    for i in &search_list {
        println!("{}", i);
    }

    // println!("{}", world_tree.size())
}
