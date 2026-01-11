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
    float grid_size = 3.0; // 3x3 grid
    
    // Scale UVs to repeat
    vec2 grid_uv = fract(v_tex_coords * grid_size);
    
    // Sample mask and texture at the repeated UVs
    // This effectively "clones" the input 9 times
    float mask_val = texture(sampler2D(t_mask, s_sampler), grid_uv).r;
    vec4 color = texture(sampler2D(t_texture, s_sampler), grid_uv);
    
    // Output
    f_mask_out = mask_val;
    f_color = color;
}
