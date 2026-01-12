#version 450

// Rotating cube with camera texture on each face

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
layout(location=1) out float f_mask_out;

#define PI 3.14159265359

// Rotation matrices
mat3 rotateX(float a) {
    float c = cos(a), s = sin(a);
    return mat3(1.0, 0.0, 0.0,
                0.0, c, -s,
                0.0, s, c);
}

mat3 rotateY(float a) {
    float c = cos(a), s = sin(a);
    return mat3(c, 0.0, s,
                0.0, 1.0, 0.0,
                -s, 0.0, c);
}

mat3 rotateZ(float a) {
    float c = cos(a), s = sin(a);
    return mat3(c, -s, 0.0,
                s, c, 0.0,
                0.0, 0.0, 1.0);
}

// Box intersection - returns t for entry and exit, and the face normal
vec2 boxIntersect(vec3 ro, vec3 rd, vec3 boxSize, out vec3 outNormal) {
    vec3 m = 1.0 / rd;
    vec3 n = m * ro;
    vec3 k = abs(m) * boxSize;
    vec3 t1 = -n - k;
    vec3 t2 = -n + k;
    float tN = max(max(t1.x, t1.y), t1.z);
    float tF = min(min(t2.x, t2.y), t2.z);
    if (tN > tF || tF < 0.0) return vec2(-1.0);
    
    // Calculate which face was hit
    outNormal = -sign(rd) * step(t1.yzx, t1.xyz) * step(t1.zxy, t1.xyz);
    return vec2(tN, tF);
}

// Get UV coordinates for a face hit
vec2 getFaceUV(vec3 hitPos, vec3 normal, float boxSize) {
    vec2 uv;
    vec3 absNormal = abs(normal);
    
    if (absNormal.x > 0.5) {
        // Left or right face
        uv = hitPos.zy / boxSize;
        if (normal.x > 0.0) uv.x = -uv.x;
    } else if (absNormal.y > 0.5) {
        // Top or bottom face
        uv = hitPos.xz / boxSize;
        if (normal.y < 0.0) uv.y = -uv.y;
    } else {
        // Front or back face
        uv = hitPos.xy / boxSize;
        if (normal.z < 0.0) uv.x = -uv.x;
    }
    
    // Remap from [-1,1] to [0,1]
    uv = uv * 0.5 + 0.5;
    // Flip Y for correct texture orientation
    uv.y = 1.0 - uv.y;
    
    return uv;
}

void main() {
    vec2 uv = v_tex_coords;
    uv.y = 1.0 - uv.y;
    
    // Normalized device coordinates
    vec2 ndc = uv * 2.0 - 1.0;
    ndc.x *= width / height;
    
    // Camera setup
    float camDist = 4.0;
    vec3 ro = vec3(0.0, 0.0, camDist); // Ray origin (camera position)
    vec3 rd = normalize(vec3(ndc, -1.5)); // Ray direction
    
    // Rotation angles
    float rotX = time * 0.3;
    float rotY = time * 0.5;
    float rotZ = time * 0.2;
    
    // Create rotation matrix
    mat3 rot = rotateY(rotY) * rotateX(rotX) * rotateZ(rotZ);
    mat3 invRot = transpose(rot); // Inverse of rotation matrix
    
    // Transform ray into cube's local space
    vec3 localRo = invRot * ro;
    vec3 localRd = invRot * rd;
    
    // Box size
    float boxSize = 1.8;
    
    // Ray-box intersection
    vec3 normal;
    vec2 t = boxIntersect(localRo, localRd, vec3(boxSize), normal);
    
    vec3 color;
    
    if (t.x > 0.0) {
        // Hit the cube
        vec3 hitPos = localRo + localRd * t.x;
        
        // Get UV for this face
        vec2 faceUV = getFaceUV(hitPos, normal, boxSize);
        
        // Sample the camera texture and mask
        vec4 texColor = texture(sampler2D(t_texture, s_sampler), faceUV);
        float mask_val = texture(sampler2D(t_mask, s_sampler), faceUV).r;
        f_mask_out = mask_val;
        
        // Simple lighting
        vec3 worldNormal = rot * normal;
        vec3 lightDir = normalize(vec3(1.0, 1.0, 1.0));
        float diffuse = max(dot(worldNormal, lightDir), 0.0);
        float ambient = 0.3;
        float lighting = ambient + diffuse * 0.7;
        
        // Add specular highlight
        vec3 viewDir = normalize(-rd);
        vec3 reflectDir = reflect(-lightDir, worldNormal);
        float spec = pow(max(dot(viewDir, reflectDir), 0.0), 32.0);
        
        color = texColor.rgb * lighting + vec3(1.0) * spec * 0.3;
        
        // Add subtle edge darkening for depth
        float edge = 1.0 - pow(1.0 - abs(dot(worldNormal, viewDir)), 2.0);
        color *= 0.8 + 0.2 * edge;
    } else {
        // Background - gradient
        float gradient = length(ndc) * 0.3;
        color = mix(vec3(0.1, 0.1, 0.2), vec3(0.02, 0.02, 0.05), gradient);
        
        // Add subtle grid pattern
        vec2 grid = fract(ndc * 10.0);
        float gridLine = step(0.95, max(grid.x, grid.y));
        color += vec3(0.05) * gridLine * (1.0 - gradient);
        
        // No person in background
        f_mask_out = 0.0;
    }
    
    f_color = vec4(color, 1.0);
}
