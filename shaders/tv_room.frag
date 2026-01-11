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

// Raymarching constants - optimized for performance
const int MAX_STEPS = 64;
const float MAX_DIST = 50.0;
const float SURF_DIST = 0.02;

// IDs for object identification
const int ID_NONE = 0;
const int ID_FLOOR = 1;
const int ID_WALL = 2;
const int ID_TV_CASE = 3;
const int ID_TV_SCREEN = 4;
const int ID_STAND = 5;

const int ID_SPEAKER = 6;
const int ID_SHELF = 7;
const int ID_RUG = 8;
const int ID_TABLE = 9;
const int ID_PLANT = 10;
const int ID_ART = 11;
const int ID_WALL_LIGHT = 12;
const int ID_COUCH = 13;
const int ID_DECOR = 14;
const int ID_MIRROR = 15;
const int ID_WINDOW = 16;

// SDF Primitives
float sdBox(vec3 p, vec3 b) {
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

float sdCylinder(vec3 p, float h, float r) {
  vec2 d = abs(vec2(length(p.xz),p.y)) - vec2(r,h);
  return min(max(d.x,d.y),0.0) + length(max(d,0.0));
}

// Scene Description
vec2 GetDist(vec3 p) {
    float d = MAX_DIST;
    int id = ID_NONE;

    // Floor (Plane at y=0)
    float dFloor = p.y;
    if(dFloor < d) { d = dFloor; id = ID_FLOOR; }

    // Back Wall (at z = 4.5)
    float dWall = -(p.z - 4.5);
    if(dWall < d) { d = dWall; id = ID_WALL; }

    // Left Wall (at x = -4.0)
    float dLeftWall = p.x + 4.0;
    if(dLeftWall < d) { d = dLeftWall; id = ID_WALL; }

    // Right Wall (at x = 4.0)
    float dRightWall = -(p.x - 4.0);
    if(dRightWall < d) { d = dRightWall; id = ID_WALL; }

    // TV Stand (Cabinet)
    vec3 standPos = p - vec3(0.0, 0.4, 3.5);
    float dStand = sdBox(standPos, vec3(1.2, 0.4, 0.4));
    if(dStand < d) { d = dStand; id = ID_STAND; }

    // TV Foot/Pedestal - Modern curved design
    // Base plate
    vec3 footPos = p - vec3(0.0, 0.82, 3.5);
    float dFootBase = sdBox(footPos, vec3(0.6, 0.02, 0.3));
    if(dFootBase < d) { d = dFootBase; id = ID_STAND; }
    
    // Curved neck (tilted back slightly for style)
    vec3 neckPos = p - vec3(0.0, 1.0, 3.52);
    float dNeck = sdBox(neckPos, vec3(0.04, 0.18, 0.04));
    if(dNeck < d) { d = dNeck; id = ID_STAND; }

    // TV Case (mounted on foot)
    vec3 tvPos = p - vec3(0.0, 1.5, 3.5);
    float dTvCase = sdBox(tvPos, vec3(1.4, 0.7, 0.08));
    if(dTvCase < d) { d = dTvCase; id = ID_TV_CASE; }

    // TV Screen
    vec3 screenPos = p - vec3(0.0, 1.5, 3.42);
    float dScreen = sdBox(screenPos, vec3(1.25, 0.6, 0.005));
    if(dScreen < d) { d = dScreen; id = ID_TV_SCREEN; }
    
    // Speakers (Left and Right)
    vec3 pSpeaker = p;
    pSpeaker.x = abs(pSpeaker.x);
    pSpeaker = pSpeaker - vec3(2.0, 0.8, 3.5);
    float dSpeaker = sdBox(pSpeaker, vec3(0.3, 0.8, 0.3));
    if(dSpeaker < d) { d = dSpeaker; id = ID_SPEAKER; }
    
    // Shelf (On wall, above TV)
    vec3 pShelf = p - vec3(0.0, 3.2, 4.3);
    float dShelf = sdBox(pShelf, vec3(1.5, 0.04, 0.15));
    if(dShelf < d) { d = dShelf; id = ID_SHELF; }

    // Rug (on floor, center of room)
    vec3 pRug = p - vec3(0.0, 0.01, 1.5);
    float dRug = sdCylinder(pRug, 0.02, 1.8);
    if(dRug < d) { d = dRug; id = ID_RUG; }

    // Coffee Table (Rectangular, more visible)
    vec3 pTable = p - vec3(0.0, 0.35, 1.2);
    // Table top
    float dTableTop = sdBox(pTable, vec3(0.7, 0.03, 0.4));
    // Table legs (4 legs reaching to floor)
    vec3 pLeg = p;
    pLeg.x = abs(pLeg.x);
    pLeg.z = abs(pLeg.z - 1.2);
    float dLeg = sdBox(pLeg - vec3(0.6, 0.16, 0.3), vec3(0.04, 0.16, 0.04));
    float dTable = min(dTableTop, dLeg);
    if(dTable < d) { d = dTable; id = ID_TABLE; }
    
    // Vase on coffee table (table top surface at Y=0.38)
    // Vase bottom at Y=0.38, extends upward
    vec3 pVase = p - vec3(0.0, 0.38, 1.2);
    float vaseH = 0.12; // Height
    float vaseR = 0.04 + 0.01 * sin((pVase.y / vaseH) * 6.28);
    float dVase = length(pVase.xz) - vaseR;
    dVase = max(dVase, -pVase.y); // Bottom at Y=0.38
    dVase = max(dVase, pVase.y - vaseH); // Top
    if(dVase < d) { d = dVase; id = ID_DECOR; }
    
    // Books on shelf (standing upright)
    // Book dimensions: width (x), height (y), depth (z)
    vec3 pBooks = p - vec3(-0.8, 3.28, 4.18);
    // Book 1 - tall
    float dBook1 = sdBox(pBooks - vec3(0.0, 0.1, 0.0), vec3(0.04, 0.14, 0.08));
    // Book 2 - medium
    float dBook2 = sdBox(pBooks - vec3(0.1, 0.08, 0.0), vec3(0.03, 0.12, 0.07));
    // Book 3 - short
    float dBook3 = sdBox(pBooks - vec3(0.18, 0.06, 0.0), vec3(0.035, 0.1, 0.06));
    // Book 4
    float dBook4 = sdBox(pBooks - vec3(0.27, 0.09, 0.0), vec3(0.04, 0.13, 0.08));
    float dBooks = min(min(dBook1, dBook2), min(dBook3, dBook4));
    if(dBooks < d) { d = dBooks; id = ID_DECOR; }
    
    // Cushions on couch
    vec3 pCushion = p - vec3(0.0, 0.55, -1.2);
    pCushion.x = abs(pCushion.x);
    vec3 pC1 = pCushion - vec3(0.8, 0.0, 0.0);
    float dCushion = sdBox(pC1, vec3(0.2, 0.15, 0.12));
    if(dCushion < d) { d = dCushion; id = ID_DECOR; }
    
    // Couch (Behind coffee table, facing TV) - Moved back
    vec3 pCouch = p - vec3(0.0, 0.3, -1.2);
    // Seat
    float dSeat = sdBox(pCouch, vec3(1.4, 0.2, 0.4));
    // Backrest
    float dBack = sdBox(pCouch - vec3(0.0, 0.4, -0.35), vec3(1.4, 0.4, 0.08));
    // Armrests
    vec3 pArm = pCouch;
    pArm.x = abs(pArm.x);
    float dArm = sdBox(pArm - vec3(1.3, 0.2, 0.0), vec3(0.12, 0.35, 0.4));
    float dCouch = min(min(dSeat, dBack), dArm);
    if(dCouch < d) { d = dCouch; id = ID_COUCH; }
    
    // Potted Plant (Left corner) - More detailed
    vec3 pPlant = p - vec3(-3.2, 0.0, 3.5);
    
    // Pot (tapered cylinder shape)
    float potR = 0.3 - pPlant.y * 0.05; // Tapers toward top
    float dPot = length(pPlant.xz) - potR;
    dPot = max(dPot, -pPlant.y);           // Bottom cap
    dPot = max(dPot, pPlant.y - 0.5);      // Top opening
    
    // Soil inside pot
    float dSoil = length(pPlant.xz) - 0.25;
    dSoil = max(dSoil, abs(pPlant.y - 0.48) - 0.03);
    
    // Multiple leaf clusters for fullness
    float dLeaves = 1000.0;
    // Center cluster
    dLeaves = min(dLeaves, length(pPlant - vec3(0.0, 0.9, 0.0)) - 0.35);
    // Side clusters
    dLeaves = min(dLeaves, length(pPlant - vec3(0.2, 0.75, 0.1)) - 0.25);
    dLeaves = min(dLeaves, length(pPlant - vec3(-0.15, 0.8, 0.15)) - 0.28);
    dLeaves = min(dLeaves, length(pPlant - vec3(0.1, 0.7, -0.2)) - 0.22);
    dLeaves = min(dLeaves, length(pPlant - vec3(-0.1, 1.0, 0.05)) - 0.2);
    
    float dPlant = min(min(dPot, dSoil), dLeaves);
    if(dPlant < d) { d = dPlant; id = ID_PLANT; }
    
    // Wall Art (Canvas on back wall)
    vec3 pArt = p - vec3(0.0, 3.8, 4.4);
    float dArt = sdBox(pArt, vec3(0.8, 0.5, 0.04));
    if(dArt < d) { d = dArt; id = ID_ART; }

    // Wall Lights (Left and Right of TV)
    vec3 pLight = p;
    pLight.x = abs(pLight.x);
    pLight = pLight - vec3(2.8, 2.2, 4.4);
    float dLightBox = sdBox(pLight, vec3(0.15, 0.3, 0.08));
    if(dLightBox < d) { d = dLightBox; id = ID_WALL_LIGHT; }

    // Abstract Picture on left wall
    vec3 pMirror = p - vec3(-3.95, 1.8, 2.0);
    float dMirror = sdBox(pMirror, vec3(0.02, 1.2, 1.0));
    if(dMirror < d) { d = dMirror; id = ID_MIRROR; }
    
    // Window on right wall
    vec3 pWindow = p - vec3(3.95, 2.0, 2.0);
    float dWindow = sdBox(pWindow, vec3(0.02, 1.0, 0.8));
    if(dWindow < d) { d = dWindow; id = ID_WINDOW; }

    return vec2(d, float(id));
}

// Raymarch function
vec2 RayMarch(vec3 ro, vec3 rd) {
    float dO = 0.0;
    int id = ID_NONE;
    
    for(int i=0; i<MAX_STEPS; i++) {
        vec3 p = ro + rd * dO;
        vec2 dS = GetDist(p);
        dO += dS.x;
        id = int(dS.y);
        if(dO > MAX_DIST || abs(dS.x) < SURF_DIST) break;
    }
    
    return vec2(dO, float(id));
}

vec3 GetNormal(vec3 p) {
    float d = GetDist(p).x;
    vec2 e = vec2(0.01, 0.0);
    
    vec3 n = d - vec3(
        GetDist(p-e.xyy).x,
        GetDist(p-e.yxy).x,
        GetDist(p-e.yyx).x
    );
    
    return normalize(n);
}

// Ambient Occlusion - optimized (3 samples)
float CalcAO(vec3 p, vec3 n) {
    float occ = 0.0;
    float sca = 1.0;
    for(int i=0; i<3; i++) {
        float h = 0.02 + 0.15 * float(i)/2.0;
        float d = GetDist(p + h*n).x;
        occ += (h-d)*sca;
        sca *= 0.9;
    }
    return clamp(1.0 - 2.5*occ, 0.0, 1.0);
}

// Lighting and shading
vec3 GetMaterial(vec3 p, int id, vec3 n, vec3 rd, vec3 lightPos) {
    vec3 col = vec3(0.0);
    vec3 V = -rd; // View direction
    
    // Ambient Occlusion
    float ao = CalcAO(p, n);
    
    // === PHOTOREALISTIC LIGHTING ===
    
    // Wall Lights - Primary warm light sources
    vec3 l1Pos = vec3(2.8, 2.2, 4.0);
    vec3 l2Pos = vec3(-2.8, 2.2, 4.0);
    vec3 warmLight = vec3(1.0, 0.7, 0.4); // Warm tungsten
    
    // Light 1
    vec3 L1 = normalize(l1Pos - p);
    float dist1 = length(l1Pos - p);
    float atten1 = 1.0 / (1.0 + 0.1 * dist1 + 0.02 * dist1 * dist1);
    float NdL1 = max(dot(n, L1), 0.0);
    
    // Light 2
    vec3 L2 = normalize(l2Pos - p);
    float dist2 = length(l2Pos - p);
    float atten2 = 1.0 / (1.0 + 0.1 * dist2 + 0.02 * dist2 * dist2);
    float NdL2 = max(dot(n, L2), 0.0);
    
    // Combined warm lighting (no shadows for performance)
    vec3 warmLighting = warmLight * (NdL1 * atten1 + NdL2 * atten2) * 2.0;
    
    // TV Screen - Cool fill light
    vec3 tvPos = vec3(0.0, 1.5, 3.5);
    vec3 Ltv = normalize(tvPos - p);
    float tvDist = length(tvPos - p);
    float tvAtten = 1.0 / (1.0 + 0.2 * tvDist * tvDist);
    float NdLtv = max(dot(n, -Ltv), 0.0); // Facing away from TV
    vec3 tvLight = vec3(0.4, 0.5, 0.7) * NdLtv * tvAtten * 1.5;
    
    // Ambient - Very dark blue
    vec3 ambient = vec3(0.02, 0.025, 0.04) * (0.5 + 0.5 * n.y);
    
    // Hemisphere ambient (ground bounce)
    vec3 groundBounce = vec3(0.01, 0.008, 0.005) * max(-n.y, 0.0);
    
    // Final lighting
    vec3 lighting = (warmLighting + tvLight + ambient + groundBounce) * ao;
    
    // Fresnel for all materials
    float NdV = max(dot(n, V), 0.0);
    float fresnel = pow(1.0 - NdV, 4.0);
    vec3 fresnelTint = vec3(0.08, 0.06, 0.04); // Warm edge

    if (id == ID_FLOOR) {
        // Solid Dark Polished Floor
        vec3 floorColor = vec3(0.012, 0.012, 0.015);
        col = floorColor * lighting;
        
        // Simple Fresnel highlight (no expensive raymarch)
        float NdotV = clamp(dot(n, -rd), 0.0, 1.0);
        float floorFresnel = 0.02 + 0.08 * pow(1.0 - NdotV, 3.0);
        col += vec3(0.15, 0.12, 0.1) * floorFresnel;
    } else if (id == ID_WALL) {
        // Paint texture
        float noise = fract(sin(dot(p.xy, vec2(12.9898, 78.233))) * 43758.5453);
        col = vec3(0.4, 0.42, 0.4) * lighting * (0.95 + 0.05 * noise); 
    } else if (id == ID_STAND) {
        // Metallic
        col = vec3(0.1) * lighting;
        vec3 ref = reflect(-L1, n);
        float spec = pow(max(dot(ref, V), 0.0), 64.0);
        col += spec * atten1 * warmLight * 0.5; // Specular from wall lights
    } else if (id == ID_SPEAKER) {
        col = vec3(0.05) * lighting;
        float grill = step(0.9, fract(p.y * 20.0));
        col *= mix(0.8, 1.0, grill);
    } else if (id == ID_SHELF) {
        col = vec3(0.2, 0.1, 0.05) * lighting;
    } else if (id == ID_RUG) {
        // Fluffy Rug - Dark Red
        float noise = fract(sin(dot(p.xz, vec2(12.9898, 78.233))) * 43758.5453);
        col = vec3(0.4, 0.05, 0.05) * (0.7 + 0.3 * noise) * lighting; 
    } else if (id == ID_TABLE) {
        // White modern table - very smooth
        col = vec3(0.8) * lighting;
        float spec = pow(max(dot(reflect(-L1, n), V), 0.0), 32.0);
        col += spec * 0.5 * atten1;
    } else if (id == ID_PLANT) {
        vec3 pPlant = p - vec3(-3.2, 0.0, 3.5);
        if (p.y < 0.5) {
            // Pot (terracotta)
            col = vec3(0.5, 0.25, 0.12) * lighting;
        } else if (p.y < 0.52) {
            // Soil
            col = vec3(0.1, 0.06, 0.04) * lighting;
        } else {
            // Leaves (rich green with variation)
            float leafVar = fract(sin(dot(pPlant.xz * 10.0, vec2(12.9898, 78.233))) * 43758.5453);
            col = mix(vec3(0.05, 0.25, 0.05), vec3(0.08, 0.35, 0.08), leafVar) * lighting;
        }
    } else if (id == ID_ART) {
        // Geometric Abstract Art above shelf
        vec3 pArt = p - vec3(0.0, 3.8, 4.4);
        vec2 uv = pArt.xy * vec2(1.25, 2.0) * 0.5 + 0.5;
        
        // Geometric pattern - circles and lines
        float circles = 0.0;
        circles += smoothstep(0.22, 0.2, length(uv - vec2(0.3, 0.4)));
        circles += smoothstep(0.15, 0.13, length(uv - vec2(0.7, 0.6)));
        circles += smoothstep(0.1, 0.08, length(uv - vec2(0.5, 0.3)));
        
        // Lines
        float lines = step(0.98, fract(uv.x * 8.0)) + step(0.98, fract(uv.y * 6.0));
        
        // Color scheme - cool blues and teals
        vec3 bgCol = vec3(0.05, 0.1, 0.15);
        vec3 circleCol = vec3(0.1, 0.4, 0.5);
        vec3 lineCol = vec3(0.6, 0.7, 0.8);
        
        col = bgCol;
        col = mix(col, circleCol, circles);
        col = mix(col, lineCol, lines * 0.5);
        col *= 0.7;
        
        // Frame
        if (max(abs(pArt.x) - 0.75, abs(pArt.y) - 0.45) > 0.0) {
            col = vec3(0.04, 0.04, 0.05) * lighting;
        }
    } else if (id == ID_WALL_LIGHT) {
        // Bright Emissive with glow
        col = vec3(2.0, 1.5, 0.8); // Brighter emissive
        // Add bloom effect by increasing brightness at center
        float glow = 1.0 - length(p.xy - vec2(abs(p.x) > 0.0 ? sign(p.x) * 2.8 : 0.0, 2.2)) * 0.5;
        col *= (1.0 + max(glow, 0.0) * 0.5); 
    } else if (id == ID_COUCH) {
        // Dark leather/fabric couch
        col = vec3(0.15, 0.12, 0.1) * lighting;
        // Subtle fabric texture
        float tex = fract(sin(dot(p.xz * 5.0, vec2(12.9898, 78.233))) * 43758.5453);
        col *= (0.9 + 0.1 * tex);
    } else if (id == ID_DECOR) {
        // Various decoration colors based on position
        if (p.y > 3.0) {
            // Books - varied colors
            float bookId = floor(p.x * 3.0);
            if (bookId < -1.0) col = vec3(0.6, 0.15, 0.1) * lighting; // Red book
            else if (bookId < 0.0) col = vec3(0.1, 0.2, 0.5) * lighting; // Blue book
            else col = vec3(0.4, 0.35, 0.1) * lighting; // Brown book
        } else if (p.y > 0.6) {
            // Cushions - warm fabric
            col = vec3(0.4, 0.15, 0.1) * lighting;
        } else {
            // Vase - ceramic white
            col = vec3(0.7, 0.7, 0.65) * lighting;
            float spec = pow(max(dot(reflect(-L1, n), V), 0.0), 16.0);
            col += spec * 0.3 * atten1;
        }
    } else if (id == ID_TV_CASE) {
        col = vec3(0.02) * lighting;
        // Plastic specular
        float spec = pow(max(dot(reflect(-L1, n), V), 0.0), 16.0);
        col += spec * 0.2 * atten1;
    } else if (id == ID_MIRROR) {
        // Abstract Picture (was mirror) - procedural art
        vec3 pPic = p - vec3(-3.95, 1.8, 2.0);
        vec2 uv = pPic.yz * 0.5 + 0.5; // Normalize to 0-1
        
        // Abstract swirling pattern
        float t = time * 0.3;
        float pattern = sin(uv.x * 8.0 + sin(uv.y * 4.0 + t)) * 0.5 + 0.5;
        pattern += sin(uv.y * 6.0 - cos(uv.x * 3.0 - t * 0.7)) * 0.3;
        pattern += sin(length(uv - 0.5) * 12.0 - t * 2.0) * 0.2;
        
        // Color palette - warm sunset tones
        vec3 col1 = vec3(0.8, 0.3, 0.1);  // Orange
        vec3 col2 = vec3(0.2, 0.1, 0.4);  // Purple
        vec3 col3 = vec3(0.9, 0.6, 0.2);  // Gold
        
        col = mix(col1, col2, pattern);
        col = mix(col, col3, sin(pattern * 3.14159) * 0.5);
        col *= 0.6; // Darken for ambient
        
        // Frame
        if (max(abs(pPic.y) - 1.15, abs(pPic.z) - 0.95) > 0.0) {
            col = vec3(0.08, 0.04, 0.02) * lighting;
        }
    } else if (id == ID_WINDOW) {
        // Window - clean night sky
        vec3 pWindow = p - vec3(3.95, 2.0, 2.0);
        
        // Night sky gradient
        float skyY = (pWindow.y + 1.0) / 2.0;
        vec3 skyColor = mix(vec3(0.01, 0.02, 0.05), vec3(0.005, 0.01, 0.03), skyY);
        
        // Moon (clean circle)
        float moonDist = length(pWindow.yz - vec2(0.5, 0.3));
        float moon = smoothstep(0.12, 0.1, moonDist);
        skyColor = mix(skyColor, vec3(0.95, 0.9, 0.8), moon);
        
        // Subtle moon glow
        float glow = smoothstep(0.4, 0.1, moonDist);
        skyColor += vec3(0.05, 0.04, 0.03) * glow;
        
        col = skyColor;
        
        // Window frame
        if (max(abs(pWindow.y) - 0.95, abs(pWindow.z) - 0.75) > 0.0) {
            col = vec3(0.1, 0.1, 0.12) * lighting;
        }
        // Window cross
        if (abs(pWindow.y) < 0.02 || abs(pWindow.z) < 0.02) {
            col = vec3(0.1, 0.1, 0.12) * lighting;
        }
    } else if (id == ID_TV_SCREEN) {
        col = vec3(0.0);
    }
    
    // Add subtle fresnel to everything for realism
    col += fresnelTint * fresnel * 0.2;
    
    return col * ao;
}

void main() {
    vec2 uv = (v_tex_coords - 0.5) * vec2(width/height, 1.0);

    // Camera - Ping Pong Orbit
    float orbit_range = 0.5; // Smaller arc
    float angle = sin(time * 0.5) * orbit_range;
    
    float camDist = 4.5;
    vec3 ro = vec3(sin(angle) * camDist, 2.2, 1.0 - cos(angle) * camDist);
    vec3 target = vec3(0.0, 1.2, 2.0);
    
    vec3 fwd = normalize(target - ro);
    vec3 right = normalize(cross(vec3(0.0, 1.0, 0.0), fwd));
    vec3 up = cross(fwd, right);
    
    vec3 rd = normalize(fwd + right * uv.x - up * uv.y);

    // Raymarch
    vec2 res = RayMarch(ro, rd);
    float d = res.x;
    int id = int(res.y);
    
    vec3 col = vec3(0.0);
    vec3 lightPos = vec3(2.0, 5.0, -2.0);

    if(d < MAX_DIST) {
        vec3 p = ro + rd * d;
        vec3 n = GetNormal(p);
        
        if (id == ID_TV_SCREEN) {
            // Texture Mapping
            vec3 center = vec3(0.0, 1.5, 3.42);
            vec2 localUV = p.xy - center.xy;
            
            localUV.x = localUV.x / (1.25 * 2.0) + 0.5;
            localUV.y = -localUV.y / (0.6 * 2.0) + 0.5;
            localUV.x = 1.0 - localUV.x;

            if (localUV.x >= 0.0 && localUV.x <= 1.0 && localUV.y >= 0.0 && localUV.y <= 1.0) {
                 col = texture(sampler2D(t_texture, s_sampler), localUV).rgb;
                 col *= 1.2;
            } else {
                col = vec3(0.0);
            }
        } else {
            col = GetMaterial(p, id, n, rd, lightPos);
            
            // Floor Reflection
            if (id == ID_FLOOR) {
                 vec3 rInv = reflect(rd, n);
                 if (rInv.y > 0.0) {
                     // Very simple fake reflection
                 }
            }
        }
    } else {
        col = vec3(0.01, 0.005, 0.0); // Very dark background
    }
    
    col = pow(col, vec3(1.0/2.2));
    f_color = vec4(col, 1.0);
    f_mask_out = 1.0;
}
