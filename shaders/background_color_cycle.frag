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

// Helper to convert HSV to RGB
vec3 hsv2rgb(vec3 c) {
    vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

void main() {
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    
    // Cycle hue based on time (slow cycle)
    float hue = fract(time * 0.1); 
    vec3 rainbow = hsv2rgb(vec3(hue, 0.8, 0.8)); // High saturation/value
    vec4 background_color = vec4(rainbow, 1.0);

    // Smoothstep for cleaner edges
    // Adjusted to be more inclusive of the person (0.01 instead of 0.1)
    mask = smoothstep(0.4, 0.6, mask);

    f_color = mix(background_color, person_color, mask);
}
