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

void main() {
    vec4 color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;

    // Overlay mask in RED with 50% opacity
    // If mask aligns with person, the person should be tinted red.
    // If mask is flipped, the red tint will be upside down.
    
    vec4 mask_overlay = vec4(1.0, 0.0, 0.0, 1.0);
    
    f_color = mix(color, mask_overlay, mask * 0.5);
}
