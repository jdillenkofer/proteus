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
    float speed = 0.5;
    
    // Shift UVs to the left to move image to the right
    // We use fract to wrap around 0..1 range
    vec2 scroll_uv = vec2(fract(v_tex_coords.x - time * speed), v_tex_coords.y);
    
    // Sample mask and texture at shifted UVs
    float mask_val = texture(sampler2D(t_mask, s_sampler), scroll_uv).r;
    vec4 color = texture(sampler2D(t_texture, s_sampler), scroll_uv);
    
    // Output mask for propagation
    f_mask_out = mask_val;
    
    // Output color masked by the person mask
    // This ensures we scroll a "cutout" of the person, not the whole camera frame
    f_color = vec4(color.rgb * mask_val, 1.0);
}
