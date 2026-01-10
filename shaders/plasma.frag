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

// Plasma effect shader
// Uses multiple sine waves to create a liquid-like color shifting effect

void main() {
    vec2 uv = tex_coords;
    
    // Scale UVs for the pattern
    vec2 p = -1.0 + 2.0 * uv;
    
    // Calculate plasma value using multiple sine waves
    float v = 0.0;
    
    // Wave 1: Moving diagonally
    v += sin((p.x + time * 0.5));
    
    // Wave 2: Moving vertically with different frequency
    v += sin((p.y + time * 0.2) * 2.0);
    
    // Wave 3: Circular pattern moving around
    v += sin((p.x + p.y + time * 0.9) * 2.0);
    
    // Wave 4: Rotating pattern
    float cx = p.x + 0.5 * sin(time * 0.3);
    float cy = p.y + 0.5 * cos(time * 0.2);
    v += sin(sqrt(cx * cx + cy * cy + 1.0) * 5.0);
    
    // Map the value to colors
    // Shift the value to be positive and scale it
    v = v * 0.5;
    
    // Create color channels based on the plasma value
    float r = sin(v * 3.14159);
    float g = cos(v * 3.14159);
    float b = sin(v * 3.14159 + 3.14159 / 2.0);
    
    // Sample the original texture
    vec4 tex_color = texture(sampler2D(t_texture, s_sampler), uv);
    
    // Mix the plasma effect with the original video
    // Use a blending mode like Overlay or Soft Light for better integration
    vec3 plasma_color = vec3(r, g, b) * 0.5 + 0.5;
    
    // Mix: 50% original, 50% plasma
    vec3 final_color = mix(tex_color.rgb, plasma_color, 0.4);
    
    // preserve alpha
    frag_color = vec4(final_color, tex_color.a);
}
