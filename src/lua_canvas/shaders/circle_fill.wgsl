// Circle fill shader using SDF

struct Uniforms {
    bounds: vec4<f32>,  // cx, cy, radius, unused
    color: vec4<f32>,   // RGBA
    extra: vec4<f32>,   // unused, canvas_width, canvas_height, unused
    extra2: vec4<f32>,  // unused
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = (pos + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

// Circle SDF
fn circle_sdf(p: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return length(p - center) - radius;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let canvas_size = vec2<f32>(uniforms.extra.y, uniforms.extra.z);
    let pixel_pos = in.uv * canvas_size;
    
    let center = vec2<f32>(uniforms.bounds.x, uniforms.bounds.y);
    let radius = uniforms.bounds.z;
    
    let dist = circle_sdf(pixel_pos, center, radius);
    
    // Anti-aliased edge
    let alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
    
    if alpha <= 0.0 {
        discard;
    }
    
    return vec4<f32>(uniforms.color.rgb, uniforms.color.a * alpha);
}
