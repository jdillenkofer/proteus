// Line shader using SDF

struct Uniforms {
    start_end: vec4<f32>,  // x1, y1, x2, y2
    color: vec4<f32>,      // RGBA
    extra: vec4<f32>,      // stroke_width, canvas_width, canvas_height, unused
    extra2: vec4<f32>,     // unused
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

// Line segment SDF with rounded caps
fn line_sdf(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, width: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - width * 0.5;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let canvas_size = vec2<f32>(uniforms.extra.y, uniforms.extra.z);
    let pixel_pos = in.uv * canvas_size;
    
    let start = vec2<f32>(uniforms.start_end.x, uniforms.start_end.y);
    let end = vec2<f32>(uniforms.start_end.z, uniforms.start_end.w);
    let stroke_width = uniforms.extra.x;
    
    let dist = line_sdf(pixel_pos, start, end, stroke_width);
    
    // Anti-aliased edge
    let alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
    
    if alpha <= 0.0 {
        discard;
    }
    
    return vec4<f32>(uniforms.color.rgb, uniforms.color.a * alpha);
}
