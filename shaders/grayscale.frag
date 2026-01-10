#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;

void main() {
    vec4 color = texture(sampler2D(t_texture, s_sampler), tex_coords);
    
    // Convert to grayscale using luminance weights
    float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
    
    frag_color = vec4(gray, gray, gray, color.a);
}
