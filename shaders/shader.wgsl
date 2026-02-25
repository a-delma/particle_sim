@group(0) @binding(0) var myTexture: texture_2d<f32>;
@group(0) @binding(1) var mySampler: sampler;
@group(1) @binding(0) var<uniform> view_projection: mat4x4<f32>;

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
    @location(1) texCoord: vec2<f32>,
};

@vertex
fn vs_main(vertex: Vertex) -> VertexPayload {
    var out: VertexPayload;

    out.position = vec4<f32>(vertex.position * vertex.size, 1.0);
    out.position += vec4<f32>(vertex.offset, 0.0, 0.0);
    out.position = view_projection * out.position;

    out.texCoord = vec2<f32>(vertex.position.x, vertex.position.y);
    out.color = vertex.inst_color; 
    // UV inside the quad, ignoring offset

    return out;
}

@fragment
fn fs_main(in: VertexPayload) -> @location(0) vec4f {
    // Distance from center of particle (0.5, 0.5)
    let dist = length(in.texCoord);
    
    let innerRadius = 0.05;
    let outerRadius = 0.85;
    let delta = 0.05;
    let innerGlow = 0.5 * (1 + smoothstep(innerRadius, 0, dist));
    let innerStep = 0.5 * smoothstep(innerRadius-delta,innerRadius+delta, dist);
    let innerIntensity = innerGlow - innerStep;

    let outerGlow = smoothstep(innerRadius, outerRadius, dist);
    var outerIntensity = max(innerStep - (outerGlow), 0);
    outerIntensity = outerIntensity * outerIntensity * outerIntensity;

    let innerColor = 0.75 * (in.color * innerIntensity);
    let outerColor = 0.75 * vec3f(0.0, 1.0, 1.0) * outerIntensity;
    
    return vec4f(innerColor + outerColor, innerIntensity + outerIntensity);
}

