use crate::model::quad_tree::{Body, QuadTree};

use rayon::prelude::*;
use glm::{Vec2};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// Gravitational constant. The disc's masses are chosen for G = 1.
pub const G: f32 = 1.0;
/// Density of the mass-1.0 bodies in the Random, Circle and Grid patterns, giving them a radius of 0.002.
const PATTERN_DENSITY: f32 = 1.0 / (0.002 * 0.002 * 0.002);

/// Approximation constants
const THETA: f32 = 0.5;
const SOFTENING2: f32 = 1e-3;

/// Collision Constants
const RESTITUTION: f32 = 0.5;
const MERGE_DEPTH: f32 = 0.90;
const SEPARATION_RATE: f32 = 0.3;
const SEPARATION_SLOP: f32 = 0.05;

/// Shatter Constants
const FRAGMENTS: usize = 5;
const FRAGMENT_SPEED: f32 = 0.75;
const SHATTER_RATIO: f32 = 1.0;
const MIN_SHATTER_MASS: f32 = 1e-4;


/// Other Consts
const SPEED_SPREAD: f32 = 0.05;
const CULL_RADIUS: f32 = 10.0;

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
    pairs: Vec<(u32, u32)>,
    alive: Vec<bool>,
    spawned: Vec<Body>
}

impl World {
    pub fn new(count: usize, pattern: SpawnPattern) -> Self {
        let is_disc = matches!(pattern, SpawnPattern::Disc);
        let quads = match pattern {
            SpawnPattern::Random => Self::create_random_objects(count),
            SpawnPattern::Circle => Self::create_circle_objects(count, 0.5),
            SpawnPattern::Grid => Self::create_grid_objects(count, 0.5),
            SpawnPattern::Disc => Self::create_exponential_disc_objects(count),
        };
 
        let mut world = World { 
            bodies: quads, 
            tree: QuadTree::new(8, 16), 
            stack: Vec::new(), 
            acc: Vec::new(), 
            pairs: Vec::new(), 
            alive: Vec::new(),
            spawned: Vec::new(),
        };

        if is_disc { world.set_circular_velocities(); }
        world
    }

    fn create_random_objects(count: usize) -> Vec<Body> {
        let mut rng = StdRng::seed_from_u64(42);
        let mut objects = Vec::with_capacity(count);

        for _ in 0..count {
            objects.push(Body::new(
                Vec2::new(
                    rng.random_range(-1.0..1.0),
                    rng.random_range(-1.0..1.0),
                ),
                Vec2::new(0.50, -0.25),
                1.0,
                PATTERN_DENSITY,
            ));
        }

        objects
    }

    fn create_circle_objects(count: usize, radius: f32) -> Vec<Body> {
        let mut objects = Vec::with_capacity(count);

        for i in 0..count {
            let angle = (i as f32 / count as f32) * std::f32::consts::TAU;

            objects.push(Body::new(
                Vec2::new(radius * angle.cos(), radius * angle.sin()),
                Vec2::new(angle.cos(), angle.sin()),
                1.0,
                PATTERN_DENSITY,
            ));
        }

        objects
    }

    fn create_grid_objects(count: usize, spacing: f32) -> Vec<Body> {
        let width = (count as f64).sqrt() as usize;
        let height = (count as f64).sqrt() as usize;

        let mut objects = Vec::with_capacity(width * height);

        for y in 0..height {
            for x in 0..width {
                objects.push(Body::new(
                    Vec2::new(x as f32 * spacing, y as f32 * spacing),
                    Vec2::new(0.0, 0.0),
                    1.0,
                    PATTERN_DENSITY,
                ));
            }
        }
        objects
    }

    fn create_exponential_disc_objects(count: usize) -> Vec<Body> {
        const DISC_RADIUS: f32 = 1.0;
        // keeps the innermost orbits clear of the central body
        const INNER_RADIUS: f32 = 0.05;
        const CENTRAL_MASS: f32 = 0.85;
        const CENTRAL_DENSITY: f32 = CENTRAL_MASS / (0.05 * 0.05 * 0.05);
        // total, shared between all disc bodies
        const DISC_MASS: f32 = 0.15;
        // surface density falls by a factor of e every SCALE_LENGTH
        const SCALE_LENGTH: f32 = 0.3;

        // fraction of an untruncated exponential disc's mass inside r
        let cumulative = |r: f32| {
            let x = r / SCALE_LENGTH;
            1.0 - (1.0 + x) * (-x).exp()
        };
        let (f_inner, f_outer) = (cumulative(INNER_RADIUS), cumulative(DISC_RADIUS));

        let mut rng = StdRng::seed_from_u64(42);
        let mut objects = Vec::with_capacity(count + 1);

        objects.push(Body::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), CENTRAL_MASS, CENTRAL_DENSITY));

        let body_mass = DISC_MASS / count.max(1) as f32;
        let body_density = body_mass / (0.002_f32).powi(3);
        for _ in 0..count {
            let angle = rng.random_range(0.0..std::f32::consts::TAU);
            // Gamma(2, h) is the sum of two exponentials; redraw until i
            let r = loop {
                let u: f32 = rng.random_range(f32::EPSILON..1.0) * rng.random_range(f32::EPSILON..1.0);
                let r = -SCALE_LENGTH * u.ln();
                if (INNER_RADIUS..DISC_RADIUS).contains(&r) {
                    break r;
                }
            };

            // mass inside this orbit: the central body plus the part of the disc within r
            let disc_fraction = (cumulative(r) - f_inner) / (f_outer - f_inner);
            let enclosed = CENTRAL_MASS + DISC_MASS * disc_fraction;
            let speed = (G * enclosed / r).sqrt();

            objects.push(Body::new(
                    Vec2::new(r * angle.cos(), r * angle.sin()),
                    Vec2::new(-angle.sin(), angle.cos()) * speed,
                    body_mass, 
                    body_density));
        }

        objects
    }

    fn set_circular_velocities(&mut self) {
        self.tree.build(&self.bodies);
        let center = self.bodies[0].pos;
        let mut rng = StdRng::seed_from_u64(7);
        for i in 1..self.bodies.len() {
            let a = self.tree.acceleration(i, &self.bodies, THETA, SOFTENING2, &mut self.stack) * G;
            let d = self.bodies[i].pos - center;
            let r = (d.x * d.x + d.y * d.y).sqrt();
            if r < 1e-6 { continue; }
            let r_hat = d * (1.0 / r);
            let a_in = -(a.x * r_hat.x + a.y *r_hat.y);
            let speed = (r * a_in.max(0.0)).sqrt();

            let u1: f32 = rng.random_range(f32::EPSILON..1.0);
            let u2: f32 = rng.random_range(0.0..1.0);
            let z = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos();
            let factor = (1.0 + SPEED_SPREAD * z).max(0.0);
            self.bodies[i].vel = Vec2::new(-r_hat.y, r_hat.x) * speed * factor;
                
        }
        

    }

    pub fn add_body(&mut self, start: Vec2, target: Vec2, speed: f32, mass: f32, density: f32) {
       let d = target - start;
       let len = (d.x * d.x + d.y * d.y).sqrt();
       let dir = if len > 1e-6 { d * (1.0 / len) } else { Vec2::new(0.0, 0.0) };
       self.bodies.push(Body::new(start, dir * speed, mass, density));
    }

    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    
    fn resolve_collisions(&mut self) -> bool {
        self.tree.collision_pairs(&self.bodies, &mut self.stack, &mut self.pairs);
        if self.pairs.is_empty() { return false; }

        self.alive.clear();
        self.alive.resize(self.bodies.len(), true);
        let mut changed = false;

        for p in 0..self.pairs.len() {
            let (i, j) = (self.pairs[p].0 as usize, self.pairs[p].1 as usize);
            if !self.alive[i] || !self.alive[j] { continue; }

            let (a, b) = (self.bodies[i], self.bodies[j]);
            let d = b.pos - a.pos;
            let dist2 = d.x * d.x + d.y * d.y;
            let contact = a.radius + b.radius;
            if dist2 >= contact * contact { continue; }

            let dv = b.vel - a.vel;
            let dv2 = dv.x * dv.x + dv.y * dv.y;
            let v_esc2 = 2.0 * G * (a.mass + b.mass) / (contact * contact + SOFTENING2).sqrt();
            let bound = dv2 < v_esc2;

            let mass = a.mass + b.mass;
            let q2 = dv2 * (a.mass * b.mass / mass) / mass;

            let shatter = q2 > SHATTER_RATIO * SHATTER_RATIO * v_esc2
                && mass >= MIN_SHATTER_MASS
                && i != 0;
            if bound {
                if dist2 < MERGE_DEPTH * MERGE_DEPTH * contact * contact {
                    self.bodies[i] = merge(a, b);
                    self.alive[j] = false;
                    changed = true;
                } else {    
                    let (a, b) = bounce(a, b, d, dist2.sqrt());
                    self.bodies[i] = a;
                    self.bodies[j] = b;
                }
            } else if shatter {
                shatter_into(a, b, q2.sqrt(), &mut self.spawned);
                self.alive[i] = false;
                self.alive[j] = false;
                changed = true;
            } else {
                let (a, b) = bounce(a, b, d, dist2.sqrt());
                self.bodies[i] = a;
                self.bodies[j] = b;
            }

        }

        if changed {
            let alive = &self.alive;
            let mut k = 0;
            self.bodies.retain(|_| {
                k += 1;
                alive[k - 1]
            });
            self.bodies.extend(self.spawned.drain(..));
        }
        changed
    }

    pub fn update(&mut self, dt: f32) {

        self.tree.build(&self.bodies);
        if self.resolve_collisions() {
            self.tree.build(&self.bodies);
        }
        self.acc.resize(self.bodies.len(), Vec2::new(0.0, 0.0));

        let (tree, bodies) = (&self.tree, &self.bodies);
        self.acc
            .par_iter_mut()
            .enumerate()
            .for_each_init(Vec::new, |stack, (i,a)| {
                *a = tree.acceleration(i, bodies, THETA, SOFTENING2, stack) * G;
            });

        // for i in 0..self.bodies.len() {
        //     self.acc[i] = self.tree.acceleration(i, &self.bodies, THETA, SOFTENING2, &mut self.stack) * G;
        // }

        let c = self.bodies[0];
        for (b, a) in self.bodies.iter_mut().zip(&self.acc){
            b.vel = b.vel + *a * dt;
            b.pos = b.pos + b.vel * dt;

            // reframing around the central mass
            b.pos = b.pos - c.pos;
            b.vel = b.vel - c.vel;
        }
        let c = c.pos;
        self.bodies.retain(|b| {
            let d = b.pos - c;
            d.x * d.x + d.y * d.y < CULL_RADIUS * CULL_RADIUS
        });

    }
}

fn shatter_into(a: Body, b: Body, impact_q: f32, out: &mut Vec<Body>) {
    let whole = merge(a, b);
    let n = FRAGMENTS;
    let mass = whole.mass / n as f32;
    let radius = (mass / whole.density).cbrt();
    let speed = impact_q * FRAGMENT_SPEED;

    let d = b.pos - a.pos;
    let base = d.y.atan2(d.x);
    for k in 0..n {
        let angle = base + std::f32::consts::TAU * k as f32 / n as f32;
        let dir = Vec2::new(angle.cos(), angle.sin());
        out.push(Body::new(whole.pos + dir * (2.0 * radius), whole.vel + dir * speed, mass, whole.density));
    }
}

fn merge(a: Body, b: Body) -> Body {
    let mass = a.mass + b.mass;
    let (wa, wb) = (a.mass / mass, b.mass / mass);
    let volume = a.mass / a.density + b.mass / b.density;
    Body::new(a.pos * wa + b.pos * wb, a.vel * wa + b.vel * wb, mass, mass / volume)
}

fn bounce(mut a: Body, mut b: Body, d: Vec2, dist: f32) -> (Body, Body) {
    let n = if dist > 1e-9 { d * (1.0 / dist) } else { Vec2::new(1.0, 0.0) };

    let (inv_a, inv_b) = (1.0 / a.mass, 1.0 / b.mass);

    let dv = b.vel - a.vel;
    let vn = dv.x * n.x + dv.y * n.y;
    
    if vn < 0.0 {
        let j = -(1.0 + RESTITUTION) * vn / (inv_a + inv_b);
        a.vel = a.vel - n * (j * inv_a);
        b.vel = b.vel + n * (j * inv_b);
    }

    let contact = a.radius + b.radius;
    let overlap = contact - dist;
    let correction = (overlap - SEPARATION_SLOP * contact).max(0.0) * SEPARATION_RATE;
    let share = correction / (inv_a + inv_b);
    a.pos = a.pos - n * (share * inv_a);
    b.pos = b.pos + n * (share * inv_b);
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(x: f32, vx: f32, mass: f32, radius: f32) -> Body {
        Body::new(Vec2::new(x, 0.0), Vec2::new(vx, 0.0), mass, mass / radius.powi(3))
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    fn collide(bodies: Vec<Body>) -> World {
        let mut w = World {
            bodies,
            tree: QuadTree::new(8, 16),
            stack: Vec::new(),
            acc: Vec::new(),
            pairs: Vec::new(),
            alive: Vec::new(),
            spawned: Vec::new(),
        };
        w.tree.build(&w.bodies);
        w.resolve_collisions();
        w
    }

    #[test]
    fn grain_merging_into_dense_body_barely_grows_it() {
        // the central body and a disc particle, at their spawn densities
        let w = collide(vec![body(0.0, 0.0, 0.9, 0.05), body(0.03, 0.0, 1.8e-6, 0.002)]);
        assert_eq!(w.bodies.len(), 1);
        let b = w.bodies[0];
        assert!(close(b.radius, 0.05), "radius {}", b.radius);
    }

    #[test]
    fn slow_deep_overlapping_pair_merges() {
        let w = collide(vec![body(0.0, 0.0, 1.0, 0.03), body(0.02, -0.1, 3.0, 0.04)]);
        assert_eq!(w.bodies.len(), 1);
        let b = w.bodies[0];
        assert!(close(b.mass, 4.0), "mass {}", b.mass);
        assert!(close(b.vel.x * b.mass, -0.3), "momentum {}", b.vel.x * b.mass);
        assert!(close(b.pos.x, 0.015), "center of mass {}", b.pos.x);
        // volume is conserved: cbrt(0.03^3 + 0.04^3)
        assert!(close(b.radius, 0.044979), "radius {}", b.radius);
    }

    #[test]
    fn slow_shallow_overlapping_pair_is_left_for_gravity() {
        // halfway between the merge depth and first contact
        let x = 0.02 * (MERGE_DEPTH + 1.0) / 2.0;
        let w = collide(vec![body(0.0, 0.0, 1.0, 0.01), body(x, 0.0, 1.0, 0.01)]);
        assert_eq!(w.bodies.len(), 2);
        assert_eq!(w.bodies[0].pos.x, 0.0);
        assert_eq!(w.bodies[1].pos.x, x);
    }

    /// Gap after one `bounce` between two r = 0.01 bodies that start 0.005 apart.
    fn gap_after_bounce() -> f32 {
        let (contact, start) = (0.02, 0.005);
        start + (contact - start - SEPARATION_SLOP * contact).max(0.0) * SEPARATION_RATE
    }

    #[test]
    fn fast_overlapping_pair_bounces() {
        let w = collide(vec![body(0.0, 50.0, 1.0, 0.01), body(0.005, -50.0, 3.0, 0.01)]);
        assert_eq!(w.bodies.len(), 2);
        let (a, b) = (w.bodies[0], w.bodies[1]);
        let momentum = a.vel.x + 3.0 * b.vel.x;
        assert!(close(momentum, -100.0), "momentum {momentum}");
        // separating at RESTITUTION times the approach speed of 100
        let separation_speed = b.vel.x - a.vel.x;
        assert!(close(separation_speed, 100.0 * RESTITUTION), "separation speed {separation_speed}");
        // pushed apart by part of the overlap, and the heavier body moved a third as far
        assert!(close(b.pos.x - a.pos.x, gap_after_bounce()), "gap {}", b.pos.x - a.pos.x);
        assert!(close(-a.pos.x, 3.0 * (b.pos.x - 0.005)), "push split {} {}", a.pos.x, b.pos.x);
    }

    #[test]
    fn separating_overlapping_pair_is_only_pushed_apart() {
        let w = collide(vec![body(0.0, -50.0, 1.0, 0.01), body(0.005, 50.0, 1.0, 0.01)]);
        assert_eq!(w.bodies.len(), 2);
        assert_eq!(w.bodies[0].vel.x, -50.0);
        assert_eq!(w.bodies[1].vel.x, 50.0);
        let gap = w.bodies[1].pos.x - w.bodies[0].pos.x;
        assert!(close(gap, gap_after_bounce()), "gap {gap}");
    }

    #[test]
    fn overlap_within_slop_is_not_pushed() {
        let x = 0.02 * (1.0 - SEPARATION_SLOP / 2.0);
        let w = collide(vec![body(0.0, 50.0, 1.0, 0.01), body(x, -50.0, 1.0, 0.01)]);
        assert_eq!(w.bodies[0].pos.x, 0.0);
        assert_eq!(w.bodies[1].pos.x, x);
    }

    #[test]
    fn separate_pair_is_untouched() {
        let w = collide(vec![body(0.0, 0.0, 1.0, 0.01), body(0.5, 0.0, 1.0, 0.01)]);
        assert_eq!(w.bodies.len(), 2);
        assert_eq!(w.bodies[0].pos.x, 0.0);
        assert_eq!(w.bodies[1].pos.x, 0.5);
    }

    #[test]
    fn body_absorbs_several_neighbours_in_one_step() {
        // the second merge only passes MERGE_DEPTH because the first one moved and grew body 0
        let w = collide(vec![
            body(0.0, 0.0, 1.0, 0.01),
            body(-0.006, 0.0, 1.0, 0.01),
            body(0.006, 0.0, 1.0, 0.01),
        ]);
        assert_eq!(w.bodies.len(), 1);
        assert!(close(w.bodies[0].mass, 3.0));
        assert!(close(w.bodies[0].pos.x, 0.0));
    }

    #[test]
    fn merged_bodies_are_removed_from_the_middle() {
        // only the middle two overlap, so the outer bodies must survive in place
        let w = collide(vec![
            body(-0.5, 0.0, 1.0, 0.01),
            body(0.0, 0.0, 1.0, 0.01),
            body(0.005, 0.0, 1.0, 0.01),
            body(0.5, 0.0, 1.0, 0.01),
        ]);
        assert_eq!(w.bodies.len(), 3);
        assert_eq!(w.bodies[0].pos.x, -0.5);
        assert!(close(w.bodies[1].mass, 2.0));
        assert_eq!(w.bodies[2].pos.x, 0.5);
    }

    #[test]
    fn dust_does_not_shatter_a_heavy_body() {
        // a fast asteroid clipping a dust speck: plenty of relative speed, almost no energy
        let w = collide(vec![
            body(5.0, 0.0, 1e-9, 0.001), // stand-in for the central body, which never shatters
            body(0.0, 2.0, 0.02, 0.01),
            body(0.011, 0.0, 5e-6, 0.002),
        ]);
        assert_eq!(w.bodies.len(), 3);
    }

    #[test]
    fn equal_masses_shatter_head_on_without_gaining_energy() {
        let (a, b) = (body(0.0, 2.0, 0.01, 0.01), body(0.015, -2.0, 0.01, 0.01));
        let energy = |bs: &[Body]| {
            bs.iter().map(|b| 0.5 * b.mass * (b.vel.x * b.vel.x + b.vel.y * b.vel.y)).sum::<f32>()
        };
        let before = energy(&[a, b]);

        let w = collide(vec![body(5.0, 0.0, 1e-9, 0.001), a, b]);
        assert_eq!(w.bodies.len(), 1 + FRAGMENTS);

        let fragments = &w.bodies[1..];
        assert!(energy(fragments) <= before, "energy {} > {}", energy(fragments), before);
        let px: f32 = fragments.iter().map(|f| f.mass * f.vel.x).sum();
        let py: f32 = fragments.iter().map(|f| f.mass * f.vel.y).sum();
        assert!(px.abs() < 1e-6 && py.abs() < 1e-6, "momentum ({px}, {py})");
    }
}
