#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;

const float LEVELS = 4.0;  // Number of color levels per channel

void main() {
    vec4 color = texture(sampler2D(t_texture, s_sampler), tex_coords);
    
    // Reduce color levels for pop-art/comic look
    vec3 posterized = floor(color.rgb * LEVELS) / (LEVELS - 1.0);
    
    frag_color = vec4(posterized, color.a);
}
