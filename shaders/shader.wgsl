@group(0) @binding(0) var myTexture: texture_2d<f32>;
@group(0) @binding(1) var mySampler: sampler;
@group(1) @binding(0) var<uniform> view_projection: mat4x4<f32>;

struct Vertex {
    // Mesh Vertex data (slot 0)
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,

    // Instance data (slot 1)
    @location(2) inst_color: vec4<f32>,
    @location(3) offset: vec2<f32>,
    @location(4) size: f32,
};

struct VertexPayload {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) texCoord: vec2<f32>,
};

@vertex
fn vs_main(vertex: Vertex) -> VertexPayload {
    var out: VertexPayload;

    out.position = vec4<f32>(vertex.position * vertex.size, 1.0);
    out.position += vec4<f32>(vertex.offset, 0.0, 0.0);
    out.position = view_projection * out.position;

    out.texCoord = vec2<f32>(vertex.position.x, vertex.position.y);
    out.color = vertex.color; 
    // UV inside the quad, ignoring offset

    return out;
}

@fragment
fn fs_main(in: VertexPayload) -> @location(0) vec4<f32> {
    let dist = length(in.texCoord);

    if dist > 1.0 {
        discard;
    }
    let alpha = smoothstep(1.0, 0.8, dist);
    return vec4<f32>(in.color, 1.0 * alpha);
}
