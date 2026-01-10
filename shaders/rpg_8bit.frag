#version 450

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask; // Using mask for outline

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

// Simple quantization to limited palette
vec3 quantize(vec3 color, float levels) {
    return floor(color * levels) / levels;
}

void main() {
    // 1. Pixelation
    float pixel_size = 6.0; // Moderate chunky pixels
    vec2 dims = vec2(width, height);
    vec2 uv = floor(v_tex_coords * dims / pixel_size) * pixel_size / dims;
    
    vec4 color = texture(sampler2D(t_texture, s_sampler), uv);
    float mask = texture(sampler2D(t_mask, s_sampler), uv).r;

    // 2. Reduce Color Palette (16-bit simplistic look)
    // Reduce R, G, B channels to fewer levels
    // e.g. 4 levels per channel = 64 colors total roughly
    vec3 posterized = quantize(color.rgb, 4.0);
    
    // Boost saturation slightly to make it look "cartoon/gamey"
    vec3 gray = vec3(dot(posterized, vec3(0.299, 0.587, 0.114)));
    vec3 saturated = mix(gray, posterized, 1.3);
    
    // 3. Outline around the person (Sprite outline)
    // Gradient based outline on mask
    float d = pixel_size / width; // Use pixel size for thickness
    float m_up = texture(sampler2D(t_mask, s_sampler), uv + vec2(0.0, d)).r;
    float m_down = texture(sampler2D(t_mask, s_sampler), uv - vec2(0.0, d)).r;
    float m_left = texture(sampler2D(t_mask, s_sampler), uv - vec2(d, 0.0)).r;
    float m_right = texture(sampler2D(t_mask, s_sampler), uv + vec2(d, 0.0)).r;
    
    float edge = step(0.5, abs(m_up - m_down) + abs(m_left - m_right));
    
    // If edge is high, make it black outline
    vec3 final_color = mix(saturated, vec3(0.0, 0.0, 0.0), edge);
    
    // Apply mask logic? Actually, we want the whole scene to look like a game match.
    // Maybe keep background but slightly darkened?
    // Or just treat person as a Sprite and rest as Background layer.
    
    // Let's just output the whole processed frame, but the outline helps separate "Sprite" (person) from "Background"
    
    f_color = vec4(final_color, 1.0);
}
