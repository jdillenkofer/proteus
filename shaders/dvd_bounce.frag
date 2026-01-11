#version 450

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;
layout(location=1) out float f_mask_out;

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask;

void main() {
    float scale = 0.3; // Person is 30% of screen size
    float aspect = width / height;
    
    // Bounds for the top-left corner of the "person box"
    // The box size is (scale, scale) in UV space? 
    // If we want to preserve aspect ratio of the person, we should scale X and Y differently?
    // Let's just scale strictly in UV space to keep it simple, 
    // so the person shrinks uniformly if the screen is 1:1, or non-uniformly if not? 
    // Actually t_texture is the camera feed. Usually camera feed has physically square pixels but 16:9 ratio.
    // If we scale both U and V by 0.3, the aspect ratio of the "mini person" is the same as the full feed.
    
    vec2 box_size = vec2(scale, scale);
    vec2 max_pos = vec2(1.0) - box_size;
    
    // Bouncing logic using triangle wave
    // pos ranges roughly from 0 to max_pos
    // speed adjusted by aspect to make movement look somewhat uniform in pixel space
    vec2 speed = vec2(0.1, 0.13); 
    
    vec2 pos;
    pos.x = abs(fract(time * speed.x) * 2.0 - 1.0) * max_pos.x;
    pos.y = abs(fract(time * speed.y) * 2.0 - 1.0) * max_pos.y;
    
    // Check if current pixel is inside the box
    vec2 uv = v_tex_coords;
    if (uv.x >= pos.x && uv.x <= pos.x + box_size.x &&
        uv.y >= pos.y && uv.y <= pos.y + box_size.y) {
        
        // Map current pixel relative to box to 0..1 range to sample the person
        vec2 sample_uv = (uv - pos) / box_size;
        
        // Sample with our mapped UVs
        f_color = texture(sampler2D(t_texture, s_sampler), sample_uv);
        f_mask_out = texture(sampler2D(t_mask, s_sampler), sample_uv).r;
    } else {
        // Background
        f_color = vec4(0.0, 0.0, 0.0, 1.0);
        f_mask_out = 0.0;
    }
}
