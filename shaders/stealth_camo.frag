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

// Simplex noise function
vec3 permute(vec3 x) { return mod(((x*34.0)+1.0)*x, 289.0); }

float snoise(vec2 v){
    const vec4 C = vec4(0.211324865405187, 0.366025403784439,
    -0.577350269189626, 0.024390243902439);
    vec2 i  = floor(v + dot(v, C.yy) );
    vec2 x0 = v -   i + dot(i, C.xx);
    vec2 i1;
    i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
    vec4 x12 = x0.xyxy + C.xxzz;
    x12.xy -= i1;
    i = mod(i, 289.0);
    vec3 p = permute( permute( i.y + vec3(0.0, i1.y, 1.0 ))
    + i.x + vec3(0.0, i1.x, 1.0 ));
    vec3 m = max(0.5 - vec3(dot(x0,x0), dot(x12.xy,x12.xy), dot(x12.zw,x12.zw)), 0.0);
    m = m*m ;
    m = m*m ;
    vec3 x = 2.0 * fract(p * C.www) - 1.0;
    vec3 h = abs(x) - 0.5;
    vec3 ox = floor(x + 0.5);
    vec3 a0 = x - ox;
    m *= 1.79284291400159 - 0.85373472095314 * ( a0*a0 + h*h );
    vec3 g;
    g.x  = a0.x  * x0.x  + h.x  * x0.y;
    g.yz = a0.yz * x12.xz + h.yz * x12.yw;
    return 130.0 * dot(m, g);
}

void main() {
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;

    // Background color (original)
    vec4 bg_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);

    // Distortion amount
    float distortion_strength = 0.05; // 5% UV shift
    
    // Generate noise for distortion
    // Animated by time
    float noise = snoise(v_tex_coords * 10.0 + time * 2.0);
    
    // Offset UVs based on noise
    vec2 distorted_uv = v_tex_coords + vec2(noise * distortion_strength);
    
    // Sample "background" (but really just distorted image)
    vec4 distorted_color = texture(sampler2D(t_texture, s_sampler), distorted_uv);
    
    // Add a subtle ripple/chromatic aberration to the distortion
    float aberration = 0.01;
    float r = texture(sampler2D(t_texture, s_sampler), distorted_uv + vec2(aberration, 0.0)).r;
    float b = texture(sampler2D(t_texture, s_sampler), distorted_uv - vec2(aberration, 0.0)).b;
    distorted_color.r = mix(distorted_color.r, r, 0.5);
    distorted_color.b = mix(distorted_color.b, b, 0.5);
    
    // Make the distortion color slightly brighter (electric feel)
    distorted_color += 0.1;

    // Final mix:
    // If mask is 1 (person), show distorted "invisibility" effect
    // If mask is 0 (background), show normal background
    
    // Rim light calculation for the "edge" of the invisibility
    // Use gradient of mask to find edges
    float d = 1.0 / width; // roughly 1 px
    float m_right = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(d, 0.0)).r;
    float m_up = texture(sampler2D(t_mask, s_sampler), v_tex_coords + vec2(0.0, d)).r;
    float edge = length(vec2(mask - m_right, mask - m_up));
    
    // Add rim light
    distorted_color += vec4(0.2, 0.5, 1.0, 0.0) * edge * 2.0;

    f_color = mix(bg_color, distorted_color, mask);
}
