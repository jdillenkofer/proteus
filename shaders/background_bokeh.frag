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

// Depth of field with bokeh: the background is defocused through a disc-shaped
// aperture, and bright spots are weighted up so they bloom into bokeh discs
// like a fast lens would render them. The person stays in focus.

const int SAMPLES = 96;
const float BLUR_RADIUS = 0.028;     // Aperture radius as a fraction of frame height
const float HIGHLIGHT_BOOST = 40.0;  // How strongly bright spots dominate the disc
const float HIGHLIGHT_CURVE = 5.0;   // Higher = only the brightest spots bloom
const float GOLDEN_ANGLE = 2.39996323;

void main() {
    vec2 uv = v_tex_coords;
    vec4 original = texture(sampler2D(t_texture, s_sampler), uv);
    float mask = texture(sampler2D(t_mask, s_sampler), uv).r;

    // Optimization: If mask is 1.0 (person), skip blur entirely
    if (mask >= 0.99) {
        f_color = original;
        return;
    }

    // Radius in UV space, corrected so the disc is round on any aspect ratio
    vec2 radius = BLUR_RADIUS * vec2(height / width, 1.0);

    vec3 sum = vec3(0.0);
    float total_weight = 0.0;

    // Golden-angle spiral: covers the disc evenly without visible banding
    for (int i = 0; i < SAMPLES; i++) {
        float r = sqrt((float(i) + 0.5) / float(SAMPLES));
        float a = float(i) * GOLDEN_ANGLE;
        vec2 sample_uv = uv + vec2(cos(a), sin(a)) * r * radius;

        vec3 c = texture(sampler2D(t_texture, s_sampler), sample_uv).rgb;
        float sample_mask = texture(sampler2D(t_mask, s_sampler), sample_uv).r;

        float l = dot(c, vec3(0.299, 0.587, 0.114));
        float weight = 1.0 + HIGHLIGHT_BOOST * pow(l, HIGHLIGHT_CURVE);

        // Ignore samples that land on the person, otherwise their colors
        // smear outwards into a halo around the silhouette
        weight *= 1.0 - smoothstep(0.2, 0.8, sample_mask);

        sum += c * weight;
        total_weight += weight;
    }

    // Every sample landed on the person: nothing to blur with
    vec3 blurred = (total_weight > 0.001) ? sum / total_weight : original.rgb;

    // smoothstep mask for softer transition
    float alpha = smoothstep(0.2, 0.8, mask);

    f_color = vec4(mix(blurred, original.rgb, alpha), 1.0);
}
