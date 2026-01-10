#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;

layout(set = 0, binding = 2) uniform Uniforms {
    float time;
    float width;
    float height;
};

// Psychedelic effect shader
// Uses coordinate distortion and color cycling

void main() {
    vec2 uv = tex_coords;
    
    // Center coordinates
    vec2 p = -1.0 + 2.0 * uv;
    
    // Distortion effect
    // Calculate angle and radius
    float r = length(p);
    float a = atan(p.y, p.x);
    
    // Swirl effect based on radius and time
    float swirl = 2.0 * sin(time * 0.5);
    a += swirl * sin(r * 3.0 - time);
    
    // Convert back to cartesian with distorted angle
    vec2 distorted_p;
    distorted_p.x = r * cos(a);
    distorted_p.y = r * sin(a);
    
    // Map back to 0-1 UV space
    vec2 distorted_uv = distorted_p * 0.5 + 0.5;
    
    // Ensure UVs wrap around or clamp
    // Wrapping creates a Kaleidoscope effect
    distorted_uv = fract(distorted_uv);
    
    // Sample texture with distorted UVs
    vec4 tex_color = texture(sampler2D(t_texture, s_sampler), distorted_uv);
    
    // Color cycling
    // Shift Hue
    vec3 color = tex_color.rgb;
    
    // Simple hue shift approximation
    float shift = time * 0.5;
    vec3 k = vec3(0.57735, 0.57735, 0.57735);
    // Rodrigues rotation formula for hue rotation
    float cos_angle = cos(shift);
    vec3 mixed_color = mix(k * dot(k, color), color, cos_angle) + 
                      cross(k, color) * sin(shift);

    frag_color = vec4(mixed_color, tex_color.a);
}
