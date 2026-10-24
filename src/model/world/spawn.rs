//! Initial layouts for `World`: one constructor per `SpawnPattern`.

use std::time::{SystemTime, UNIX_EPOCH};

use super::{World, G, THETA};
use crate::model::quad_tree::Body;

use glm::Vec2;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// Total mass of the Random, Circle and Grid patterns, shared equally between their bodies.
/// Matches the disc pattern's total, which its velocities and the softening are tuned for.
const PATTERN_MASS: f32 = 1.0;
/// Radius of each body in the Random, Circle and Grid patterns.
const PATTERN_RADIUS: f32 = 0.002;

/// Solar Constants
const PLANETS: [(f32, f32, f32, f32); 8] = [
    (0.11, 0.21, 1.7e-7, 0.004), // Mercury
    (0.15, 0.01, 2.4e-6, 0.006), // Venus
    (0.17, 0.02, 3.0e-6, 0.006), // Earth
    (0.21, 0.09, 3.2e-7, 0.005), // Mars
    (0.40, 0.05, 9.5e-4, 0.016), // Jupiter
    (0.54, 0.06, 2.9e-4, 0.014), // Saturn
    (0.76, 0.05, 4.4e-5, 0.010), // Uranus
    (0.95, 0.01, 5.2e-5, 0.010), // Neptune
];

/// Spread of the disc's orbital speeds, as a fraction of the circular speed.
const SPEED_SPREAD: f32 = 0.0;

pub enum SpawnPattern {
    Random,
    Grid,
    Circle,
    /// A heavy central body with the rest orbiting it in a disc.
    Disc,
    Galaxy,
    Solar,
}

impl World {
    /// The starting bodies for `pattern`. The disc's velocities are set afterwards by
    /// `set_circular_velocities`, since they depend on the tree.
    pub(super) fn spawn_bodies(pattern: &SpawnPattern, count: usize) -> Vec<Body> {
        match pattern {
            SpawnPattern::Random => Self::create_random_objects(count),
            SpawnPattern::Circle => Self::create_circle_objects(count, 0.5),
            SpawnPattern::Grid => Self::create_grid_objects(count, 0.9),
            SpawnPattern::Disc => Self::create_exponential_disc_objects(count),
            SpawnPattern::Galaxy => Self::create_galaxy_objects(count),
            SpawnPattern::Solar => Self::create_solar_objects(count),
        }
    }

    /// Mass and density for one of `count` equal bodies sharing PATTERN_MASS.
    fn pattern_body(count: usize) -> (f32, f32) {
        let mass = PATTERN_MASS / count.max(1) as f32;
        (mass, mass / PATTERN_RADIUS.powi(3))
    }

    /// Bodies scattered uniformly over the view, all drifting together. Body 0 sits at the
    /// centre, so the view stays on the middle of the cloud.
    fn create_random_objects(count: usize) -> Vec<Body> {
        let mut rng = StdRng::seed_from_u64(42);
        let mut objects = Vec::with_capacity(count);
        let (mass, density) = Self::pattern_body(count);
        let drift = Vec2::new(0.50, -0.25);

        objects.push(Body::new(Vec2::new(0.0, 0.0), drift, mass, density));
        for _ in 1..count {
            objects.push(Body::new(
                Vec2::new(
                    rng.random_range(-1.0..1.0),
                    rng.random_range(-1.0..1.0),
                ),
                drift,
                mass,
                density,
            ));
        }

        objects
    }

    /// A ring of bodies flying outwards from its centre. Body 0 sits still at the centre,
    /// which is also the ring's centre of mass.
    fn create_circle_objects(count: usize, radius: f32) -> Vec<Body> {
        let mut objects = Vec::with_capacity(count);
        let (mass, density) = Self::pattern_body(count);

        objects.push(Body::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), mass, density));
        let ring = count.saturating_sub(1).max(1);
        for i in 0..ring {
            let angle = (i as f32 / ring as f32) * std::f32::consts::TAU;

            objects.push(Body::new(
                Vec2::new(radius * angle.cos(), radius * angle.sin()),
                Vec2::new(angle.cos(), angle.sin()),
                mass,
                density,
            ));
        }

        objects
    }

    /// A square grid at rest, centred on the origin and `half_size` from the centre to each
    /// side. The side has an odd number of bodies so one lands exactly on the centre, and
    /// that one is body 0.
    fn create_grid_objects(count: usize, half_size: f32) -> Vec<Body> {
        let mut side = (count as f64).sqrt() as usize;
        if side % 2 == 0 {
            side = side.saturating_sub(1);
        }
        let side = side.max(1);
        let spacing = if side > 1 { 2.0 * half_size / (side - 1) as f32 } else { 0.0 };
        let (mass, density) = Self::pattern_body(side * side);

        let mut objects = Vec::with_capacity(side * side);
        for y in 0..side {
            for x in 0..side {
                objects.push(Body::new(
                    Vec2::new(x as f32 * spacing - half_size, y as f32 * spacing - half_size),
                    Vec2::new(0.0, 0.0),
                    mass,
                    density,
                ));
            }
        }
        let centre = (side / 2) * side + side / 2;
        objects.swap(0, centre);
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


    fn create_galaxy_objects(count: usize) -> Vec<Body> {
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

        // Setting up initial conditions
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let mut rng = StdRng::seed_from_u64(t);
        let n_arms: usize = rng.random_range(2..5);
        let pitch: f32 = rng.random_range(12.0..28.0);
        let a_phase: f32 = rng.random_range(0.0..std::f32::consts::TAU);
        let a_width: f32 = rng.random_range(0.15..0.35);
        let frac: f32 = rng.random_range(0.15..0.35);
        let spin: f32 = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
        let r_bulge = rng.random_range(0.15..0.3);
        eprintln!(
            "galaxy: seed {t}, {n_arms} arms, pitch {pitch:.1}°, phase {a_phase:.2} rad, \
             width {a_width:.2} rad, between-arm {:.0}%, spin {}, buldge {r_bulge:.2}",
            frac * 100.0,
            if spin > 0.0 { "counterclockwise" } else { "clockwise" },
        );

        
        let mut objects = Vec::with_capacity(count + 1);

        objects.push(Body::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), CENTRAL_MASS, CENTRAL_DENSITY));

        let body_mass = DISC_MASS / count.max(1) as f32;
        let body_density = body_mass / (0.002_f32).powi(3);
        for _ in 0..count {
            
            // Gamma(2, h) is the sum of two exponentials; redraw until i
            let r = loop {
                let u: f32 = rng.random_range(f32::EPSILON..1.0) * rng.random_range(f32::EPSILON..1.0);
                let r = -SCALE_LENGTH * u.ln();
                if (INNER_RADIUS..DISC_RADIUS).contains(&r) {
                    break r;
                }
            };
            let angle: f32 = {
                let t = ((r - INNER_RADIUS) / (r_bulge - INNER_RADIUS)).clamp(0.0, 1.0);
                let t = t*t*(3.0 - 2.0*t);
                if rng.random_range(0.0..1.0) <= (1.0 - (1.0 - frac) * t) {
                    rng.random_range(0.0..std::f32::consts::TAU)
                } else {
                    let wind: f32 = 1.0 / pitch.to_radians().tan();
                    let n: usize = rng.random_range(0..n_arms);
                    let u1: f32 = rng.random_range(f32::EPSILON..1.0);
                    let u2: f32 = rng.random_range(0.0..1.0);
                    let z = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos();
                    a_phase + std::f32::consts::TAU * (n as f32) / (n_arms as f32) - spin * wind * (r / INNER_RADIUS).ln() + a_width * z
                }
            };
            // mass inside this orbit: the central body plus the part of the disc within r
            let disc_fraction = (cumulative(r) - f_inner) / (f_outer - f_inner);
            let enclosed = CENTRAL_MASS + DISC_MASS * disc_fraction;
            let speed = (G * enclosed / r).sqrt();

            objects.push(Body::new(
                    Vec2::new(r * angle.cos(), r * angle.sin()),
                    Vec2::new(-angle.sin(), angle.cos()) * speed * spin,
                    body_mass, 
                    body_density));
        }

        objects
    }

    pub(super) fn set_circular_velocities(&mut self) {
        self.tree.build(&self.bodies);
        let center = self.bodies[0].pos;
        let mut rng = StdRng::seed_from_u64(7);
        for i in 1..self.bodies.len() {
            let a = self.tree.acceleration(i, &self.bodies, THETA, self.softening2, &mut self.stack) * G;
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

    fn create_solar_objects(asteroids: usize) -> Vec<Body> {
        const SUN_MASS: f32 = 1.0;
        const SUN_RADIUS: f32 = 0.03;
        use std::f32::consts::TAU;

        let mut rng = StdRng::seed_from_u64(42);
        let mut objects = vec![Body::new(
            Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), SUN_MASS, SUN_MASS / SUN_RADIUS.powi(3),
        )];

        for &(a, e, mass, radius) in &PLANETS {
            let (pos, vel) = Self::kepler_state(SUN_MASS, a, e, rng.random_range(0.0..TAU), rng.random_range(0.0..TAU));
            objects.push(Body::new(pos, vel, mass, mass / radius.powi(3)));
        }

        // asteroid belt between Mars and Jupiter: massless in practice, slightly eccentric
        let rock = 1e-10;
        for _ in 0..asteroids {
            let a = rng.random_range(0.26..0.34);
            let e = rng.random_range(0.0..0.12);
            let (pos, vel) = Self::kepler_state(SUN_MASS, a, e, rng.random_range(0.0..TAU), rng.random_range(0.0..TAU));
            objects.push(Body::new(pos, vel, rock, rock / 0.001_f32.powi(3)));
        }

        // a comet starting at aphelion (nu = π), diving in to 0.18 at perihelion
        let (pos, vel) = Self::kepler_state(SUN_MASS, 0.6, 0.7, std::f32::consts::PI, 1.0);
        objects.push(Body::new(pos, vel, 1e-9, 1e-9 / 0.004_f32.powi(3)));

        objects
    }

    /// Position and velocity, relative to a central mass at rest at the origin, of a body on a
    /// Kepler orbit: semi-major axis `a`, eccentricity `e` (0 ≤ e < 1), true anomaly `nu` (angle
    /// from perihelion) and perihelion direction `omega`. Orbits run counter-clockwise.
    pub(super) fn kepler_state(central_mass: f32, a: f32, e: f32, nu: f32, omega: f32) -> (Vec2, Vec2) {
        let p = a * (1.0 - e * e);
        let r = p / (1.0 + e * nu.cos());
        let pos = Vec2::new(r * nu.cos(), r * nu.sin());
        let vel = Vec2::new(-nu.sin(), e + nu.cos()) * (G * central_mass / p).sqrt();
        let (s, c) = omega.sin_cos();
        let rot = |v: Vec2| Vec2::new(c * v.x - s * v.y, s * v.x + c * v.y);
        (rot(pos), rot(vel))
    }
}
