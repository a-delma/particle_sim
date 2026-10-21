use crate::model::quad_tree::{Body, QuadTree};

use glm::{Vec2};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// Gravitational constant. The disc's masses are chosen for G = 1.
pub const G: f32 = 1.0;

const THETA: f32 = 0.5;
const SOFTENING2: f32 = 1e-3;

pub enum SpawnPattern {
    Random,
    Grid,
    Circle,
    /// A heavy central body with the rest orbiting it in a disc.
    Disc,
}

pub struct World {
    bodies: Vec<Body>,
    tree: QuadTree,

    stack: Vec<u32>,
    acc: Vec<Vec2>,
    pairs: Vec<(u32, u32)>
}

impl World {
    pub fn new(count: usize, pattern: SpawnPattern) -> Self {
        let quads = match pattern {
            SpawnPattern::Random => Self::create_random_objects(count),
            SpawnPattern::Circle => Self::create_circle_objects(count, 0.5),
            SpawnPattern::Grid => Self::create_grid_objects(count, 0.5),
            SpawnPattern::Disc => Self::create_disc_objects(count),
        };
 
        let world_tree = QuadTree::new(8, 16);
        World { bodies: quads, tree: world_tree, stack: Vec::new(), acc: Vec::new(), pairs: Vec::new() }
 
    }

    fn create_random_objects(count: usize) -> Vec<Body> {
        let mut rng = StdRng::seed_from_u64(42);
        let mut objects = Vec::with_capacity(count);

        for _ in 0..count {
            objects.push(Body {
                mass: 1.0,
                pos: Vec2::new(
                    rng.random_range(-1.0..1.0),
                    rng.random_range(-1.0..1.0),
                ),
                radius : 0.005,
                vel: Vec2::new(0.50, -0.25),
            });
        }

        objects
    }

    fn create_circle_objects(count: usize, radius: f32) -> Vec<Body> {
        let mut objects = Vec::with_capacity(count);

        for i in 0..count {
            let angle = (i as f32 / count as f32) * std::f32::consts::TAU;

            objects.push(Body {
                mass: 1.0,
                pos: Vec2::new(radius * angle.cos(), radius * angle.sin()),
                radius: 0.005,
                vel: Vec2::new(angle.cos(), angle.sin()),
            });
        }

        objects
    }

    fn create_grid_objects(count: usize, spacing: f32) -> Vec<Body> {
        let width = (count as f64).sqrt() as usize;
        let height = (count as f64).sqrt() as usize;

        let mut objects = Vec::with_capacity(width * height);

        for y in 0..height {
            for x in 0..width {
                objects.push(Body {
                    mass: 1.0,
                    pos: Vec2::new(x as f32 * spacing, y as f32 * spacing),
                    radius: 0.005,
                    vel: Vec2::new(0.0, 0.0),
                });
            }
        }
        objects
    }

    /// `count` light bodies in a disc around one heavy central body, so `count + 1`
    /// bodies in total. Each starts at the circular-orbit speed for the mass inside
    /// its radius, moving counter-clockwise.
    fn create_disc_objects(count: usize) -> Vec<Body> {
        const DISC_RADIUS: f32 = 0.8;
        // keeps the innermost orbits clear of the central body
        const INNER_RADIUS: f32 = 0.05;
        const CENTRAL_MASS: f32 = 0.1;
        // total, shared between all disc bodies
        const DISC_MASS: f32 = 0.08;

        let mut rng = StdRng::seed_from_u64(42);
        let mut objects = Vec::with_capacity(count + 1);

        objects.push(Body {
            mass: CENTRAL_MASS,
            pos: Vec2::new(0.0, 0.0),
            radius: 0.03,
            vel: Vec2::new(0.0, 0.0),
        });

        let body_mass = DISC_MASS / count.max(1) as f32;
        let min_fraction = (INNER_RADIUS / DISC_RADIUS).powi(2);
        for _ in 0..count {
            let angle = rng.random_range(0.0..std::f32::consts::TAU);
            // the square root spreads bodies evenly by area instead of bunching them at the centre
            let r = DISC_RADIUS * rng.random_range(min_fraction..1.0f32).sqrt();

            // mass inside this orbit: the central body plus the part of the disc within r
            let enclosed = CENTRAL_MASS + DISC_MASS * (r / DISC_RADIUS).powi(2);
            let speed = (G * enclosed / r).sqrt();

            objects.push(Body {
                mass: body_mass,
                pos: Vec2::new(r * angle.cos(), r * angle.sin()),
                radius: 0.005,
                // perpendicular to the radius
                vel: Vec2::new(-angle.sin(), angle.cos()) * speed,
            });
        }

        objects
    }

    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    pub fn update(&mut self, dt: f32) {

        self.tree.build(&self.bodies);
        self.acc.resize(self.bodies.len(), Vec2::new(0.0, 0.0));

        for i in 0..self.bodies.len() {
            self.acc[i] = self.tree.acceleration(i, &self.bodies, THETA, SOFTENING2, &mut self.stack) * G;
        }

        for (b, a) in self.bodies.iter_mut().zip(&self.acc){
            b.vel = b.vel + *a * dt;
            b.pos = b.pos + b.vel * dt;

            if b.pos.x > 1.0 {
                b.pos.x = 1.0;
                b.vel.x = -b.vel.x;
            } else if b.pos.x < -1.0 {
                b.pos.x = -1.0;
                b.vel.x = -b.vel.x;
            }

            if b.pos.y > 1.0 {
                b.pos.y = 1.0;
                b.vel.y = -b.vel.y;
            } else if b.pos.y < -1.0 {
                b.pos.y = -1.0;
                b.vel.y = -b.vel.y;
            }
        }
    }
}
