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

void main() {
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    mask = smoothstep(0.00, 0.2, mask);

    // Synthwave Grid
    // We basically want a 3D plane projection.
    // Horizon is at y = 0.4 roughly.
    
    vec2 uv = v_tex_coords;
    float horizon = 0.4;
    
    vec3 bg_color = vec3(0.0);
    
    if (uv.y < horizon) {
        // Sky (Gradient + Sun)
        // Sunset Gradient
        vec3 top_sky = vec3(0.1, 0.0, 0.2); // Deep purple
        vec3 bottom_sky = vec3(1.0, 0.2, 0.5); // Pink/Orange
        bg_color = mix(top_sky, bottom_sky, pow(uv.y / horizon, 2.0));
        
        // Sun
        vec2 sun_pos = vec2(0.5, horizon);
        float dist = distance(uv, sun_pos);
        if (dist < 0.2) {
             // Sun stripes
             float stripe =  sin(uv.y * 100.0);
             if (uv.y < horizon - 0.05 && stripe > 0.5) {
                // Cutout
             } else {
                 // Gradient Sun
                 bg_color = mix(vec3(1.0, 1.0, 0.0), vec3(1.0, 0.0, 0.5), (sun_pos.y - uv.y) * 5.0);
             }
        }

    } else {
        // Ground (Grid)
        // Perspective projection: z depends on y
        // Map uv.y from [horizon, 1.0] to Z depth
        float z = 1.0 / (uv.y - horizon + 0.1); // Avoid div by zero
        float x = (uv.x - 0.5) * z;
        
        // Moving grid
        float speed = 2.0;
        float grid_z = fract(z + time * speed); // Moving forward
        float grid_x = fract(x * 2.0);
        
        // Grid lines
        float line_width = 0.05 * z; // Thicker closer to camera
        float line = step(1.0 - line_width, grid_z) + step(1.0 - line_width, grid_x);
        
        vec3 grid_color = vec3(1.0, 0.0, 1.0); // Magenta neon
        vec3 floor_color = vec3(0.1, 0.0, 0.2); // Dark purple floor
        
        bg_color = mix(floor_color, grid_color, clamp(line, 0.0, 1.0));
        
        // Fade to horizon
        float mist = smoothstep(0.0, 1.0, (uv.y - horizon) * 2.0);
        bg_color *= mist;
    }

    f_color = vec4(mix(bg_color, person_color.rgb, mask), 1.0);
}
