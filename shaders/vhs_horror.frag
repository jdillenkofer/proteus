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
layout(location=1) out float f_mask_out;

float rand(vec2 co) {
    return fract(sin(dot(co.xy ,vec2(12.9898,78.233))) * 43758.5453);
}

float noise(vec2 p) {
    vec2 ip = floor(p);
    vec2 u = fract(p);
    u = u*u*(3.0-2.0*u);
    float res = mix(
        mix(rand(ip), rand(ip+vec2(1.0,0.0)), u.x),
        mix(rand(ip+vec2(0.0,1.0)), rand(ip+vec2(1.0,1.0)), u.x), u.y);
    return res*res;
}

void main() {
    vec2 uv = v_tex_coords;
    
    // 1. V-Hold / Vertical Roll
    // Shifts y coordinate over time, making the screen "roll"
    float roll_speed = 0.3;
    float roll = fract(time * roll_speed);
    uv.y = fract(uv.y + roll);
    
    // Create the "roll bar" (black sync bar)
    // It exists at the wrap-around point of the UVs
    // Since we offset by +roll, the wrap happens where uv.y + roll > 1.0.
    // Effectively, we just darken the top/bottom edges of the rolled UV.
    float roll_bar = smoothstep(0.0, 0.05, uv.y) * smoothstep(1.0, 0.95, uv.y);
    // Actually, "roll_bar" should be dark, so we want 1.0 in center and 0.0 at edges.
    
    // 2. Aggressive Horizontal Tear
    // Random chance to offset a chunk of lines horizontally
    float tear_thresh = 0.95;
    if (rand(vec2(floor(time * 10.0), uv.y)) > tear_thresh) {
        uv.x += (rand(vec2(time, uv.y)) - 0.5) * 0.1; // 10% shift
    }
    
    // 3. Chromatic Aberration (RGB Split)
    // Dynamic split based on distance from center + noise
    float split_amt = 0.005 + noise(vec2(time*20.0, 0.0)) * 0.015; 
    
    float r = texture(sampler2D(t_texture, s_sampler), uv + vec2(split_amt, 0.0)).r;
    float g = texture(sampler2D(t_texture, s_sampler), uv).g;
    float b = texture(sampler2D(t_texture, s_sampler), uv - vec2(split_amt, 0.0)).b;
    
    vec3 color = vec3(r, g, b);
    
    // 4. Color Grading: High Contrast + Red Tint
    color = pow(color, vec3(1.5)); // Contrast
    color *= vec3(1.1, 0.95, 0.95); // Slight red warmer tint
    
    // 5. Interlacing / Scanlines
    // Darken every alternate 2 pixels
    if (mod(gl_FragCoord.y, 4.0) < 2.0) {
        color *= 0.8; 
    }
    
    // 6. Apply Roll Bar (Darken)
    color *= roll_bar;

    // 7. Random Glitch Blocks (Data Loss)
    float block_noise = noise(floor(uv * vec2(8.0, 8.0)) + floor(time * 15.0));
    if (block_noise > 0.96) { // Less frequent
        color = vec3(0.0); // Drop out to black (signal loss)
    }
    
    // 8. Static Noise Overlay (Gritty)
    float grain = rand(uv * time * 2.0) * 0.25; // Sharper grain
    color += grain;
    
    // 9. Claustrophobic Vignette
    // Make corners very dark
    float d_vig = distance(uv, vec2(0.5));
    color *= smoothstep(0.8, 0.3, d_vig);

    f_color = vec4(color, 1.0);
    f_mask_out = texture(sampler2D(t_mask, s_sampler), uv).r;
}
