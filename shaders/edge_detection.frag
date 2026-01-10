#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;

void main() {
    vec2 tex_size = vec2(textureSize(sampler2D(t_texture, s_sampler), 0));
    vec2 pixel = 1.0 / tex_size;
    
    // Sobel kernels
    // Gx kernel: [-1 0 1; -2 0 2; -1 0 1]
    // Gy kernel: [-1 -2 -1; 0 0 0; 1 2 1]
    
    // Sample 3x3 neighborhood
    float tl = dot(texture(sampler2D(t_texture, s_sampler), tex_coords + vec2(-pixel.x, -pixel.y)).rgb, vec3(0.299, 0.587, 0.114));
    float tc = dot(texture(sampler2D(t_texture, s_sampler), tex_coords + vec2(0.0, -pixel.y)).rgb, vec3(0.299, 0.587, 0.114));
    float tr = dot(texture(sampler2D(t_texture, s_sampler), tex_coords + vec2(pixel.x, -pixel.y)).rgb, vec3(0.299, 0.587, 0.114));
    
    float ml = dot(texture(sampler2D(t_texture, s_sampler), tex_coords + vec2(-pixel.x, 0.0)).rgb, vec3(0.299, 0.587, 0.114));
    float mr = dot(texture(sampler2D(t_texture, s_sampler), tex_coords + vec2(pixel.x, 0.0)).rgb, vec3(0.299, 0.587, 0.114));
    
    float bl = dot(texture(sampler2D(t_texture, s_sampler), tex_coords + vec2(-pixel.x, pixel.y)).rgb, vec3(0.299, 0.587, 0.114));
    float bc = dot(texture(sampler2D(t_texture, s_sampler), tex_coords + vec2(0.0, pixel.y)).rgb, vec3(0.299, 0.587, 0.114));
    float br = dot(texture(sampler2D(t_texture, s_sampler), tex_coords + vec2(pixel.x, pixel.y)).rgb, vec3(0.299, 0.587, 0.114));
    
    // Compute gradients
    float gx = -tl + tr - 2.0 * ml + 2.0 * mr - bl + br;
    float gy = -tl - 2.0 * tc - tr + bl + 2.0 * bc + br;
    
    // Gradient magnitude
    float edge = sqrt(gx * gx + gy * gy);
    
    frag_color = vec4(vec3(edge), 1.0);
}
