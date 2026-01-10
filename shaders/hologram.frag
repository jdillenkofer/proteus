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
    
    // Make mask generous
    mask = smoothstep(0.00, 0.2, mask);

    // Hologram Effect Calculation
    vec3 holo_tint = vec3(0.0, 1.0, 1.0); // Cyan
    
    // Moving scanlines
    float scanline = sin(v_tex_coords.y * 800.0 - time * 10.0);
    scanline = smoothstep(0.4, 0.5, scanline);
    
    // Glitch/Wave effect
    float distort = sin(v_tex_coords.y * 50.0 + time * 5.0) * 0.002;
    
    // Sample distorted color for hologram
    vec4 distorted_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords + vec2(distort, 0.0));
    
    // Combine
    vec3 holo_rgb = distorted_color.rgb;
    
    // 1. Grayscale + Tint
    float gray = dot(holo_rgb, vec3(0.299, 0.587, 0.114));
    holo_rgb = vec3(gray) * holo_tint * 1.5;
    
    // 2. Add Scanlines
    holo_rgb += holo_tint * scanline * 0.1;
    
    // 3. Fresnel/Rim light effect
    // We use the inverted mask edge for rim light
    float rim = pow(1.0 - mask, 4.0) * mask; 
    holo_rgb += holo_tint * rim * 2.0;

    // Mix Background (original color) with Hologram (person)
    f_color = vec4(mix(color.rgb, holo_rgb, mask), 1.0);
}
