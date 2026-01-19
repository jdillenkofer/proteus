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

layout(set = 0, binding = 3) uniform texture2D t_mask;

// CRT effect parameters (tuned for 720p visibility)
const float CURVATURE = 0.08;           // Screen curvature amount
const float SCANLINE_INTENSITY = 0.35;  // Scanline darkness
const float SCANLINE_COUNT = 360.0;     // Number of scanlines (fewer = thicker lines)
const float VIGNETTE_INTENSITY = 0.5;   // Edge darkening
const float BRIGHTNESS = 1.25;          // Overall brightness boost
const float CHROMATIC_ABERRATION = 0.004; // RGB separation amount
const float SCANLINE_SPEED = 2.0;       // Speed of scanline movement

// Apply barrel distortion for CRT curvature
vec2 curve_coords(vec2 uv) {
    uv = uv * 2.0 - 1.0;  // Convert to -1 to 1 range
    vec2 offset = abs(uv.yx) / vec2(6.0, 4.0);
    uv = uv + uv * offset * offset * CURVATURE;
    uv = uv * 0.5 + 0.5;  // Convert back to 0 to 1 range
    return uv;
}

// Check if coordinates are within screen bounds
float inside_screen(vec2 uv) {
    return step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
}

// Generate scanline effect
float scanline(vec2 uv) {
    // Add time-based movement to scanlines
    float line = sin((uv.y + time * SCANLINE_SPEED * 0.01) * SCANLINE_COUNT * 3.14159265);
    return 1.0 - SCANLINE_INTENSITY * (0.5 - 0.5 * line);
}

// Generate vignette effect (darker edges)
float vignette(vec2 uv) {
    uv = uv * 2.0 - 1.0;
    return 1.0 - VIGNETTE_INTENSITY * dot(uv, uv);
}

// Simulate phosphor RGB subpixel pattern
vec3 phosphor_mask(vec2 uv) {
    vec2 tex_size = vec2(textureSize(sampler2D(t_texture, s_sampler), 0));
    float x = mod(uv.x * tex_size.x, 3.0);
    vec3 mask = vec3(1.0);
    if (x < 1.0) {
        mask = vec3(1.0, 0.7, 0.7);
    } else if (x < 2.0) {
        mask = vec3(0.7, 1.0, 0.7);
    } else {
        mask = vec3(0.7, 0.7, 1.0);
    }
    return mix(vec3(1.0), mask, 0.3);  // Subtle effect
}

void main() {
    // 1. Calculate CRT Color (Background)
    vec2 curved_uv = curve_coords(tex_coords);
    float mask_crt = inside_screen(curved_uv);
    
    float r = texture(sampler2D(t_texture, s_sampler), curved_uv + vec2(CHROMATIC_ABERRATION, 0.0)).r;
    float g = texture(sampler2D(t_texture, s_sampler), curved_uv).g;
    float b = texture(sampler2D(t_texture, s_sampler), curved_uv - vec2(CHROMATIC_ABERRATION, 0.0)).b;
    vec3 crt_color = vec3(r, g, b);
    
    crt_color *= phosphor_mask(curved_uv);
    crt_color *= scanline(curved_uv);
    crt_color *= vignette(curved_uv);
    crt_color *= BRIGHTNESS;
    crt_color *= mask_crt; // formatting implies black bars outside CRT
    
    // 2. Fetch Original Color (Person) - No warping, no effects
    vec4 original_color = texture(sampler2D(t_texture, s_sampler), tex_coords);
    
    // 3. Fetch Person Mask
    float person_mask = texture(sampler2D(t_mask, s_sampler), tex_coords).r;
    // person_mask = smoothstep(0.4, 0.6, person_mask); // Optional: Sharpen mask if needed
    
    // 4. Mix: Show Person (Original) on top of CRT (Background)
    vec3 final_rgb = mix(crt_color, original_color.rgb, person_mask);
    
    frag_color = vec4(final_rgb, 1.0);
}
