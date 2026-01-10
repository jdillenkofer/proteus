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

float rand(vec2 co){
    return fract(sin(dot(co, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    
    mask = smoothstep(0.4, 0.6, mask);

    // Glitch Calculations
    vec2 uv = v_tex_coords;
    
    // 1. Horizontal Tearing
    float split_prob = rand(vec2(floor(uv.y * 20.0), floor(time * 20.0)));
    float split_offset = 0.0;
    if (split_prob > 0.8) {
        split_offset = (rand(vec2(time)) - 0.5) * 0.1;
    }
    uv.x += split_offset;
    
    // 2. Chromatic Aberration
    float aber_strength = 0.02;
    float r = texture(sampler2D(t_texture, s_sampler), uv + vec2(aber_strength, 0.0)).r;
    float g = texture(sampler2D(t_texture, s_sampler), uv).g;
    float b = texture(sampler2D(t_texture, s_sampler), uv - vec2(aber_strength, 0.0)).b;
    
    vec3 glitch_color = vec3(r, g, b);
    
    // 3. Random noise block overlay
    float block_noise = rand(vec2(floor(uv.x * 10.0), floor(uv.y * 10.0)) + time);
    if (block_noise > 0.95) {
        glitch_color *= 1.5; // Brighten random blocks
    }

    // Mix Glitched Background with Stable Person
    f_color = vec4(mix(glitch_color, person_color.rgb, mask), 1.0);
}
