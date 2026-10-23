@group(0) @binding(0) var myTexture: texture_2d<f32>;
@group(0) @binding(1) var mySampler: sampler;
@group(1) @binding(0) var<uniform> view_projection: mat4x4<f32>;

// Each particle is drawn twice from the same instance data:
//  1. Glow pass (vs_glow / fs_glow): the full square, lit with a coloured glow that
//     starts at the body's edge and fades smoothly to zero before the square's edge.
//     This pass is additive, so overlapping glows brighten each other.
//  2. Core pass (vs_core / fs_core): a small square around the body, drawn as a solid
//     dark disc. This pass is alpha blended after every glow, so the core blocks the
//     light behind it rather than adding to it.
//
// Distances below are measured in "quad units": 0 at the particle's centre, 1 at the
// middle of the glow square's edge.

// The particle's physical (collision) radius in quad units. The quad is
// radius * GLOW_SCALE wide (renderer.rs), so this is 2 / GLOW_SCALE; keep them in sync.
const BODY_RADIUS: f32 = 0.10;
// The core's colour. Its edge is anti-aliased over about one screen pixel (via fwidth),
// so it stays crisp at any resolution or zoom.
const CORE_COLOR: vec3<f32> = vec3<f32>(0.01, 0.008, 0.015);
// Half-width of the core pass's square, as a multiple of BODY_RADIUS. Just over 1 so the
// anti-aliased edge is not clipped.
const CORE_QUAD_SCALE: f32 = 1.25;
// Brightness of the glow at the body's edge.
const HALO_STRENGTH: f32 = 0.5;
// How fast the glow fades with distance from the edge. Larger = a tighter glow.
const HALO_FALLOFF: f32 = 120.0;
// The glow shifts from a light violet at the body's edge to deep purple further out.
// Both are multiplied by the particle's own colour.
const HALO_INNER_TINT: vec3<f32> = vec3<f32>(0.52, 0.33, 0.75);
const HALO_EDGE_TINT: vec3<f32> = vec3<f32>(0.22, 0.03, 0.49);
// Distance at which the glow has fully reached HALO_EDGE_TINT.
const EDGE_TINT_DISTANCE: f32 = 0.5;

struct Vertex {
    // Mesh Vertex data (slot 0)
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,

    // Instance data (slot 1)
    @location(2) inst_color: vec3<f32>,
    @location(3) offset: vec2<f32>,
    @location(4) size: f32,
};

struct VertexPayload {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    // position inside the quad, from -1 to 1 on each axis
    @location(1) uv: vec2<f32>,
};

// Places the unit quad, scaled by `scale` relative to the glow square.
fn place_quad(vertex: Vertex, scale: f32) -> VertexPayload {
    var out: VertexPayload;

    out.position = vec4<f32>(vertex.position * vertex.size * scale, 1.0);
    out.position += vec4<f32>(vertex.offset, 0.0, 0.0);
    out.position = view_projection * out.position;

    // The quad's corners are at ±0.5; doubling gives ±1 at the glow square's edge, so
    // uv stays in quad units whatever the scale.
    out.uv = vertex.position.xy * 2.0 * scale;
    out.color = vertex.inst_color;

    return out;
}

@vertex
fn vs_glow(vertex: Vertex) -> VertexPayload {
    return place_quad(vertex, 1.0);
}

@vertex
fn vs_core(vertex: Vertex) -> VertexPayload {
    return place_quad(vertex, BODY_RADIUS * CORE_QUAD_SCALE);
}

@fragment
fn fs_glow(in: VertexPayload) -> @location(0) vec4f {
    let d2 = dot(in.uv, in.uv); // squared distance from the centre
    let d = sqrt(d2);

    // Glow: falls off like 1 / distance² from the body's edge, the way real light does.
    // The "1 +" keeps it finite at the edge. Inside the body it stays at full strength,
    // but the core pass covers that part.
    let e = max(d - BODY_RADIUS, 0.0);
    let glow = HALO_STRENGTH / (1.0 + HALO_FALLOFF * e * e);

    // 1 / distance² never actually reaches zero, which would leave a visible square
    // outline. This factor brings it smoothly down to exactly 0 at distance 1.
    let fade = saturate(1.0 - d2);
    let halo = glow * fade * fade;

    // Violet at the body's edge, deep purple out in the glow.
    let t = smoothstep(BODY_RADIUS, EDGE_TINT_DISTANCE, d);
    let halo_color = mix(HALO_INNER_TINT, HALO_EDGE_TINT, t);

    return vec4f(in.color * halo_color * halo, 1.0);
}

@fragment
fn fs_core(in: VertexPayload) -> @location(0) vec4f {
    let d = length(in.uv);

    // Solid disc with a one-pixel anti-aliased edge. fwidth gives how much d changes
    // across one pixel, so the edge width is fixed in pixels.
    let aa = fwidth(d);
    let coverage = 1.0 - smoothstep(BODY_RADIUS - aa, BODY_RADIUS + aa, d);

    // Premultiplied alpha: the core replaces what is behind it by its coverage.
    return vec4f(CORE_COLOR * coverage, coverage);
}
