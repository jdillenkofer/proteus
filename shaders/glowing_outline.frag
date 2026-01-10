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
    vec4 color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    
    // Edge detection on the mask to find the outline
    // We sample neighbors to see if we are on the boundary of the person
    float offset_x = 4.0 / width;
    float offset_y = 4.0 / height;
    
    float mask_up = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(0.0, -offset_y)).r;
    float mask_down = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(0.0, offset_y)).r;
    float mask_left = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(-offset_x, 0.0)).r;
    float mask_right = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(offset_x, 0.0)).r;
    
    // A pixel is on the edge if it's not fully person but neighbors are different, or vice versa.
    // Simple edge: gradients magnitude
    float edge = abs(mask - mask_up) + abs(mask - mask_down) + abs(mask - mask_left) + abs(mask - mask_right);
    edge = smoothstep(0.1, 0.5, edge); // Sharpen the edge
    
    // Dynamic Glow Color (Pulse)
    vec3 glow_color = vec3(1.0, 0.5, 0.0); // Orange default
    glow_color.r = 0.5 + 0.5 * sin(time * 2.0);
    glow_color.g = 0.5 + 0.5 * cos(time * 3.0);
    glow_color.b = 0.5 + 0.5 * sin(time * 1.5);
    
    // Composition
    // Background: Darkened
    vec3 final_color = color.rgb * 0.3; 
    
    // Person: Normal
    final_color = mix(final_color, color.rgb, mask);
    
    // Add Glow (Additive blending)
    final_color += glow_color * edge * 2.0;

    f_color = vec4(final_color, 1.0);
}
