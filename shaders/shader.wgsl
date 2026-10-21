@group(0) @binding(0) var myTexture: texture_2d<f32>;
@group(0) @binding(1) var mySampler: sampler;
@group(1) @binding(0) var<uniform> view_projection: mat4x4<f32>;

// Each particle is drawn as a square. The fragment shader turns that square into a
// point light: a white-hot centre plus a coloured glow that fades smoothly to zero
// before the square's edge. The pipeline adds every particle's light on top of what is
// already on screen, so overlapping glows brighten each other.
//
// Distances below are measured in "quad units": 0 at the particle's centre, 1 at the
// middle of the square's edge.

// Radius of the white-hot centre. Too small and it drops below one pixel and flickers.
const CORE_RADIUS: f32 = 0.12;
// Brightness of the glow around the centre, relative to the centre's peak of 1.
const HALO_STRENGTH: f32 = 0.35;
// How fast the glow fades. Larger = a tighter glow.
const HALO_FALLOFF: f32 = 60.0;
// Colour the glow is tinted towards, multiplied by the particle's own colour.
const HALO_TINT: vec3<f32> = vec3<f32>(0.55, 0.8, 1.0);

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

@vertex
fn vs_main(vertex: Vertex) -> VertexPayload {
    var out: VertexPayload;

    out.position = vec4<f32>(vertex.position * vertex.size, 1.0);
    out.position += vec4<f32>(vertex.offset, 0.0, 0.0);
    out.position = view_projection * out.position;

    // The quad's corners are at ±0.5; doubling gives ±1 so the edge is at distance 1.
    out.uv = vertex.position.xy * 2.0;
    out.color = vertex.inst_color;

    return out;
}

@fragment
fn fs_main(in: VertexPayload) -> @location(0) vec4f {
    let d2 = dot(in.uv, in.uv); // squared distance from the centre

    // White-hot centre: a Gaussian, 1 at the centre and falling off very quickly.
    let core = exp(-d2 / (CORE_RADIUS * CORE_RADIUS));

    // Glow: falls off like 1 / distance², the way real light does. The "1 +" keeps it
    // finite at the centre.
    let glow = HALO_STRENGTH / (1.0 + HALO_FALLOFF * d2);

    // 1 / distance² never actually reaches zero, which would leave a visible square
    // outline. This factor brings it smoothly down to exactly 0 at distance 1.
    let fade = saturate(1.0 - d2);
    let halo = glow * fade * fade;

    let color = vec3f(1.0) * core + in.color * HALO_TINT * halo;
    return vec4f(color, 1.0);
}
