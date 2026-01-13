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
layout(set=0, binding=4) uniform texture2D t_image0;

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

void main() {
    // Sample the mask directly (smoothness is now handled by CPU-side bilinear scaling)
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    // Calculate aspect-correct UVs for the video (slot 0) using textureSize
    // We want "contain" style (letterboxing): entire video visible, black bars if needed.
    
    // Default to 1.0 aspect if size query fails
    float video_aspect = 1.0;
    
    // Get texture size from the texture bound to t_image0 (binding set 4 in pipeline)
    // Note: t_image0 is a texture2D uniform
    ivec2 vid_size = textureSize(sampler2D(t_image0, s_sampler), 0);
    if (vid_size.x > 0 && vid_size.y > 0) {
        video_aspect = float(vid_size.x) / float(vid_size.y);
    }

    float screen_aspect = width / height;
    vec2 uv = v_tex_coords;
    
    // To "contain", we scale such that the video fits inside the screen.
    if (screen_aspect > video_aspect) {
        // Screen is wider. Bars on sides.
        float scale = screen_aspect / video_aspect;
        uv.x = (uv.x - 0.5) * scale + 0.5;
    } else {
        // Screen is taller. Bars on top/bottom.
        float scale = video_aspect / screen_aspect;
        uv.y = (uv.y - 0.5) * scale + 0.5;
    }
    // Black bars for out-of-bounds UVs (letterboxing)
    vec4 background_color;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        background_color = vec4(0.0, 0.0, 0.0, 1.0);
    } else {
        background_color = texture(sampler2D(t_image0, s_sampler), uv);
    }
    
    f_color = mix(background_color, person_color, mask);
}
