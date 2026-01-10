#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;

void main() {
    vec4 color = texture(sampler2D(t_texture, s_sampler), tex_coords);
    
    // Sepia tone matrix
    float r = dot(color.rgb, vec3(0.393, 0.769, 0.189));
    float g = dot(color.rgb, vec3(0.349, 0.686, 0.168));
    float b = dot(color.rgb, vec3(0.272, 0.534, 0.131));
    
    frag_color = vec4(min(r, 1.0), min(g, 1.0), min(b, 1.0), color.a);
}
