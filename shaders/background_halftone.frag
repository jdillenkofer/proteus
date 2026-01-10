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
    vec4 color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);

    mask = smoothstep(0.00, 0.2, mask);

    // Grid for halftone dots
    float frequency = 100.0;
    vec2 nearest = 2.0 * fract(frequency * v_tex_coords) - 1.0;
    float dist = length(nearest);
    float radius = 0.8; // Dot radius

    // Halftone color calculation
    // Convert background to grayscale for intensity
    float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
    
    // Size of dot depends on darkness (darker = larger dot)
    // Invert gray so dark areas have bigger dots
    float dot_size = (1.0 - gray) * radius * 1.5;
    
    vec3 bg_color;
    if (dist < dot_size) {
        bg_color = vec3(0.1, 0.1, 0.1); // Dark dot
    } else {
        bg_color = vec3(0.9, 0.9, 0.9); // Light paper
    }

    // Mix Halftone Background with Real Person
    f_color = vec4(mix(bg_color, color.rgb, mask), 1.0);
}
