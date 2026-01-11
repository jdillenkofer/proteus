#version 450

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;
layout(location=1) out float f_mask_out;

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask;

void main() {
    vec2 center = vec2(0.5, 0.5);
    vec2 p = v_tex_coords - center;
    float r = length(p);
    
    // Breathing distortion
    // 1.0 = no distortion
    // > 1.0 = zoom in / pinch
    // < 1.0 = zoom out / bulge
    float distortion = 1.0 + sin(time * 2.0) * 0.5 * r;
    
    vec2 distorted_uv = center + p * distortion;
    
    // Bounds check to avoid streaking edges
    if (distorted_uv.x < 0.0 || distorted_uv.x > 1.0 || distorted_uv.y < 0.0 || distorted_uv.y > 1.0) {
        f_color = vec4(0.0, 0.0, 0.0, 1.0);
        f_mask_out = 0.0;
    } else {
        f_color = texture(sampler2D(t_texture, s_sampler), distorted_uv);
        f_mask_out = texture(sampler2D(t_mask, s_sampler), distorted_uv).r;
    }
}
