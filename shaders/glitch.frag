#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;

layout(set = 0, binding = 2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};

// Glitch Effect Shader
// Simulates digital video corruption, RGB splitting, and block displacement

// Random number generation
float random(vec2 st) {
    return fract(sin(dot(st.xy + seed, vec2(12.9898, 78.233))) * 43758.5453123);
}

// Value noise
float noise(vec2 st) {
    vec2 i = floor(st);
    vec2 f = fract(st);
    float a = random(i);
    float b = random(i + vec2(1.0, 0.0));
    float c = random(i + vec2(0.0, 1.0));
    float d = random(i + vec2(1.0, 1.0));
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}

void main() {
    vec2 uv = tex_coords;
    
    // Time-based glitch intensity
    // Glitch happens more intensely in bursts
    float glitch_strength = 0.0;
    
    // Periodic major glitches
    float t_mod = mod(time, 3.0);
    if (t_mod > 2.8) {
        glitch_strength = 0.8;
    } else if (random(vec2(time, 0.0)) > 0.95) {
        // Random sporadic glitches
        glitch_strength = 0.5;
    }
    
    // Horizontal block displacement
    // Divide screen into blocks
    float blocks = 20.0;
    float block_line = floor(uv.y * blocks);
    
    // Random offset per block based on time
    float block_offset = 0.0;
    if (glitch_strength > 0.0) {
        float r = random(vec2(block_line, floor(time * 20.0)));
        if (r < 0.3 * glitch_strength) {
            block_offset = (r - 0.5) * 0.2 * glitch_strength;
        }
    }
    
    vec2 block_uv = uv;
    block_uv.x += block_offset;
    
    // RGB Split / Chromatic Aberration
    // Separation increases with glitch strength
    float split = 0.005 + 0.05 * glitch_strength * random(vec2(time));
    
    float r = texture(sampler2D(t_texture, s_sampler), block_uv + vec2(split, 0.0)).r;
    float g = texture(sampler2D(t_texture, s_sampler), block_uv).g;
    float b = texture(sampler2D(t_texture, s_sampler), block_uv - vec2(split, 0.0)).b;
    
    // Scanline jitter
    // Shift individual lines horizontally slightly
    if (glitch_strength > 0.5) {
        float jitter = (random(vec2(uv.y, time)) - 0.5) * 0.05 * glitch_strength;
        if (random(vec2(time)) > 0.8) {
             // Only apply sometimes
             r = texture(sampler2D(t_texture, s_sampler), block_uv + vec2(split + jitter, 0.0)).r;
             g = texture(sampler2D(t_texture, s_sampler), block_uv + vec2(jitter, 0.0)).g;
             b = texture(sampler2D(t_texture, s_sampler), block_uv - vec2(split - jitter, 0.0)).b;
        }
    }
    
    vec3 color = vec3(r, g, b);
    
    // Add noise/static
    // More noise during glitches
    float noise_val = random(uv * time);
    if (noise_val > 0.9) {
        color += (noise_val - 0.9) * (0.2 + glitch_strength);
    }
    
    // Occasional color inversion or tint
    if (glitch_strength > 0.7 && random(vec2(time * 10.0)) > 0.9) {
        color = 1.0 - color;
    }

    frag_color = vec4(color, 1.0);
}
