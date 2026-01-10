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

// Safe tanh to avoid NaN/Inf and black artifacts
vec2 stanh(vec2 x) {
    return tanh(clamp(x, -15.0, 15.0));
}

void main() {
    float iTime = time;
    vec2 iResolution = vec2(width, height);
    vec2 fragCoord = vec2(v_tex_coords.x, 1.0 - v_tex_coords.y) * iResolution;

    vec2 v = iResolution.xy;
    vec2 u = 0.2 * (fragCoord + fragCoord - v) / v.y;
    
    vec4 z = vec4(1.0, 2.0, 3.0, 0.0);
    vec4 o = z;
    
    float a = 0.5;
    float t = iTime;
    
    for (float i = 0.0; ++i < 19.0; ) {
        // Loop body: update v first
        a += 0.03;
        v = cos(++t - 7.0 * u * pow(a, i)) - 5.0 * u;
        
        // Update u with matrix multiplication (u *= mat2(...) happens before dot(u,u))
        vec4 m_vec = cos(i + 0.02 * t - vec4(0.0, 11.0, 33.0, 0.0));
        mat2 m = mat2(m_vec.x, m_vec.y, m_vec.z, m_vec.w);
        u *= m;
        
        // Using stanh (safe tanh) to avoid black artifacts
        u += stanh(40.0 * dot(u, u) * cos(100.0 * u.yx + t)) / 200.0
           + 0.2 * a * u
           + cos(4.0 / exp(dot(o, o) / 100.0) + t) / 300.0;
        
        // Loop increment expression: accumulate o (runs after body)
        o += (1.0 + cos(z + t)) 
           / length((1.0 + i * dot(v, v)) 
                  * sin(1.5 * u / (0.5 - dot(u, u)) - 9.0 * u.yx + t));
    }
    
    o = 25.6 / (min(o, 13.0) + 164.0 / o) - dot(u, u) / 250.0;
    
    // Person composition
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    mask = smoothstep(0.4, 0.6, mask);

    f_color = vec4(mix(o.rgb, person_color.rgb, mask), 1.0);
}
