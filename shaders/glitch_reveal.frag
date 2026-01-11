#version 450

// Glitch Reveal Effect
// Randomly glitches to reveal t_image0 underneath, then snaps back to webcam.
// Creates a "digital interference" look - great for streaming!
// Usage: cargo run -- -s shaders/glitch_reveal.frag --image hidden_image.jpg

layout(set=0, binding=0) uniform texture2D t_texture;
layout(set=0, binding=1) uniform sampler s_sampler;
layout(set=0, binding=2) uniform Uniforms {
    float time;
    float width;
    float height;
    float seed;
};
layout(set=0, binding=3) uniform texture2D t_mask;
layout(set=0, binding=4) uniform texture2D t_image0;

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

// Pseudo-random function
float random(vec2 st) {
    return fract(sin(dot(st.xy, vec2(12.9898, 78.233))) * 43758.5453123);
}

// Hash function for glitch timing
float hash(float n) {
    return fract(sin(n) * 43758.5453);
}

// Helper to sample texture with "contain" aspect ratio (letterboxing)
vec4 sample_letterboxed(texture2D t, sampler s, vec2 uv) {
    ivec2 tex_size = textureSize(sampler2D(t, s), 0);
    float tex_aspect = 1.0;
    if (tex_size.x > 0 && tex_size.y > 0) {
        tex_aspect = float(tex_size.x) / float(tex_size.y);
    }
    float screen_aspect = width / height;
    
    if (screen_aspect > tex_aspect) {
        float scale = screen_aspect / tex_aspect;
        uv.x = (uv.x - 0.5) * scale + 0.5;
    } else {
        float scale = tex_aspect / screen_aspect;
        uv.y = (uv.y - 0.5) * scale + 0.5;
    }
    
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return vec4(0.0, 0.0, 0.0, 1.0);
    } else {
        return texture(sampler2D(t, s), uv);
    }
}

void main() {
    vec2 uv = v_tex_coords;
    
    // Time-based glitch trigger (random bursts)
    float glitchTime = floor(time * 3.0); // Changes every ~0.33 seconds
    float glitchRandom = hash(glitchTime + seed);
    
    // Only glitch occasionally (30% of the time)
    bool shouldGlitch = glitchRandom > 0.7;
    
    // Glitch intensity for smooth transitions
    float glitchIntensity = 0.0;
    if (shouldGlitch) {
        // Create pulsing glitch intensity within the glitch window
        float glitchPhase = fract(time * 3.0);
        glitchIntensity = sin(glitchPhase * 3.14159) * (0.5 + 0.5 * hash(glitchTime + 1.0));
    }
    
    // Horizontal line distortion
    float lineGlitch = 0.0;
    if (glitchIntensity > 0.1) {
        float lineY = floor(uv.y * 50.0);
        float lineRand = hash(lineY + glitchTime);
        if (lineRand > 0.7) {
            lineGlitch = (hash(lineY + glitchTime + 0.5) - 0.5) * 0.1 * glitchIntensity;
        }
    }
    
    // Apply horizontal shift
    vec2 glitchUV = uv;
    glitchUV.x += lineGlitch;
    
    // Color channel splitting (chromatic aberration during glitch)
    float chromaShift = glitchIntensity * 0.01;
    vec4 webcam;
    if (glitchIntensity > 0.2) {
        // RGB split
        webcam.r = texture(sampler2D(t_texture, s_sampler), glitchUV + vec2(chromaShift, 0.0)).r;
        webcam.g = texture(sampler2D(t_texture, s_sampler), glitchUV).g;
        webcam.b = texture(sampler2D(t_texture, s_sampler), glitchUV - vec2(chromaShift, 0.0)).b;
        webcam.a = 1.0;
    } else {
        webcam = texture(sampler2D(t_texture, s_sampler), glitchUV);
    }
    
    // Sample the hidden image (letterboxed)
    vec4 hiddenImage = sample_letterboxed(t_image0, s_sampler, glitchUV);
    
    // Block-based reveal (random rectangles show the hidden image)
    float blockSize = 0.05;
    vec2 blockPos = floor(uv / blockSize);
    float blockRand = hash(blockPos.x + blockPos.y * 100.0 + glitchTime);
    
    // Reveal hidden image in random blocks during glitch
    float revealAmount = 0.0;
    if (shouldGlitch && blockRand > (1.0 - glitchIntensity * 0.5)) {
        revealAmount = glitchIntensity;
    }
    
    // Static noise overlay during glitch
    float noise = 0.0;
    if (glitchIntensity > 0.3) {
        noise = random(uv + time) * 0.15 * glitchIntensity;
    }
    
    // Blend between webcam and hidden image
    vec4 result = mix(webcam, hiddenImage, revealAmount);
    
    // Add noise
    result.rgb += vec3(noise);
    
    // Scanline effect during glitch
    if (glitchIntensity > 0.2) {
        float scanline = sin(uv.y * height * 2.0) * 0.03 * glitchIntensity;
        result.rgb -= scanline;
    }
    
    f_color = result;
}
