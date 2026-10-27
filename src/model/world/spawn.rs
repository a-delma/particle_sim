//! Initial layouts for `World`: one constructor per `SpawnPattern`.

use std::time::{SystemTime, UNIX_EPOCH};

use super::{World, G, SOFTENING2, THETA};
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
/// a, e, mass (sun = 1), drawn radius, glow colour. Colours keep their brightest channel
/// around 0.6–0.85, like DEFAULT_COLOR, since the glow is additive.
const PLANETS: [(f32, f32, f32, f32, [f32; 3]); 8] = [
    (0.11, 0.21, 1.7e-7, 0.004, [0.55, 0.52, 0.50]), // Mercury: grey
    (0.15, 0.01, 2.4e-6, 0.006, [0.75, 0.65, 0.40]), // Venus: pale yellow
    (0.17, 0.02, 3.0e-6, 0.006, [0.30, 0.50, 0.80]), // Earth: blue
    (0.21, 0.09, 3.2e-7, 0.005, [0.80, 0.35, 0.20]), // Mars: rust red
    (0.40, 0.05, 9.5e-4, 0.016, [0.80, 0.60, 0.40]), // Jupiter: tan
    (0.54, 0.06, 2.9e-4, 0.014, [0.80, 0.70, 0.45]), // Saturn: pale gold
    (0.76, 0.05, 4.4e-5, 0.010, [0.45, 0.75, 0.80]), // Uranus: cyan
    (0.95, 0.01, 5.2e-5, 0.010, [0.30, 0.40, 0.85]), // Neptune: deep blue
];
const SUN_COLOR: [f32; 3] = [0.85, 0.70, 0.40];
const ASTEROID_COLOR: [f32; 3] = [0.45, 0.40, 0.35];
const COMET_COLOR: [f32; 3] = [0.60, 0.80, 0.85];

/// Spread of the disc's orbital speeds, as a fraction of the circular speed.
const SPEED_SPREAD: f32 = 0.0;

/// Color Constants
const DEFAULT_COLOR: [f32; 3] = [0.52, 0.30, 0.60];
// Brightest channel kept around 0.6–0.8 since the glow is additive. Green and yellow sit a
// little lower because they look brighter than red or blue at the same values.
const VIOLET: [f32; 3] = [0.52, 0.30, 0.60];
const RED: [f32; 3] = [0.75, 0.25, 0.22];
const BLUE: [f32; 3] = [0.25, 0.45, 0.80];
const GREEN: [f32; 3] = [0.28, 0.62, 0.36];
const YELLOW: [f32; 3] = [0.75, 0.62, 0.28];

const COLORS: [[f32; 3]; 5] = [
    VIOLET,
    RED,
    BLUE,
    GREEN,
    YELLOW
];

pub enum SpawnPattern {
    Random,
    Grid,
    Circle,
    /// A heavy central body with the rest orbiting it in a disc.
    Disc,
    Galaxy,
    Encounter,
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
            SpawnPattern::Disc => Self::create_exponential_disc_objects(count, DEFAULT_COLOR),
            SpawnPattern::Galaxy => Self::create_galaxy_objects(count),
            SpawnPattern::Encounter => Self::create_galaxy_encounter_objects(count),
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

        objects.push(Body::new(Vec2::new(0.0, 0.0), drift, mass, density, DEFAULT_COLOR));
        for _ in 1..count {
            objects.push(Body::new(
                Vec2::new(
                    rng.random_range(-1.0..1.0),
                    rng.random_range(-1.0..1.0),
                ),
                drift,
                mass,
                density,
                DEFAULT_COLOR,
            ));
        }

        objects
    }

    /// A ring of bodies flying outwards from its centre. Body 0 sits still at the centre,
    /// which is also the ring's centre of mass.
    fn create_circle_objects(count: usize, radius: f32) -> Vec<Body> {
        let mut objects = Vec::with_capacity(count);
        let (mass, density) = Self::pattern_body(count);

        objects.push(Body::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), mass, density, DEFAULT_COLOR));
        let ring = count.saturating_sub(1).max(1);
        for i in 0..ring {
            let angle = (i as f32 / ring as f32) * std::f32::consts::TAU;

            objects.push(Body::new(
                Vec2::new(radius * angle.cos(), radius * angle.sin()),
                Vec2::new(angle.cos(), angle.sin()),
                mass,
                density,
                DEFAULT_COLOR
            ));
        }

        objects
    }

    /// A square grid at rest, centred on the origin and `half_size` from the centre to each
    /// side. The side has an odd number of bodies so one lands exactly on the centre, and
    /// that one is body 0.
    fn create_grid_objects(count: usize, half_size: f32) -> Vec<Body> {
        let mut side = (count as f64).sqrt() as usize;
        if side.is_multiple_of(2) {
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
                    DEFAULT_COLOR,
                ));
            }
        }
        let centre = (side / 2) * side + side / 2;
        objects.swap(0, centre);
        objects
    }

    fn create_exponential_disc_objects(count: usize, color: [f32; 3]) -> Vec<Body> {
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

        objects.push(Body::new(Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), CENTRAL_MASS, CENTRAL_DENSITY, color));

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
                    body_density,
                    color));
        }

        objects
    }

    fn create_galaxy_inner(
        count: usize, 
        rng: &mut StdRng, 
        center: Vec2, 
        bulk_vel: Vec2, 
        t_mass: f32,
        color: [f32; 3]) -> Vec<Body> {

        // lighter galaxies are smaller: lengths scale with √mass, so 1.0 is the full-size galaxy
        let size = t_mass.sqrt();
        let disc_radius: f32 = 1.0 * size;
        // keeps the innermost orbits clear of the central body
        const INNER_RADIUS: f32 = 0.05;
        const CENTRAL_RATIO: f32 = 0.85;
        let central_mass: f32 = t_mass * CENTRAL_RATIO;
        let central_density: f32 = central_mass / (0.05 * 0.05 * 0.05);
        // total, shared between all disc bodies
        const DISC_RATIO: f32 = 0.15;
        let disc_mass = t_mass * DISC_RATIO;
        // surface density falls by a factor of e every scale_length
        let scale_length: f32 = 0.3 * size;

        // fraction of an untruncated exponential disc's mass inside r
        let cumulative = |r: f32| {
            let x = r / scale_length;
            1.0 - (1.0 + x) * (-x).exp()
        };
        let (f_inner, f_outer) = (cumulative(INNER_RADIUS), cumulative(disc_radius));

        // Setting up initial conditions
        
        let n_arms: usize = rng.random_range(2..5);
        let pitch: f32 = rng.random_range(12.0..28.0);
        let a_phase: f32 = rng.random_range(0.0..std::f32::consts::TAU);
        let a_width: f32 = rng.random_range(0.15..0.35);
        let frac: f32 = rng.random_range(0.15..0.35);
        let spin: f32 = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
        let r_bulge = rng.random_range(0.15..0.3) * size;
        eprintln!(
            "galaxy: {n_arms} arms, pitch {pitch:.1}°, phase {a_phase:.2} rad, \
             width {a_width:.2} rad, between-arm {:.0}%, spin {}, bulge {r_bulge:.2}",
            frac * 100.0,
            if spin > 0.0 { "counterclockwise" } else { "clockwise" },
        );

        
        let mut objects = Vec::with_capacity(count + 1);

        objects.push(Body::new(
                Vec2::new(0.0, 0.0) + center, 
                Vec2::new(0.0, 0.0) + bulk_vel, 
                central_mass, 
                central_density,
                color));

        let body_mass = disc_mass / count.max(1) as f32;
        let body_density = body_mass / (0.002_f32).powi(3);
        for _ in 0..count {
            
            // Gamma(2, h) is the sum of two exponentials; redraw until i
            let r = loop {
                let u: f32 = rng.random_range(f32::EPSILON..1.0) * rng.random_range(f32::EPSILON..1.0);
                let r = -scale_length * u.ln();
                if (INNER_RADIUS..disc_radius).contains(&r) {
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
            let enclosed = central_mass + disc_mass * disc_fraction;
            // circular speed under the softened pull G·M·r / (r² + ε²)^1.5 that `pull` applies,
            // rather than plain G·M / r², which is up to 40% stronger at the inner edge and would
            // send the innermost bodies out on wide ellipses
            let r2 = r * r;
            let speed = (G * enclosed * r2 / (r2 + SOFTENING2).powf(1.5)).sqrt();

            objects.push(Body::new(
                    Vec2::new(r * angle.cos(), r * angle.sin()) + center,
                    Vec2::new(-angle.sin(), angle.cos()) * speed * spin + bulk_vel,
                    body_mass, 
                    body_density,
                    color));
        }

        objects
    }


    fn create_galaxy_objects(count: usize) -> Vec<Body> {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let mut rng = StdRng::seed_from_u64(t);
        Self::create_galaxy_inner(count, &mut rng, Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), 1.0, DEFAULT_COLOR)
    }

    fn create_galaxy_encounter_objects(count: usize) -> Vec<Body> {
        const MAIN_MASS: f32 = 1.0;
        const MASS_RATIO: f32 = 0.5;
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let mut rng = StdRng::seed_from_u64(t);
        eprintln!("encounter: seed {t}, main galaxy then companion:");
        let c1 = rng.random_range(0..COLORS.len());
        let c2 = c1 + rng.random_range(1..COLORS.len());
        let c1 = COLORS[c1];
        let c2 = COLORS[c2 % COLORS.len()];


        let mut bodies = Self::create_galaxy_inner(count, &mut rng, Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), MAIN_MASS, c1);
        // let mut bodies = Self::create_exponential_disc_objects(count, c1);

        let q: f32 = rng.random_range(1.5..2.2);
        let e: f32 = rng.random_range(0.5..0.8);
        // far enough that the companion's whole disc (radius √MASS_RATIO) starts outside the
        // ±1.78 × ±1 view
        let r_zero: f32 = 4.5;
        let omega: f32 = rng.random_range(0.0..std::f32::consts::TAU);
        let p: f32 = q * (1.0 + e);
        let a: f32 = q / (1.0 - e);
        let nu: f32 = -((p / r_zero - 1.0) / e).clamp(-1.0, 1.0).acos();

        let (center, bulk_vel) = Self::kepler_state(MAIN_MASS * (1.0 + MASS_RATIO), a, e, nu, omega);
        let speed = (bulk_vel.x * bulk_vel.x + bulk_vel.y * bulk_vel.y).sqrt();
        eprintln!(
            "encounter orbit: mass ratio {MASS_RATIO}, pericentre {q:.2}, eccentricity {e:.2}, \
             apocentre {:.1}, approach {:.0}°, start nu {:.0}° at r {r_zero}",
            a * (1.0 + e),
            omega.to_degrees(),
            nu.to_degrees(),
        );
        eprintln!(
            "encounter start: companion at ({:.2}, {:.2}), velocity ({:.2}, {:.2}), speed {speed:.2}",
            center.x, center.y, bulk_vel.x, bulk_vel.y,
        );
        let s_count = (count as f32 * MASS_RATIO) as usize;
        bodies.append(&mut Self::create_galaxy_inner(s_count, &mut rng, center, bulk_vel, MASS_RATIO, c2));
        bodies
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
            Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0), SUN_MASS, SUN_MASS / SUN_RADIUS.powi(3), SUN_COLOR,
        )];

        for &(a, e, mass, radius, color) in &PLANETS {
            let (pos, vel) = Self::kepler_state(SUN_MASS, a, e, rng.random_range(0.0..TAU), rng.random_range(0.0..TAU));
            objects.push(Body::new(pos, vel, mass, mass / radius.powi(3), color));
        }

        // asteroid belt between Mars and Jupiter: massless in practice, slightly eccentric
        let rock = 1e-10;
        for _ in 0..asteroids {
            let a = rng.random_range(0.26..0.34);
            let e = rng.random_range(0.0..0.12);
            let (pos, vel) = Self::kepler_state(SUN_MASS, a, e, rng.random_range(0.0..TAU), rng.random_range(0.0..TAU));
            objects.push(Body::new(pos, vel, rock, rock / 0.001_f32.powi(3), ASTEROID_COLOR));
        }

        // a comet starting at aphelion (nu = π), diving in to 0.18 at perihelion
        let (pos, vel) = Self::kepler_state(SUN_MASS, 0.6, 0.7, std::f32::consts::PI, 1.0);
        objects.push(Body::new(pos, vel, 1e-9, 1e-9 / 0.004_f32.powi(3), COMET_COLOR));

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
