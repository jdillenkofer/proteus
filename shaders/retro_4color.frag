#version 450

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask; // Unused for this global effect, but available

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

const vec3 C0 = vec3(15, 56, 15) / 255.0;   // Darkest
const vec3 C1 = vec3(48, 98, 48) / 255.0;   // Dark
const vec3 C2 = vec3(139, 172, 15) / 255.0; // Light
const vec3 C3 = vec3(155, 188, 15) / 255.0; // Lightest

void main() {
    // Pixelation Logic
    float pixel_scale = 4.0; // Scale factor to make pixels visible on HD
    vec2 res = vec2(width, height) / pixel_scale;
    
    vec2 uv = floor(v_tex_coords * res) / res;
    
    vec4 color = texture(sampler2D(t_texture, s_sampler), uv);
    
    // Convert to grayscale using luminance
    float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
    
    // Quantize to 4 levels
    // 0.0 - 0.25 -> C0
    // 0.25 - 0.50 -> C1
    // 0.50 - 0.75 -> C2
    // 0.75 - 1.0 -> C3
    
    vec3 out_col;
    
    if (gray < 0.25) {
        out_col = C0;
    } else if (gray < 0.5) {
        out_col = C1;
    } else if (gray < 0.75) {
        out_col = C2;
    } else {
        out_col = C3;
    }
    
    f_color = vec4(out_col, 1.0);
}
