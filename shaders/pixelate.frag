#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;

const float PIXEL_SIZE = 8.0;  // Size of each "pixel" block

void main() {
    vec2 tex_size = vec2(textureSize(sampler2D(t_texture, s_sampler), 0));
    
    // Calculate pixel block coordinates
    vec2 block = floor(tex_coords * tex_size / PIXEL_SIZE) * PIXEL_SIZE;
    vec2 uv = block / tex_size;
    
    frag_color = texture(sampler2D(t_texture, s_sampler), uv);
}
