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

float grid(vec2 uv, float scale) {
    vec2 grid_uv = fract(uv * scale);
    return step(0.95, grid_uv.x) + step(0.95, grid_uv.y);
}

void main() {
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);

    // Convert person to a digital monochromatic look
    vec3 cyanish = vec3(0.0, 0.8, 1.0);
    float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
    vec3 person_col = cyanish * gray * 1.5;
    
    // Add a hexagonal/grid overlay to the person
    // Warping the UVs slightly can look cool, but let's stick to flat grid for now
    float g = grid(v_tex_coords, 50.0);
    person_col += vec3(0.0, 0.5, 0.8) * g * 0.5;

    // Scanline effect
    // Moving horizontal bar
    float scan_pos = fract(time * 0.5);
    float dist = abs(v_tex_coords.y - scan_pos);
    float scanline = smoothstep(0.05, 0.0, dist);
    
    // Add scanline glow
    person_col += vec3(0.5, 1.0, 1.0) * scanline;
    
    // Edge detection on the person (Analysis mode)
    float d = 1.0 / width;
    float m_up = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(0.0, d)).r;
    float m_down = texture(sampler2D(t_mask, s_sampler), v_tex_coords - vec2(0.0, d)).r;
    float m_left = texture(sampler2D(t_mask, s_sampler), v_tex_coords - vec2(d, 0.0)).r;
    float m_right = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(d, 0.0)).r;
    float edge = abs(m_up - m_down) + abs(m_left - m_right);
    
    // Edges are bright neon orange/red for contrast
    person_col = mix(person_col, vec3(1.0, 0.2, 0.5), edge * 2.0);

    // Background: Darken and desaturate
    vec3 bg_col = color.rgb * 0.2;
    bg_col = mix(bg_col, vec3(0.0, 0.1, 0.2), 0.5); // Blueprint blue tint

    f_color = mix(vec4(bg_col, 1.0), vec4(person_col, 1.0), mask);
}
