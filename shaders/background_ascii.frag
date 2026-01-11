#version 450

// Background ASCII Shader
// Replaces the background with procedural ASCII art characters.
// The detected face remains as the original video feed.

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

// Luminance helper
float get_luminance(vec3 color) {
    return dot(color, vec3(0.299, 0.587, 0.114));
}

// 2D Signed Distance Functions
float sdCircle(vec2 p, float r) {
    return length(p) - r;
}

float sdBox(vec2 p, vec2 b) {
    vec2 d = abs(p) - b;
    return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}

float sdSegment(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a, ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

// Procedural Character Rendering
float get_char_mask(vec2 uv, float brightness) {
    vec2 p = uv - 0.5; // Center coordinates in grid cell [-0.5, 0.5]
    float d = 1.0;     // Distance field value
    
    // Character selection based on brightness levels
    // Gradient: . : - = + * X % # & @
    
    if (brightness < 0.08) {
        // Empty
        return 0.0;
    } else if (brightness < 0.16) {
        // Dot (.)
        d = sdCircle(p, 0.06);
    } else if (brightness < 0.24) {
        // Colon (:)
        float dot1 = sdCircle(p - vec2(0.0, 0.15), 0.06);
        float dot2 = sdCircle(p - vec2(0.0, -0.15), 0.06);
        d = min(dot1, dot2);
    } else if (brightness < 0.32) {
        // Dash (-)
        d = sdBox(p, vec2(0.2, 0.04));
    } else if (brightness < 0.4) {
        // Equals (=)
        float dash1 = sdBox(p - vec2(0.0, 0.1), vec2(0.2, 0.04));
        float dash2 = sdBox(p - vec2(0.0, -0.1), vec2(0.2, 0.04));
        d = min(dash1, dash2);
    } else if (brightness < 0.48) {
        // Plus (+)
        float v_bar = sdBox(p, vec2(0.04, 0.25));
        float h_bar = sdBox(p, vec2(0.25, 0.04));
        d = min(v_bar, h_bar); 
    } else if (brightness < 0.56) {
        // Asterisk (*)
        float v_bar = sdBox(p, vec2(0.04, 0.25));
        float h_bar = sdBox(p, vec2(0.25, 0.04));
        float plus = min(v_bar, h_bar);
        
        float d1 = sdSegment(p, vec2(-0.2, -0.2), vec2(0.2, 0.2));
        float d2 = sdSegment(p, vec2(-0.2, 0.2), vec2(0.2, -0.2));
        float x_shape = min(d1, d2) - 0.04;
        
        d = min(plus, x_shape);
    } else if (brightness < 0.64) {
        // Letter X / Cross
        float d1 = sdSegment(p, vec2(-0.25, -0.25), vec2(0.25, 0.25));
        float d2 = sdSegment(p, vec2(-0.25, 0.25), vec2(0.25, -0.25));
        d = min(d1, d2) - 0.06;
    } else if (brightness < 0.72) {
        // Percent-ish (%) - Slash with two circles
        float slash = sdSegment(p, vec2(-0.2, -0.2), vec2(0.2, 0.2)) - 0.04;
        float c1 = sdCircle(p - vec2(-0.2, 0.2), 0.06);
        float c2 = sdCircle(p - vec2(0.2, -0.2), 0.06);
        d = min(slash, min(c1, c2));
    } else if (brightness < 0.8) {
        // Hash (#)
        float v1 = sdBox(p - vec2(0.1, 0.0), vec2(0.04, 0.25));
        float v2 = sdBox(p - vec2(-0.1, 0.0), vec2(0.04, 0.25));
        float h1 = sdBox(p - vec2(0.0, 0.1), vec2(0.25, 0.04));
        float h2 = sdBox(p - vec2(0.0, -0.1), vec2(0.25, 0.04));
        d = min(min(v1, v2), min(h1, h2));
    } else if (brightness < 0.88) {
        // Ampersand-ish (&) - High density complex shape (Union of Box and X)
        float box = sdBox(p, vec2(0.25)) - 0.02; // Hollow rounded box frame
        float d1 = sdSegment(p, vec2(-0.2, -0.2), vec2(0.2, 0.2));
        float d2 = sdSegment(p, vec2(-0.2, 0.2), vec2(0.2, -0.2));
        float x_shape = min(d1, d2) - 0.04;
        d = min(abs(box) - 0.02, x_shape);
    } else {
        // Solid Block (@) - Filled rounded box
        d = sdBox(p, vec2(0.35));
    }
    
    // Anti-aliased rendering
    float fw = fwidth(d);
    return 1.0 - smoothstep(-fw, fw, d);
}

void main() {
    // 1. Get Segmentation Mask
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    
    // 2. Original Color for Background
    vec4 original_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    
    // 3. ASCII Grid Calculations
    // Number of characters across the screen width (e.g., width / 12px chars)
    float grid_size = 12.0; 
    float aspect = width / height;
    
    vec2 grid_uv = v_tex_coords;
    grid_uv.x *= aspect; // Correct aspect ratio for square grid cells
    
    // Quantize UVs to grid cells
    vec2 cell_count = vec2(width / grid_size, height / grid_size);
    vec2 grid_index = floor(grid_uv * cell_count);
    
    // UV within the single cell [0, 1]
    vec2 cell_uv = fract(grid_uv * cell_count);
    
    // Sample texture at the center of the grid cell
    // We need to map grid_index back to [0, 1] texture space
    vec2 sample_uv = (grid_index + 0.5) / cell_count;
    sample_uv.x /= aspect; // Undo aspect correction for sampling
    
    vec3 cell_color = texture(sampler2D(t_texture, s_sampler), sample_uv).rgb;
    float luma = get_luminance(cell_color);
    
    // 4. Render ASCII Character
    float char_shape = get_char_mask(cell_uv, luma);
    
    // Matrix Green text style
    vec3 ascii_color = vec3(0.0, 1.0, 0.2) * char_shape * (0.5 + 0.5 * luma);
    // Add a dark background for the ASCII part
    vec3 ascii_bg = vec3(0.0, 0.05, 0.0);
    vec3 final_ascii = mix(ascii_bg, ascii_color, char_shape);
    
    // 5. Mix with Mask
    // Smooth transition between face and background
    mask = smoothstep(0.4, 0.6, mask);
    
    // Inverted logic: Face is original, Background is ASCII
    f_color = mix(vec4(final_ascii, 1.0), original_color, mask);
}
