#version 450

// Dual Background Transition
// Crossfades between t_image0 and t_image1 based on time.
// Usage: cargo run -- -s shaders/background_transition.frag --image bg1.jpg --image bg2.jpg

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask;
layout(set=0, binding=4) uniform texture2D t_image0;
layout(set=0, binding=5) uniform texture2D t_image1;

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

// Helper to sample texture with "contain" aspect ratio (letterboxing)
vec4 sample_letterboxed(texture2D t, sampler s, vec2 uv) {
    ivec2 tex_size = textureSize(sampler2D(t, s), 0);
    float tex_aspect = 1.0;
    if (tex_size.x > 0 && tex_size.y > 0) {
        tex_aspect = float(tex_size.x) / float(tex_size.y);
    }
    float screen_aspect = width / height;
    
    if (screen_aspect > tex_aspect) {
        float scale = screen_aspect / tex_aspect;
        uv.x = (uv.x - 0.5) * scale + 0.5;
    } else {
        float scale = tex_aspect / screen_aspect;
        uv.y = (uv.y - 0.5) * scale + 0.5;
    }
    
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return vec4(0.0, 0.0, 0.0, 1.0);
    } else {
        return texture(sampler2D(t, s), uv);
    }
}

void main() {
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    
    // Sample both background images (letterboxed)
    vec4 bg0 = sample_letterboxed(t_image0, s_sampler, v_tex_coords);
    vec4 bg1 = sample_letterboxed(t_image1, s_sampler, v_tex_coords);
    
    // Oscillate between 0 and 1 over ~5 seconds using sine wave
    float transition = (sin(time * 0.4) + 1.0) * 0.5;
    
    // Blend backgrounds
    vec4 background = mix(bg0, bg1, transition);
    
    // Smoothstep mask for cleaner person edges
    mask = smoothstep(0.4, 0.6, mask);
    
    f_color = mix(background, person_color, mask);
}
