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

// Gaussian Blur configuration
const int RADIUS = 15;
const float SIGMA = 7.0;

// simple 1D Gaussian weight
float gaussian(float x, float sigma) {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (2.50662827463 * sigma);
}

void main() {
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;

    // Optimization: If mask is 1.0 (person), skip blur entirely
    if (mask >= 0.99) {
        f_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
        return;
    }

    vec2 tex_size = vec2(width, height);
    vec2 inverse_size = 1.0 / tex_size;

    vec4 blurred = vec4(0.0);
    float total_weight = 0.0;

    // Two-pass approach is hard in single shader, so we do a simpler box/gaussian loop
    // For performance in a single pass, we might reduce radius or sample count
    // Or we use a separable blur if we had multiple passes. 
    // Here we do a single pass approximate blur (sparse sampling or small radius)
    // To keep it fast enough for real-time without multi-pass, we keep RADIUS small or step larger.
    
    // Let's do a simplified box blur spread out for effect, or a small gaussian.
    // Given we are in a fragment shader, let's do a basic loop.
    
    for (int x = -RADIUS; x <= RADIUS; x+=2) { // Step 2 for performance
        for (int y = -RADIUS; y <= RADIUS; y+=2) {
            vec2 offset = vec2(float(x), float(y)) * inverse_size * 2.5; // * Spread
            float weight = gaussian(length(vec2(float(x), float(y))), SIGMA);
            
            blurred += texture(sampler2D(t_texture, s_sampler), v_tex_coords + offset) * weight;
            total_weight += weight;
        }
    }
    blurred /= total_weight;

    vec4 original = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    
    // smoothstep mask for softer transition
    float alpha = smoothstep(0.2, 0.8, mask);

    f_color = mix(blurred, original, alpha);
}
