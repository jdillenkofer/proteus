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

#define RAIN_SPEED 1.5
#define DROP_SIZE  1.2 

// Optimized Fast Rand
float rand(vec2 co) {
    return fract(sin(dot(co, vec2(12.9898, 78.233))) * 43758.5453);
}

// Low-cost procedural character using coordinate logic instead of branching
// This is much faster for GPUs as it avoids branching instructions
float get_glyph(vec2 p, float char_id) {
    float g = 0.0;
    float variant = floor(char_id * 8.0);
    
    // Procedural shape masks
    bool v_bar = p.x == 2.0;
    bool h_bar = p.y == 3.0;
    bool box   = (abs(p.x-2.0) == 1.0 || abs(p.y-3.5) == 2.5);
    bool diag  = abs(p.x - p.y*0.6) < 0.5;
    
    // Combine masks mathematically
    if(variant < 2.0) g = float(v_bar || h_bar); 
    else if(variant < 4.0) g = float(box);
    else if(variant < 6.0) g = float(diag);
    else g = float(rand(p + char_id) > 0.6);
    
    // Padding/Size filter
    g *= float(p.x >= 1.0 && p.x <= 3.0 && p.y >= 1.0 && p.y <= 6.0);
    return g;
}

void main() {
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    mask = smoothstep(0.00, 0.2, mask);

    vec2 uv = v_tex_coords;
    uv.y = 1.0 - uv.y; 
    
    vec2 fragCoord = uv * vec2(width, height);
    float globalTime = time * RAIN_SPEED;

    float char_w = 8.0 * DROP_SIZE; 
    float cell_w = 9.0 * DROP_SIZE; 
    float char_h = char_w * 1.8;
    float cell_h = char_h;

    vec3 result = vec3(0.0);

    // Optimized Two-Pass Rendering
    for(int layer = 0; layer < 2; ++layer) {
        float depth = (layer == 0) ? 1.0 : 0.6;
        float x_off = (layer == 0) ? 0.0 : width * 0.15;
        float speed_mult = (layer == 0) ? 1.0 : 0.7;
        
        vec2 p = fragCoord + vec2(x_off, 0.0);
        float x_idx = floor(p.x / cell_w);
        float x_local = mod(p.x, cell_w);
        
        if (x_local < char_w) {
            float rnd = rand(vec2(x_idx, 123.0 + float(layer)));
            float drop_speed = globalTime * (1.2 + rnd * 1.2) * speed_mult + rnd * 10.0;
            float y_scroll = p.y + drop_speed * 120.0;
            
            float y_idx = floor(y_scroll / cell_h);
            float y_local = mod(y_scroll, cell_h);
            
            if (y_local < char_h) {
                float loop_y = mod(y_idx, 25.0 + rnd * 15.0);
                float trail = 15.0 + rnd * 20.0;
                float signal = max(0.0, 1.0 - loop_y / trail);
                
                if (signal > 0.1) {
                    // Update char ID at a fixed rate
                    float char_id = rand(vec2(x_idx, floor(y_idx + globalTime * 8.0)));
                    float b = get_glyph(floor(vec2(x_local / char_w, y_local / char_h) * vec2(5.0, 8.0)), char_id);
                    
                    vec3 base_col = (layer == 0) ? vec3(0.0, 1.0, 0.3) : vec3(0.0, 0.4, 0.1);
                    float head = (layer == 0 && loop_y < 2.0) ? (2.0 - loop_y) / 2.0 : 0.0;
                    
                    vec3 col = mix(vec3(0.0, 0.1, 0.0), base_col, pow(signal, 1.5));
                    col = mix(col, vec3(0.8, 1.0, 0.9), head); // White head blend
                    col += vec3(0.1, 0.6, 0.2) * head; // Head glow
                    
                    result += col * b * signal * depth;
                }
            }
        }
    }

    result = clamp(result, 0.0, 1.0);
    f_color = vec4(mix(result, person_color.rgb, mask), 1.0);
}
