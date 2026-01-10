#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;

// Heat color gradient: black -> blue -> purple -> red -> orange -> yellow -> white
vec3 heat_color(float t) {
    // 7-stop gradient for thermal camera look
    if (t < 0.15) {
        return mix(vec3(0.0, 0.0, 0.1), vec3(0.0, 0.0, 0.5), t / 0.15);  // black to blue
    } else if (t < 0.35) {
        return mix(vec3(0.0, 0.0, 0.5), vec3(0.5, 0.0, 0.5), (t - 0.15) / 0.2);  // blue to purple
    } else if (t < 0.55) {
        return mix(vec3(0.5, 0.0, 0.5), vec3(1.0, 0.0, 0.0), (t - 0.35) / 0.2);  // purple to red
    } else if (t < 0.75) {
        return mix(vec3(1.0, 0.0, 0.0), vec3(1.0, 0.5, 0.0), (t - 0.55) / 0.2);  // red to orange
    } else if (t < 0.9) {
        return mix(vec3(1.0, 0.5, 0.0), vec3(1.0, 1.0, 0.0), (t - 0.75) / 0.15);  // orange to yellow
    } else {
        return mix(vec3(1.0, 1.0, 0.0), vec3(1.0, 1.0, 1.0), (t - 0.9) / 0.1);  // yellow to white
    }
}

void main() {
    vec4 color = texture(sampler2D(t_texture, s_sampler), tex_coords);
    
    // Get luminance as "temperature"
    float heat = dot(color.rgb, vec3(0.299, 0.587, 0.114));
    
    frag_color = vec4(heat_color(heat), 1.0);
}
