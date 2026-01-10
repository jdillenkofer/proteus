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
    vec4 color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);

    // BACKGROUND: Stark, high contrast, desaturated
    float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
    
    // Blow out the highlights to make it look like a white room
    float contrast = 1.2;
    float brightness = 0.2;
    float bg_val = (gray - 0.5) * contrast + 0.5 + brightness;
    vec3 bg_color = vec3(clamp(bg_val, 0.0, 1.0));
    
    // Slight bluish tint for the sterile environment feel
    bg_color = mix(bg_color, vec3(0.9, 0.95, 1.0), 0.1);


    // FOREGROUND (PERSON): Crystalline Red
    // To make it look "poly" or "crystal", we can quantize the normals (if we had them)
    // or just quantize the color values to flat bands.
    
    // Colorize the person red/orange
    vec3 person_color = vec3(1.0, 0.2, 0.1);
    
    // Use the original brightness to shade the red
    // Quantize brightness to create "facets"
    float levels = 4.0;
    float shaded_gray = floor(gray * levels) / levels;
    
    vec3 fg_color = person_color * (shaded_gray * 0.8 + 0.4); // Boost base brightness
    
    // Add a rim light to the person to make them pop
    // Simple edge detection on mask
    float d = 2.0 / width; 
    float m_up = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(0.0, d)).r;
    float m_down = texture(sampler2D(t_mask, s_sampler), v_tex_coords - vec2(0.0, d)).r;
    float m_left = texture(sampler2D(t_mask, s_sampler), v_tex_coords - vec2(d, 0.0)).r;
    float m_right = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(d, 0.0)).r;
    
    float edge = abs(m_up - m_down) + abs(m_left - m_right);
    fg_color += vec3(1.0, 0.8, 0.0) * edge * 0.5; // Golden rim

    mask = smoothstep(0.4, 0.6, mask);
    f_color = mix(vec4(bg_color, 1.0), vec4(fg_color, 1.0), mask);
}
