use glm::Vec2;

const LEAF: u32 = 0;

#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: Vec2,
    pub vel: Vec2,
    pub mass: f32,
    pub radius: f32,
    pub density: f32,
}

impl Body {
    pub fn new(pos: Vec2, vel: Vec2, mass: f32, density: f32) -> Self {
        Body { pos, vel, mass, density, radius: (mass / density).cbrt() }
    }
}

/// Axis-aligned square. The tree only ever subdivides squares, so one size is enough.
#[derive(Clone, Copy, Debug)]
struct Square {
    min: Vec2,
    size: f32,
}

impl Square {
    fn center(&self) -> Vec2 {
        let h = self.size / 2.0;
        Vec2::new(self.min.x + h, self.min.y + h)
    }

    fn quadrant(&self, q: usize) -> Square {
        let s = self.size / 2.0;
        let x_off =  if q % 2 == 1 { s } else { 0.0 };
        let y_off =  if q > 1 { s } else { 0.0 };
        Square { min: Vec2::new(self.min.x + x_off, self.min.y + y_off), size: (s) }
    }

    fn dist2_to(&self, p: Vec2) -> f32 {
        let dx = (self.min.x - p.x).max(p.x - (self.min.x + self.size)).max(0.0);
        let dy = (self.min.y - p.y).max(p.y - (self.min.y + self.size)).max(0.0);
        dx * dx + dy * dy
    }
}

#[derive(Clone, Copy, Debug)]
struct Node {
    bounds: Square,
    /// Index of the first of four consecutive children, or `LEAF`.
    first_child: u32,
    /// This node's bodies are `order[start..end]`.
    start: u32,
    end: u32,
    /// Filled in bottom-up after the tree is built.
    mass: f32,
    center_of_mass: Vec2,
    max_radius: f32
}

pub struct QuadTree {
    nodes: Vec<Node>,
    order: Vec<u32>,
    leaf_capacity : usize,
    max_depth : usize
}

impl Node {
    fn new(bounds: Square, start: u32, end: u32) -> Self {
        Self {
            bounds,
            first_child: LEAF,
            start,
            end,
            mass: 0.0,
            center_of_mass: bounds.center(),
            max_radius: 0.0,
        }
    }
}

impl QuadTree {

    pub fn new(leaf_capacity: usize, max_depth: usize) -> Self {
        Self {
            nodes: Vec::new(),
            order: Vec::new(),
            leaf_capacity,
            max_depth
        }
    }

    pub fn build(&mut self, bodies: &[Body]) {
        self.nodes.clear();
        self.order.clear();

        self.order.extend(0..bodies.len() as u32);
        let bounds = bounding_square(bodies);
        self.nodes.push(Node::new(bounds, 0, bodies.len() as u32));
        self.subdivide(0, bodies, 0);
        for i in (0..self.nodes.len()).rev() {
            self.aggregate(i, bodies);
        }
    }

    fn subdivide(&mut self, n: usize, bodies: &[Body], depth: usize) {
        let node = self.nodes[n];
        let count = (node.end - node.start) as usize;
        if count <= self.leaf_capacity || depth >= self.max_depth {
            return;
        }

        let c = node.bounds.center();
        let slice = &mut self.order[node.start as usize..node.end as usize];

        let mid_y = partition(slice, |i| bodies[i as usize].pos.y < c.y);
        let (bottom, top) = slice.split_at_mut(mid_y);
        let mid_bottom = partition(bottom, |i| bodies[i as usize].pos.x < c.x);
        let mid_top = partition(top, |i| bodies[i as usize].pos.x < c.x);

        let s = node.start;
        let cuts = [
            s,
            s + mid_bottom as u32,
            s + mid_y as u32,
            s + (mid_y + mid_top) as u32,
            node.end
        ];

        let first = self.nodes.len() as u32;
        self.nodes[n].first_child = first;
        for q in 0..4 {
            self.nodes.push(Node::new(node.bounds.quadrant(q), cuts[q], cuts[q+1])); 
        }

        for q in 0..4 {
            self.subdivide((first + q) as usize, bodies, depth + 1);
        }
    }

    fn aggregate(&mut self, i: usize, bodies: &[Body]) {
        let node = self.nodes[i];

        let mut mass = 0.0;
        let mut weighted = Vec2::new(0.0, 0.0);
        let mut max_radius: f32 = 0.0;

        if node.first_child == LEAF {
            for &j in &self.order[(node.start as usize)..(node.end as usize)] {
                let b = &bodies[j as usize];
                mass = mass + b.mass;
                weighted = weighted + b.pos * b.mass;
                max_radius = max_radius.max(b.radius);
            }
        } else {
            let first = node.first_child as usize;
            for c in first..first + 4 {
                let child = self.nodes[c];
                mass = mass + child.mass;
                weighted = weighted + child.center_of_mass * child.mass;
                max_radius = max_radius.max(child.max_radius);
            }

        }

        self.nodes[i].mass = mass;
        if mass != 0.0 {
            self.nodes[i].center_of_mass = weighted / mass;
        }
        self.nodes[i].max_radius = max_radius;

    }

    pub fn acceleration(
        &self,
        i: usize,               // which body
        bodies: &[Body],
        theta: f32,
        softening2: f32,
        stack: &mut Vec<u32>,   // scratch space, reused between calls
    ) -> Vec2 {

        if self.nodes.is_empty() {
            return Vec2::new(0.0, 0.0)
        }

        let p = bodies[i].pos;
        let mut acc = Vec2::new(0.0, 0.0);

        stack.clear();
        stack.push(0);
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n as usize];
            if node.mass == 0.0 { continue; }
            if node.first_child == LEAF {
                for &j in &self.order[(node.start as usize)..(node.end as usize)] {
                    if j as usize == i { continue; }
                    let b = &bodies[j as usize];
                    acc = acc + pull(p, b.pos, b.mass, softening2);
                }   
            } else {
                let size = node.bounds.size;
                let d = node.center_of_mass - p;
                let dist2 = d.x * d.x + d.y * d.y;
                if size * size < dist2 * theta * theta {
                    acc = acc + pull(p, node.center_of_mass, node.mass, softening2);
                } else {
                    for c in 0..4 {
                        stack.push(node.first_child + c);
                    }
                }
            }
        }
        acc
    }

    pub fn collision_pairs(
        &self,
        bodies: &[Body],
        stack: &mut Vec<u32>,
        out: &mut Vec<(u32, u32)>,   // filled with (i, j), each pair once, i < j
    ) {
        out.clear();
        if self.nodes.is_empty() { return; }
        for (i, body) in bodies.iter().enumerate() {
            stack.clear();
            stack.push(0);
            while let Some(n) = stack.pop() {
                let node = &self.nodes[n as usize];
                let reach = body.radius + node.max_radius;
                if node.bounds.dist2_to(body.pos) > reach * reach {
                    continue;
                }
                if node.first_child == LEAF {
                    for &j in &self.order[(node.start as usize)..(node.end as usize)] {
                        if j as usize <= i { continue; }
                        let b2 = bodies[j as usize];
                        let d = body.pos - b2.pos;
                        let dist2 = d.x * d.x + d.y * d.y;
                        let r = body.radius + b2.radius;
                        if dist2 < r * r {
                            out.push((i as u32, j));
                        }
                    }  
                } else {
                    for c in 0..4 {
                        stack.push(node.first_child + c);
                    }
                }
            }
        }
    }
}

fn bounding_square(bodies: &[Body]) -> Square {

    if bodies.is_empty() {
        return Square { min: Vec2::new(0.0, 0.0), size: 1.0 };
    }

    let mut mx : f32 = bodies[0].pos.x;
    let mut my : f32 = bodies[0].pos.y;
    let mut ax : f32 = bodies[0].pos.x;
    let mut ay : f32 = bodies[0].pos.y;
    for b in bodies {
        mx = mx.min(b.pos.x);
        my = my.min(b.pos.y);
        ax = ax.max(b.pos.x);
        ay = ay.max(b.pos.y);
    }
    
    let s = (ax - mx).max(ay - my).max(1e-6);

    Square { min: Vec2::new(mx, my), size: s }
}

pub fn partition(s: &mut [u32], pred: impl Fn(u32) -> bool) -> usize {
    let mut k = 0; // current index 
    for i in 0..s.len() {
        if pred(s[i]) {
            s.swap(i, k);
            k += 1;
        }
    }
    k
}

fn pull(from: Vec2, to: Vec2, mass: f32, softening2: f32) -> Vec2 {
    let d = to - from;
    let r2 = d.x*d.x + d.y*d.y + softening2;
    d * (mass / (r2 * r2.sqrt()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_puts_matches_first() {
        let mut s: Vec<u32> = (1..=10).collect();
        let k = partition(&mut s, |i| i % 2 == 0);
        assert_eq!(k, 5);
        assert!(s[..k].iter().all(|i| i % 2 == 0));
        assert!(s[k..].iter().all(|i| i % 2 == 1));
    }

    fn body(x: f32, y: f32) -> Body {
        Body {
            pos: Vec2::new(x, y),
            vel: Vec2::new(0.0, 0.0),
            mass: 1.0,
            radius: 0.01,
            density: 1.0 / (0.01 * 0.01 * 0.01),
        }
    }

    /// The bodies in node `n`, sorted, since partition doesn't keep their order.
    fn bodies_in(tree: &QuadTree, n: usize) -> Vec<u32> {
        let node = tree.nodes[n];
        let mut ids = tree.order[node.start as usize..node.end as usize].to_vec();
        ids.sort();
        ids
    }

    #[test]
    fn subdivide_splits_into_quadrants() {
        // The worked example: 0 TL, 1 BR, 2 BL, 3 TR, 4 BL, 5 TR
        let bodies = [
            body(-0.5, 0.5),
            body(0.5, -0.5),
            body(-0.5, -0.5),
            body(0.5, 0.5),
            body(-0.2, -0.7),
            body(0.3, 0.6),
        ];
        let mut tree = QuadTree::new(2, 8);
        tree.build(&bodies);

        assert_eq!(tree.nodes.len(), 5, "root plus four leaves");
        assert_eq!(tree.nodes[0].first_child, 1);

        let ranges: Vec<(u32, u32)> = (1..5).map(|c| (tree.nodes[c].start, tree.nodes[c].end)).collect();
        assert_eq!(ranges, [(0, 2), (2, 3), (3, 4), (4, 6)]);

        assert_eq!(bodies_in(&tree, 1), [2, 4], "bottom-left");
        assert_eq!(bodies_in(&tree, 2), [1], "bottom-right");
        assert_eq!(bodies_in(&tree, 3), [0], "top-left");
        assert_eq!(bodies_in(&tree, 4), [3, 5], "top-right");

        for c in 1..5 {
            assert_eq!(tree.nodes[c].first_child, LEAF, "node {c} should be a leaf");
        }
    }

    #[test]
    fn subdivide_leaves_small_nodes_alone() {
        // Fewer bodies than leaf_capacity: the root should stay a single leaf.
        let bodies = [body(0.1, 0.1), body(0.9, 0.9)];
        let mut tree = QuadTree::new(4, 4);
        tree.build(&bodies);

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].first_child, LEAF);
    }

    #[test]
    fn subdivide_stops_at_max_depth() {
        // Identical positions can never be separated, so only max_depth stops the splitting.
        // Each level splits one node into four, all bodies landing in the same child.
        let bodies = vec![body(0.3, 0.3); 10];
        let mut tree = QuadTree::new(2, 5);
        tree.build(&bodies);

        assert_eq!(tree.nodes.len(), 1 + 4 * 5);
    }

    #[test]
    fn subdivide_handles_no_bodies() {
        let mut tree = QuadTree::new(2, 8);
        tree.build(&[]);

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!((tree.nodes[0].start, tree.nodes[0].end), (0, 0));
    }

    #[test]
    fn subdivide_keeps_tree_consistent() {
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};

        let mut rng = StdRng::seed_from_u64(7);
        let bodies: Vec<Body> = (0..500)
            .map(|_| body(rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0)))
            .collect();
        let leaf_capacity = 4;
        let mut tree = QuadTree::new(leaf_capacity, 12);
        tree.build(&bodies);

        // `order` still holds every body exactly once
        let mut all = tree.order.clone();
        all.sort();
        assert_eq!(all, (0..bodies.len() as u32).collect::<Vec<_>>());

        for (n, node) in tree.nodes.iter().enumerate() {
            if node.first_child == LEAF {
                assert!(
                    (node.end - node.start) as usize <= leaf_capacity,
                    "leaf {n} has too many bodies"
                );
                // every body sits inside its leaf's square
                for &b in &tree.order[node.start as usize..node.end as usize] {
                    let p = bodies[b as usize].pos;
                    let min = node.bounds.min;
                    let max = min.x + node.bounds.size;
                    let max_y = min.y + node.bounds.size;
                    assert!(
                        p.x >= min.x && p.x <= max && p.y >= min.y && p.y <= max_y,
                        "body {b} is outside leaf {n}"
                    );
                }
            } else {
                // the four children's ranges join up exactly to cover the parent's range
                let first = node.first_child as usize;
                let children = &tree.nodes[first..first + 4];
                assert_eq!(children[0].start, node.start, "node {n}");
                for q in 0..3 {
                    assert_eq!(children[q].end, children[q + 1].start, "node {n}");
                }
                assert_eq!(children[3].end, node.end, "node {n}");
            }
        }
    }

    fn body_with(x: f32, y: f32, mass: f32, radius: f32) -> Body {
        Body {
            pos: Vec2::new(x, y),
            vel: Vec2::new(0.0, 0.0),
            mass,
            radius,
            density: mass / radius.powi(3),
        }
    }

    /// Equal up to a small relative error; summing in a different order rounds differently.
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4 * a.abs().max(b.abs()).max(1.0)
    }

    /// Mass, center of mass and largest radius computed straight from the bodies, without the tree.
    fn direct_totals(bodies: &[Body], ids: &[u32]) -> (f32, Vec2, f32) {
        let mut mass = 0.0;
        let (mut wx, mut wy) = (0.0, 0.0);
        let mut max_radius: f32 = 0.0;
        for &j in ids {
            let b = &bodies[j as usize];
            mass += b.mass;
            wx += b.pos.x * b.mass;
            wy += b.pos.y * b.mass;
            max_radius = max_radius.max(b.radius);
        }
        (mass, Vec2::new(wx / mass, wy / mass), max_radius)
    }

    fn random_weighted_bodies(count: usize, seed: u64) -> Vec<Body> {
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};

        let mut rng = StdRng::seed_from_u64(seed);
        (0..count)
            .map(|_| {
                body_with(
                    rng.random_range(-1.0..1.0),
                    rng.random_range(-1.0..1.0),
                    rng.random_range(0.1..5.0),
                    rng.random_range(0.001..0.05),
                )
            })
            .collect()
    }

    #[test]
    fn aggregate_weights_by_mass() {
        // mass 3 at x = 0 and mass 1 at x = 4: total 4, center of mass at x = (3·0 + 1·4) / 4 = 1.
        // leaf_capacity 1 forces a split, so the root has to combine its children.
        let bodies = [body_with(0.0, 0.0, 3.0, 0.02), body_with(4.0, 0.0, 1.0, 0.05)];
        let mut tree = QuadTree::new(1, 8);
        tree.build(&bodies);

        let root = tree.nodes[0];
        assert_ne!(root.first_child, LEAF, "root should have split");
        assert!(close(root.mass, 4.0), "mass {}", root.mass);
        assert!(close(root.center_of_mass.x, 1.0), "com x {}", root.center_of_mass.x);
        assert!(close(root.center_of_mass.y, 0.0), "com y {}", root.center_of_mass.y);
        assert!(close(root.max_radius, 0.05), "max_radius {}", root.max_radius);
    }

    #[test]
    fn aggregate_root_holds_totals() {
        let bodies = random_weighted_bodies(300, 11);
        let mut tree = QuadTree::new(4, 12);
        tree.build(&bodies);

        let all: Vec<u32> = (0..bodies.len() as u32).collect();
        let (mass, com, max_radius) = direct_totals(&bodies, &all);
        let root = tree.nodes[0];

        assert!(close(root.mass, mass), "mass {} vs {}", root.mass, mass);
        assert!(close(root.center_of_mass.x, com.x), "com x {} vs {}", root.center_of_mass.x, com.x);
        assert!(close(root.center_of_mass.y, com.y), "com y {} vs {}", root.center_of_mass.y, com.y);
        assert_eq!(root.max_radius, max_radius);
    }

    #[test]
    fn aggregate_every_node_matches_its_bodies() {
        // Internal nodes combine their children; this checks that gives the same
        // answer as summing every body underneath them directly.
        let bodies = random_weighted_bodies(500, 12);
        let mut tree = QuadTree::new(4, 12);
        tree.build(&bodies);

        for (n, node) in tree.nodes.iter().enumerate() {
            let ids = &tree.order[node.start as usize..node.end as usize];
            if ids.is_empty() {
                continue; // covered by aggregate_empty_nodes_are_safe
            }
            let (mass, com, max_radius) = direct_totals(&bodies, ids);
            assert!(close(node.mass, mass), "node {n}: mass {} vs {}", node.mass, mass);
            assert!(close(node.center_of_mass.x, com.x), "node {n}: com x {} vs {}", node.center_of_mass.x, com.x);
            assert!(close(node.center_of_mass.y, com.y), "node {n}: com y {} vs {}", node.center_of_mass.y, com.y);
            assert_eq!(node.max_radius, max_radius, "node {n}");
        }
    }

    #[test]
    fn aggregate_empty_nodes_are_safe() {
        // The worked example without the bottom-right body, so that quadrant is empty.
        let bodies = [
            body(-0.5, 0.5),
            body(-0.5, -0.5),
            body(0.5, 0.5),
            body(-0.2, -0.7),
            body(0.3, 0.6),
        ];
        let mut tree = QuadTree::new(2, 8);
        tree.build(&bodies);

        assert_eq!(tree.nodes.len(), 5, "root plus four leaves");
        let bottom_right = tree.nodes[2];
        assert_eq!(bottom_right.start, bottom_right.end, "bottom-right should be empty");
        assert_eq!(bottom_right.mass, 0.0);
        assert_eq!(bottom_right.max_radius, 0.0);
        assert!(!bottom_right.center_of_mass.x.is_nan() && !bottom_right.center_of_mass.y.is_nan());

        // an empty node must not distort its parent
        assert!(close(tree.nodes[0].mass, 5.0));
    }

    #[test]
    fn aggregate_no_bodies() {
        let mut tree = QuadTree::new(2, 8);
        tree.build(&[]);

        let root = tree.nodes[0];
        assert_eq!(root.mass, 0.0);
        assert!(!root.center_of_mass.x.is_nan() && !root.center_of_mass.y.is_nan());
    }

    const SOFTENING2: f32 = 1e-4;

    fn length(v: Vec2) -> f32 {
        (v.x * v.x + v.y * v.y).sqrt()
    }

    /// The exact O(n²) answer: every other body pulls on body i directly.
    fn brute_force_acceleration(i: usize, bodies: &[Body], softening2: f32) -> Vec2 {
        let mut acc = Vec2::new(0.0, 0.0);
        for (j, other) in bodies.iter().enumerate() {
            if j != i {
                acc = acc + pull(bodies[i].pos, other.pos, other.mass, softening2);
            }
        }
        acc
    }

    #[test]
    fn acceleration_two_bodies_by_hand() {
        // Body 1 (mass 1) sits 1 unit to the right of body 0 (mass 2).
        let bodies = [body_with(0.0, 0.0, 2.0, 0.01), body_with(1.0, 0.0, 1.0, 0.01)];
        let mut tree = QuadTree::new(1, 8);
        tree.build(&bodies);
        let mut stack = Vec::new();

        // size = mass / (r² + ε²)^1.5, pointing toward the other body
        let r2: f32 = 1.0 + SOFTENING2;
        let a0 = tree.acceleration(0, &bodies, 0.5, SOFTENING2, &mut stack);
        let a1 = tree.acceleration(1, &bodies, 0.5, SOFTENING2, &mut stack);

        assert!(close(a0.x, 1.0 / r2.powf(1.5)), "a0.x {}", a0.x);
        assert!(close(a0.y, 0.0), "a0.y {}", a0.y);
        assert!(close(a1.x, -2.0 / r2.powf(1.5)), "a1.x {}", a1.x);
        assert!(close(a1.y, 0.0), "a1.y {}", a1.y);
    }

    #[test]
    fn acceleration_single_body_is_zero() {
        let bodies = [body(0.3, 0.3)];
        let mut tree = QuadTree::new(4, 8);
        tree.build(&bodies);

        let a = tree.acceleration(0, &bodies, 0.5, SOFTENING2, &mut Vec::new());
        assert_eq!((a.x, a.y), (0.0, 0.0));
    }

    #[test]
    fn acceleration_is_equal_and_opposite() {
        // Newton's third law: m₀·a₀ = −m₁·a₁
        let bodies = [body_with(-0.4, 0.2, 3.0, 0.01), body_with(0.5, -0.3, 0.5, 0.01)];
        let mut tree = QuadTree::new(1, 8);
        tree.build(&bodies);
        let mut stack = Vec::new();

        let f0 = tree.acceleration(0, &bodies, 0.5, SOFTENING2, &mut stack) * bodies[0].mass;
        let f1 = tree.acceleration(1, &bodies, 0.5, SOFTENING2, &mut stack) * bodies[1].mass;

        assert!(close(f0.x, -f1.x), "x: {} vs {}", f0.x, -f1.x);
        assert!(close(f0.y, -f1.y), "y: {} vs {}", f0.y, -f1.y);
    }

    #[test]
    fn acceleration_theta_zero_matches_brute_force() {
        // θ = 0 never approximates, so any mismatch means the walk missed or
        // double-counted a body.
        let bodies = random_weighted_bodies(300, 21);
        let mut tree = QuadTree::new(4, 12);
        tree.build(&bodies);
        let mut stack = Vec::new();

        for i in 0..bodies.len() {
            let fast = tree.acceleration(i, &bodies, 0.0, SOFTENING2, &mut stack);
            let exact = brute_force_acceleration(i, &bodies, SOFTENING2);
            assert!(
                length(fast - exact) <= 1e-3 * length(exact).max(1.0),
                "body {i}: tree ({}, {}) vs exact ({}, {})",
                fast.x, fast.y, exact.x, exact.y
            );
        }
    }

    #[test]
    fn acceleration_approximation_stays_close() {
        let bodies = random_weighted_bodies(1000, 22);
        let mut tree = QuadTree::new(8, 12);
        tree.build(&bodies);
        let mut stack = Vec::new();

        let mut total_error = 0.0;
        for i in 0..bodies.len() {
            let fast = tree.acceleration(i, &bodies, 0.5, SOFTENING2, &mut stack);
            let exact = brute_force_acceleration(i, &bodies, SOFTENING2);
            total_error += length(fast - exact) / length(exact);
        }
        let mean_error = total_error / bodies.len() as f32;
        assert!(mean_error < 0.01, "mean relative error {mean_error}");
    }

    #[test]
    fn acceleration_far_cluster_is_approximated() {
        // A tight cluster far from body 0 should be treated as one mass at its
        // center of mass, which is extremely close to the exact sum.
        let mut bodies = vec![body_with(0.0, 0.0, 1.0, 0.01)];
        for k in 0..20 {
            let dx = (k % 5) as f32 * 0.01;
            let dy = (k / 5) as f32 * 0.01;
            bodies.push(body_with(10.0 + dx, 10.0 + dy, 1.0, 0.01));
        }
        let mut tree = QuadTree::new(4, 12);
        tree.build(&bodies);

        let fast = tree.acceleration(0, &bodies, 0.5, SOFTENING2, &mut Vec::new());
        let exact = brute_force_acceleration(0, &bodies, SOFTENING2);
        assert!(length(fast - exact) / length(exact) < 1e-3);
    }

    #[test]
    fn dist2_to_square() {
        let sq = Square { min: Vec2::new(0.0, 0.0), size: 1.0 };

        assert_eq!(sq.dist2_to(Vec2::new(0.5, 0.5)), 0.0, "inside");
        assert_eq!(sq.dist2_to(Vec2::new(1.0, 0.3)), 0.0, "on the edge");
        assert_eq!(sq.dist2_to(Vec2::new(-2.0, 0.5)), 4.0, "left by 2");
        assert_eq!(sq.dist2_to(Vec2::new(0.5, 3.0)), 4.0, "above by 2");
        assert_eq!(sq.dist2_to(Vec2::new(4.0, 5.0)), 25.0, "off the top-right corner by (3, 4)");
        assert_eq!(sq.dist2_to(Vec2::new(-3.0, -4.0)), 25.0, "off the bottom-left corner by (3, 4)");
    }

    /// Every overlapping pair (i, j) with i < j, by checking all of them.
    fn brute_force_pairs(bodies: &[Body]) -> Vec<(u32, u32)> {
        let mut pairs = Vec::new();
        for i in 0..bodies.len() {
            for j in i + 1..bodies.len() {
                let d = bodies[j].pos - bodies[i].pos;
                let r = bodies[i].radius + bodies[j].radius;
                if d.x * d.x + d.y * d.y < r * r {
                    pairs.push((i as u32, j as u32));
                }
            }
        }
        pairs
    }

    fn tree_pairs(bodies: &[Body], leaf_capacity: usize) -> Vec<(u32, u32)> {
        let mut tree = QuadTree::new(leaf_capacity, 12);
        tree.build(bodies);
        let mut pairs = Vec::new();
        tree.collision_pairs(bodies, &mut Vec::new(), &mut pairs);
        pairs.sort();
        pairs
    }

    #[test]
    fn collision_overlapping_pair_is_found() {
        // centres 0.15 apart, radii add up to 0.2
        let bodies = [body_with(0.0, 0.0, 1.0, 0.1), body_with(0.15, 0.0, 1.0, 0.1)];
        assert_eq!(tree_pairs(&bodies, 1), [(0, 1)]);
    }

    #[test]
    fn collision_separate_pair_is_not_found() {
        // centres 0.25 apart, radii add up to 0.2
        let bodies = [body_with(0.0, 0.0, 1.0, 0.1), body_with(0.25, 0.0, 1.0, 0.1)];
        assert_eq!(tree_pairs(&bodies, 1), []);
    }

    #[test]
    fn collision_large_body_reaches_into_other_nodes() {
        // The big body's centre is in a different leaf from the small one, but its
        // edge reaches into that leaf. Only max_radius in the skip rule finds this.
        let bodies = [
            body_with(0.0, 0.0, 1.0, 0.5),
            body_with(0.4, 0.0, 1.0, 0.001),
            body_with(1.0, 1.0, 1.0, 0.001), // far away, stretches the tree
            body_with(1.0, 0.0, 1.0, 0.001),
        ];
        assert_eq!(tree_pairs(&bodies, 1), [(0, 1)]);
    }

    #[test]
    fn collision_pairs_match_brute_force() {
        let bodies = random_weighted_bodies(500, 31);
        let expected = brute_force_pairs(&bodies);
        assert!(!expected.is_empty(), "test needs some overlapping bodies");

        assert_eq!(tree_pairs(&bodies, 4), expected);
    }

    #[test]
    fn collision_pairs_are_ordered_and_unique() {
        let bodies = random_weighted_bodies(500, 32);
        let pairs = tree_pairs(&bodies, 4);

        assert!(pairs.iter().all(|&(i, j)| i < j), "every pair should have i < j");
        let mut unique = pairs.clone();
        unique.dedup();
        assert_eq!(unique.len(), pairs.len(), "no pair should appear twice");
    }

    #[test]
    fn collision_no_bodies() {
        assert_eq!(tree_pairs(&[], 4), []);
    }
}
