// Glyph rendering shader using an alpha texture atlas
struct Uniforms {
    target_bounds: vec4<f32>,  // x, y, w, h (pixels)
    atlas_bounds: vec4<f32>,   // u, v, w, h (pixels)
    color: vec4<f32>,          // RGBA
    extra: vec4<f32>,          // atlas_w, atlas_h, canvas_w, canvas_h
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var atlas_texture: texture_2d<f32>;
@group(1) @binding(1)
var atlas_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    
    let canvas_size = vec2<f32>(uniforms.extra.z, uniforms.extra.w);
    let target_pos = uniforms.target_bounds.xy;
    let target_size = uniforms.target_bounds.zw;
    
    // Map unit pos (-1 to 1) to (0 to 1)
    let unit_pos = (pos + 1.0) * 0.5;
    
    // Map to pixel position on canvas
    let pixel_pos = target_pos + unit_pos * target_size;
    
    // Convert to NDC (-1 to 1)
    let ndc_x = (pixel_pos.x / canvas_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / canvas_size.y) * 2.0;
    
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = unit_pos;
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Map quad UV to atlas UV
    let atlas_pos = uniforms.atlas_bounds.xy + in.uv * uniforms.atlas_bounds.zw;
    let atlas_uv = atlas_pos / uniforms.extra.xy;
    
    // Sample the alpha value from the atlas
    let atlas_alpha = textureSample(atlas_texture, atlas_sampler, atlas_uv).r;
    
    let final_alpha = uniforms.color.a * atlas_alpha;
    if final_alpha <= 0.0 {
        discard;
    }
    
    return vec4<f32>(uniforms.color.rgb, final_alpha);
}
