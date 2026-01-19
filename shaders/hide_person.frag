#version 450

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask;

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;
layout(location=1) out float f_mask_out;

void main() {
    f_color = vec4(0.0, 0.0, 0.0, 1.0);
    f_mask_out = 0.0;
}
