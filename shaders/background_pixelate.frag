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
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    
    // Pixelation effect for background
    float pixel_size = 75.0; // Size of the blocks
    float dx = pixel_size * (1.0 / width);
    float dy = pixel_size * (1.0 / height);
    
    vec2 coord = vec2(dx * floor(v_tex_coords.x / dx), dy * floor(v_tex_coords.y / dy));
    vec4 background_color = texture(sampler2D(t_texture, s_sampler), coord);

    // Smoothstep for cleaner edges
    mask = smoothstep(0.00, 0.2, mask);

    f_color = mix(background_color, person_color, mask);
}
