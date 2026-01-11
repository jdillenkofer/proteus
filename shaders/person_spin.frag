#version 450

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;
layout(location=1) out float f_mask_out;

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask;

vec2 rotate(vec2 uv, vec2 center, float angle) {
    vec2 p = uv - center;
    float c = cos(angle);
    float s = sin(angle);
    // Inverse rotation to pull pixels from their new location to the current pixel
    return vec2(p.x * c + p.y * s, -p.x * s + p.y * c) + center;
}

void main() {
    float angle = time * 2.0; // Spin speed
    vec2 center = vec2(0.5, 0.5);

    // Calculate the coordinate to sample from if this pixel is part of the spun person
    // We rotate the UVs "backwards" to find which source pixel lands here
    vec2 rotated_uv = rotate(v_tex_coords, center, -angle);

    // Sample the mask at the rotated coordinate. 
    // If mask at rotated_uv is 1.0, it means the person is THERE, so they map to HERE.
    float person_mask = texture(sampler2D(t_mask, s_sampler), rotated_uv).r;

    // Output the shifted mask for the next shader
    f_mask_out = person_mask;

    // Get color
    if (person_mask > 0.1) {
        // Pixel belongs to the spun person
        f_color = texture(sampler2D(t_texture, s_sampler), rotated_uv);
    } else {
        // Pixel belongs to background
        f_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    }
}
