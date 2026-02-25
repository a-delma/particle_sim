use crate::model::game_object::Object;

use glm::{Vec2, Vec3};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

pub enum SpawnPattern {
    Random,
    Grid,
    Circle,
}

pub struct World {
    pub quads: Vec<Object>,
}

impl World {
    pub fn new(count: usize, pattern: SpawnPattern) -> Self {
        let quads = match pattern {
            SpawnPattern::Random => Self::create_random_objects(count),
            SpawnPattern::Circle => Self::create_circle_objects(count, 0.5),
            SpawnPattern::Grid => Self::create_grid_objects(count, 0.5),
        };

        World { quads: quads }
    }

    fn create_random_objects(count: usize) -> Vec<Object> {
        let mut rng = StdRng::seed_from_u64(42);
        let mut objects = Vec::with_capacity(count);

        for _ in 0..count {
            objects.push(Object {
                position: Vec3::new(
                    rng.random_range(-1.0..1.0),
                    rng.random_range(-1.0..1.0),
                    0.0,
                ),
                velocity: Vec2::new(0.00, 0.0),
            });
        }

        objects
    }

    fn create_circle_objects(count: usize, radius: f32) -> Vec<Object> {
        let mut objects = Vec::with_capacity(count);

        for i in 0..count {
            let angle = (i as f32 / count as f32) * std::f32::consts::TAU;

            objects.push(Object {
                position: Vec3::new(radius * angle.cos(), radius * angle.sin(), 0.0),
                velocity: Vec2::new(angle.cos(), angle.sin()),
            });
        }

        objects
    }

    fn create_grid_objects(count: usize, spacing: f32) -> Vec<Object> {
        let width = (count as f64).sqrt() as usize;
        let height = (count as f64).sqrt() as usize;

        let mut objects = Vec::with_capacity(width * height);

        for y in 0..height {
            for x in 0..width {
                objects.push(Object {
                    position: Vec3::new(x as f32 * spacing, y as f32 * spacing, 0.0),
                    velocity: Vec2::new(0.0, 0.0),
                });
            }
        }

        objects
    }

    pub fn update(&mut self, dt: f32) {
        for object in &mut self.quads {
            object.position.x += object.velocity.x * dt;
            object.position.y += object.velocity.y * dt;
        }
    }
}
