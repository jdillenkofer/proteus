#version 450

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

// Frosted fluted glass: the image is seen through a pane of vertical ribs.
// Every rib is a small cylindrical lens that bends the view sideways, and the
// frosted surface scatters what is left into a soft, grainy blur.

const float RIB_WIDTH = 28.0;      // Rib pitch in pixels (at 720p)
const float REFRACTION = 2.5;      // How many rib widths each rib bends the view
const int FROST_SAMPLES = 16;
const float FROST_RADIUS = 7.0;    // Scatter radius in pixels (at 720p)
const float MILKINESS = 0.12;      // How much the glass washes the image out
const vec3 GLASS_TINT = vec3(0.93, 0.97, 1.0);
const float GOLDEN_ANGLE = 2.39996323;

float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

void main() {
    float scale = max(height / 720.0, 1.0);
    vec2 px = scale / vec2(width, height);
    vec2 uv = v_tex_coords;

    // Position across the current rib: -0.5 at its left edge, 0.5 at its right
    vec2 p = uv * vec2(width, height) / scale;
    float across = fract(p.x / RIB_WIDTH) - 0.5;

    // Lens: bend the lookup further sideways the closer we get to the rib's edge
    vec2 refracted = uv + vec2(across * REFRACTION * RIB_WIDTH * px.x, 0.0);

    // Frost: scatter samples over a disc, rotated per pixel so it reads as grain
    float rotation = hash(p + seed) * 6.28318530718;
    vec3 color = vec3(0.0);
    for (int i = 0; i < FROST_SAMPLES; i++) {
        float r = sqrt((float(i) + 0.5) / float(FROST_SAMPLES)) * FROST_RADIUS;
        float a = float(i) * GOLDEN_ANGLE + rotation;
        vec2 sample_uv = refracted + vec2(cos(a), sin(a)) * r * px;
        color += texture(sampler2D(t_texture, s_sampler), clamp(sample_uv, 0.0, 1.0)).rgb;
    }
    color /= float(FROST_SAMPLES);

    // Milky glass body
    color = mix(color, vec3(1.0), MILKINESS) * GLASS_TINT;

    // Light catches one flank of each rib and the other falls into shade,
    // with a dark seam where two ribs meet
    color += 0.10 * smoothstep(0.15, 0.45, across);
    color *= 1.0 - 0.18 * smoothstep(0.15, 0.5, -across);
    color *= 1.0 - 0.25 * smoothstep(0.44, 0.5, abs(across));

    f_color = vec4(clamp(color, 0.0, 1.0), 1.0);
}
