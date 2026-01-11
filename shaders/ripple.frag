#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 frag_color;
layout(location = 1) out float f_mask_out;

layout(set = 0, binding = 0) uniform texture2D t_texture;
layout(set = 0, binding = 1) uniform sampler s_sampler;
layout(set = 0, binding = 3) uniform texture2D t_mask;

layout(set = 0, binding = 2) uniform Uniforms {
    float time;
    float width;
    float height;
};

// Ripple/Wave effect shader
// Distorts UV coordinates to simulate water ripples

void main() {
    vec2 uv = tex_coords;
    
    // Parameters
    float amplitude = 0.02; // Strength of the wave
    float frequency = 10.0; // Number of waves
    float speed = 2.0;      // Speed of the waves
    
    // Radial ripple from center
    vec2 center = vec2(0.5, 0.5);
    vec2 to_center = uv - center;
    float dist = length(to_center);
    
    // Calculate wave offset
    // Wave moves outwards from center
    float wave = sin(dist * frequency * 2.0 - time * speed) * amplitude;
    
    // Attenuate wave at edges or center if desired, but constant is fine for "underwater" look
    // Let's diminish it slightly towards the edges to avoid edge clamping artifacts too much
    // wave *= (1.0 - dist); 

    // Distortion direction
    vec2 offset = normalize(to_center) * wave;
    
    // Also add a little vertical sine wave for "drunk" effect
    offset.y += sin(uv.x * 5.0 + time) * 0.005;
    offset.x += cos(uv.y * 5.0 + time) * 0.005;

    // Apply offset
    vec2 final_uv = uv + offset;
    
    // Clamp to screen to avoid trailing
    // Or wrap? Clamp is usually better for video
    // final_uv = clamp(final_uv, 0.0, 1.0); 
    // Actually, texture() handles out of bounds based on sampler settings.
    // Our sampler is ClampToEdge, so it's fine.

    vec4 tex_color = texture(sampler2D(t_texture, s_sampler), final_uv);
    
    // Add some "specular" highlights to the waves?
    // Maybe just brighten simple spots based on the wave height
    float highlight = max(0.0, wave / amplitude); // 0 to 1
    // tex_color.rgb += highlight * 0.1;

    frag_color = tex_color;
    f_mask_out = texture(sampler2D(t_mask, s_sampler), final_uv).r;
}
