#version 450

// Shader: hud_corner.frag
// Renders t_image1 as a HUD overlay in a configurable corner position
// on top of the background/person composite.
//
// ============================================
// CONFIGURATION - Change these values!
// ============================================
// HUD Position: 0=top-left, 1=top-right, 2=bottom-left, 3=bottom-right
#define HUD_POSITION 1

// HUD size as fraction of screen width (0.0 - 1.0)
#define HUD_SCALE 0.25

// Margin from screen edge (0.0 - 0.5)
#define HUD_MARGIN 0.02
// ============================================

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask;
layout(set=0, binding=4) uniform texture2D t_image0;  // Background
layout(set=0, binding=5) uniform texture2D t_image1;  // HUD overlay

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

void main() {
    // Sample mask and person
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    
    // === Background with aspect correction ===
    float video_aspect = 1.0;
    ivec2 vid_size = textureSize(sampler2D(t_image0, s_sampler), 0);
    if (vid_size.x > 0 && vid_size.y > 0) {
        video_aspect = float(vid_size.x) / float(vid_size.y);
    }
    
    float screen_aspect = width / height;
    vec2 bg_uv = v_tex_coords;
    
    if (screen_aspect > video_aspect) {
        float scale = screen_aspect / video_aspect;
        bg_uv.x = (bg_uv.x - 0.5) * scale + 0.5;
    } else {
        float scale = video_aspect / screen_aspect;
        bg_uv.y = (bg_uv.y - 0.5) * scale + 0.5;
    }
    
    vec4 background_color;
    if (bg_uv.x < 0.0 || bg_uv.x > 1.0 || bg_uv.y < 0.0 || bg_uv.y > 1.0) {
        background_color = vec4(0.0, 0.0, 0.0, 1.0);
    } else {
        background_color = texture(sampler2D(t_image0, s_sampler), bg_uv);
    }
    
    // Composite person onto background
    vec3 base_color = mix(background_color.rgb, person_color.rgb, mask);
    
    // === HUD Overlay ===
    // Get HUD texture dimensions to preserve aspect ratio
    ivec2 hud_size = textureSize(sampler2D(t_image1, s_sampler), 0);
    float hud_aspect = float(hud_size.x) / float(hud_size.y);
    
    // Calculate HUD dimensions preserving aspect ratio
    float hud_width = HUD_SCALE;
    float hud_height = HUD_SCALE * (width / height) / hud_aspect;
    
    // Calculate position based on HUD_POSITION
    float hud_left, hud_top;
    
    #if HUD_POSITION == 0
        // Top-left
        hud_left = HUD_MARGIN;
        hud_top = HUD_MARGIN;
    #elif HUD_POSITION == 1
        // Top-right
        hud_left = 1.0 - hud_width - HUD_MARGIN;
        hud_top = HUD_MARGIN;
    #elif HUD_POSITION == 2
        // Bottom-left
        hud_left = HUD_MARGIN;
        hud_top = 1.0 - hud_height - HUD_MARGIN;
    #else
        // Bottom-right (default)
        hud_left = 1.0 - hud_width - HUD_MARGIN;
        hud_top = 1.0 - hud_height - HUD_MARGIN;
    #endif
    
    float hud_right = hud_left + hud_width;
    float hud_bottom = hud_top + hud_height;
    
    // Check if current pixel is inside HUD region
    if (v_tex_coords.x >= hud_left && v_tex_coords.x <= hud_right &&
        v_tex_coords.y >= hud_top && v_tex_coords.y <= hud_bottom) {
        
        // Map screen coords to HUD texture coords (0-1)
        vec2 hud_uv;
        hud_uv.x = (v_tex_coords.x - hud_left) / hud_width;
        hud_uv.y = (v_tex_coords.y - hud_top) / hud_height;
        
        // Sample HUD texture
        vec4 hud_color = texture(sampler2D(t_image1, s_sampler), hud_uv);
        
        // Composite HUD on top using alpha
        base_color = mix(base_color, hud_color.rgb, hud_color.a);
    }
    
    f_color = vec4(base_color, 1.0);
}
