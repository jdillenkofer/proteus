#version 450

// Jack-o'-lantern shader
// Ported from Shadertoy by @P_Malin

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

#define kRaymarchMaxIter 64
#define kBounceCount 1

float kFarClip = 100.0;

vec3 vLightPos = vec3(0.0, -0.5, 0.0);
vec3 vLightColour = vec3(1.0, 0.8, 0.4);

float fCarving = 1.0;

// Hash function
float hash(float p) {
    vec2 p2 = fract(vec2(p * 5.3983, p * 5.4427));
    p2 += dot(p2.yx, p2.xy + vec2(21.5351, 14.3137));
    return fract(p2.x * p2.y * 95.4337);
}

// CAMERA
vec2 GetWindowCoord(vec2 vUV) {
    vec2 vWindow = vUV * 2.0 - 1.0;
    vWindow.x *= width / height;
    return vWindow;
}

vec3 GetCameraRayDir(vec2 vWindow, vec3 vCameraPos, vec3 vCameraTarget) {
    vec3 vForward = normalize(vCameraTarget - vCameraPos);
    vec3 vRight = normalize(cross(vec3(0.0, 1.0, 0.0), vForward));
    vec3 vUp = normalize(cross(vForward, vRight));
    
    vec3 vDir = normalize(vWindow.x * vRight + vWindow.y * vUp + vForward * 1.5);
    return vDir;
}

// POSTFX
vec3 ApplyVignetting(vec2 vUV, vec3 vInput) {
    vec2 vOffset = (vUV - 0.5) * sqrt(2.0);
    float fDist = dot(vOffset, vOffset);
    
    float kStrength = 0.95;
    float kPower = 1.5;
    
    return vInput * ((1.0 - kStrength) + kStrength * pow(1.0 - fDist, kPower));
}

vec3 ApplyTonemap(vec3 vLinear) {
    float kExposure = 1.0;
    
    if(time < 2.0) {
        kExposure = time / 2.0;
    }
    
    return 1.0 - exp2(vLinear * -kExposure);
}

vec3 ApplyGamma(vec3 vLinear) {
    float kGamma = 2.2;
    return pow(vLinear, vec3(1.0/kGamma));
}

vec3 ApplyBlackLevel(vec3 vColour) {
    float fBlackLevel = 0.1;
    return vColour / (1.0 - fBlackLevel) - fBlackLevel;
}

vec3 ApplyPostFX(vec2 vUV, vec3 vInput) {
    vec3 vTemp = ApplyVignetting(vUV, vInput);
    vTemp = ApplyTonemap(vTemp);
    vTemp = ApplyGamma(vTemp);
    vTemp = ApplyBlackLevel(vTemp);
    return vTemp;
}

// RAYTRACE
struct C_Intersection {
    vec3 vPos;
    float fDist;
    vec3 vNormal;
    vec3 vUVW;
    float fObjectId;
};

float GetCarving2dDistance(vec2 vPos) {
    if(fCarving < 0.0)
        return 10.0;
    
    float fMouthDist = length(vPos.xy + vec2(0.0, -0.5)) - 1.5;
    float fMouthDist2 = length(vPos.xy + vec2(0.0, -1.1 - 0.5)) - 2.0;
    
    if(-fMouthDist2 > fMouthDist) {
        fMouthDist = -fMouthDist2;
    }

    float fFaceDist = fMouthDist;

    vec2 vNosePos = vPos.xy + vec2(0.0, -0.5);
    vNosePos.x = abs(vNosePos.x);
    float fNoseDist = dot(vNosePos.xy, normalize(vec2(1.0, 0.5)));
    fNoseDist = max(fNoseDist, -(vNosePos.y + 0.5));
    if(fNoseDist < fFaceDist) {
        fFaceDist = fNoseDist;
    }

    vec2 vEyePos = vPos.xy;
    vEyePos.x = abs(vEyePos.x);
    vEyePos.x -= 1.0;
    vEyePos.y -= 1.0;
    float fEyeDist = dot(vEyePos.xy, normalize(vec2(-1.0, 1.5)));
    fEyeDist = max(fEyeDist, dot(vEyePos.xy, normalize(vec2(1.0, 0.5))));
    fEyeDist = max(fEyeDist, -0.5 + dot(vEyePos.xy, normalize(vec2(0.0, -1.0))));
    if(fEyeDist < fFaceDist) {
        fFaceDist = fEyeDist;
    }
    
    return fFaceDist;
}

float GetCarvingDistance(vec3 vPos) {
    float fDist = (length(vPos * vec3(1.0, 1.4, 1.0)) - 2.7) / 1.5;

    float fFaceDist = GetCarving2dDistance(vPos.xy);
    
    float fRearDist = vPos.z;
    
    if(fRearDist > fFaceDist) {
        fFaceDist = fRearDist;
    }
    
    if(fFaceDist < fDist) {
        fDist = fFaceDist;
    }

    float fR = length(vPos.xz);
    
    float fLidDist = dot(vec2(fR, vPos.y), normalize(vec2(1.0, -1.5)));
    
    fLidDist = abs(fLidDist) - 0.03;
    if(fLidDist < fDist) {
        fDist = fLidDist;
    }
    
    return fDist;
}

float GetPumpkinDistance(out vec4 vOutUVW_Id, vec3 vPos) {
    vec3 vSphereOrigin = vec3(0.0, 0.0, 0.0);
    float fSphereRadius = 3.0;

    vec3 vOffset = vPos - vSphereOrigin;
    float fFirstDist = length(vOffset);
    
    float fOutDist;
    if(fFirstDist > 3.5) {
        fOutDist = fFirstDist - fSphereRadius;
    } else {
        float fAngle1 = atan(vOffset.x, vOffset.z);
        float fSin = sin(fAngle1 * 10.0);
        fSin = 1.0 - sqrt(abs(fSin));
        vOffset *= 1.0 + fSin * vec3(0.05, 0.025, 0.05);
        vOffset.y *= 1.0 + 0.5 * (fSphereRadius - length(vOffset.xz)) / fSphereRadius;
        fOutDist = length(vOffset) - fSphereRadius;
    }

    vec4 vSphere1UVW_Id = vec4(normalize(vPos - vSphereOrigin), 3.0);
    vOutUVW_Id = vSphere1UVW_Id;
    
    vec3 vStalkOffset = vPos;
    vStalkOffset.x += -(vStalkOffset.y - fSphereRadius) * 0.1;
    float fDist2d = length(vStalkOffset.xz);
    float fStalkDist = fDist2d - 0.2;
    fStalkDist = max(fStalkDist, vPos.y - 2.5 + vPos.x * 0.25);
    fStalkDist = max(fStalkDist, -vPos.y);
    if(fStalkDist < fOutDist) {
        fOutDist = fStalkDist;
        vOutUVW_Id = vSphere1UVW_Id;
        vOutUVW_Id.w = 2.0;
    }

    return fOutDist;
}

float GetSceneDistance(out vec4 vOutUVW_Id, vec3 vPos) {
    float fFloorDist = vPos.y + 2.0;
    vec4 vFloorUVW_Id = vec4(vPos.xz, 0.0, 1.0);

    vec3 vPumpkinDomain = vPos;
    
    float fOutDist = fFloorDist;
    vOutUVW_Id = vFloorUVW_Id;

    vec4 vPumpkinUVW_Id;
    float fPumpkinDist = GetPumpkinDistance(vPumpkinUVW_Id, vPumpkinDomain);

    float fCarvingDist = GetCarvingDistance(vPumpkinDomain);
    
    if(-fCarvingDist > fPumpkinDist) {
        fPumpkinDist = -fCarvingDist;
        vPumpkinUVW_Id = vec4(4.0);
    }

    if(fPumpkinDist < fOutDist) {
        fOutDist = fPumpkinDist;
        vOutUVW_Id = vPumpkinUVW_Id;
    }
    
    return fOutDist;
}

vec3 GetSceneNormal(vec3 vPos) {
    float fDelta = 0.001;

    vec3 vDir1 = vec3( 1.0, -1.0, -1.0);
    vec3 vDir2 = vec3(-1.0, -1.0,  1.0);
    vec3 vDir3 = vec3(-1.0,  1.0, -1.0);
    vec3 vDir4 = vec3( 1.0,  1.0,  1.0);
    
    vec3 vOffset1 = vDir1 * fDelta;
    vec3 vOffset2 = vDir2 * fDelta;
    vec3 vOffset3 = vDir3 * fDelta;
    vec3 vOffset4 = vDir4 * fDelta;

    vec4 vUnused;
    float f1 = GetSceneDistance(vUnused, vPos + vOffset1);
    float f2 = GetSceneDistance(vUnused, vPos + vOffset2);
    float f3 = GetSceneDistance(vUnused, vPos + vOffset3);
    float f4 = GetSceneDistance(vUnused, vPos + vOffset4);
    
    vec3 vNormal = vDir1 * f1 + vDir2 * f2 + vDir3 * f3 + vDir4 * f4;
    
    return normalize(vNormal);
}

void TraceScene(out C_Intersection outIntersection, vec3 vOrigin, vec3 vDir) {
    vec4 vUVW_Id = vec4(0.0);
    vec3 vPos = vec3(0.0);
    
    float t = 0.01;
    for(int i = 0; i < kRaymarchMaxIter; i++) {
        vPos = vOrigin + vDir * t;
        float fDist = GetSceneDistance(vUVW_Id, vPos);
        t += fDist;
        if(abs(fDist) < 0.001) {
            break;
        }
        if(t > 100.0) {
            t = kFarClip;
            vPos = vOrigin + vDir * t;
            vUVW_Id = vec4(0.0);
            break;
        }
    }
    
    outIntersection.fDist = t;
    outIntersection.vPos = vPos;
    outIntersection.vNormal = GetSceneNormal(vPos);
    outIntersection.vUVW = vUVW_Id.xyz;
    outIntersection.fObjectId = vUVW_Id.w;
}

float TraceShadow(vec3 vOrigin, vec3 vDir, float fDistParam) {
    C_Intersection shadowIntersection;
    TraceScene(shadowIntersection, vOrigin, vDir);
    if(shadowIntersection.fDist < fDistParam) {
        return 0.0;
    }
    return 1.0;
}

float GetSSS(vec3 vPos, vec3 vLightPosLocal) {
    vec3 vLightToPos = vPos - vLightPosLocal;
    vec3 vDir = normalize(vLightToPos);
    
    C_Intersection intersection;
    TraceScene(intersection, vLightPosLocal, vDir);
    float fOpticalDepth = length(vLightToPos) - intersection.fDist;

    fOpticalDepth = max(0.00001, fOpticalDepth);
    
    return exp2(fOpticalDepth * -8.0);
}

// LIGHTING
float GIV(float dotNV, float k) {
    return 1.0 / ((dotNV + 0.0001) * (1.0 - k) + k);
}

void AddLighting(inout vec3 vDiffuseLight, inout vec3 vSpecularLight, vec3 vViewDir, vec3 vLightDir, vec3 vNormal, float fSmoothness, vec3 vLightColourLocal) {
    vec3 vH = normalize(-vViewDir + vLightDir);
    float fNDotL = clamp(dot(vLightDir, vNormal), 0.0, 1.0);
    float fNDotV = clamp(dot(-vViewDir, vNormal), 0.0, 1.0);
    float fNDotH = clamp(dot(vNormal, vH), 0.0, 1.0);
    
    float alpha = 1.0 - fSmoothness;
    alpha = alpha * alpha;

    float alphaSqr = alpha * alpha;
    float pi = 3.14159;
    float denom = fNDotH * fNDotH * (alphaSqr - 1.0) + 1.0;
    float d = alphaSqr / (pi * denom * denom);

    float k = alpha / 2.0;
    float vis = GIV(fNDotL, k) * GIV(fNDotV, k);

    float fSpecularIntensity = d * vis * fNDotL;
    vSpecularLight += vLightColourLocal * fSpecularIntensity;

    vDiffuseLight += vLightColourLocal * fNDotL;
}

void AddPointLight(inout vec3 vDiffuseLight, inout vec3 vSpecularLight, vec3 vViewDir, vec3 vPos, vec3 vNormal, float fSmoothness, vec3 vLightPosLocal, vec3 vLightColourLocal) {
    vec3 vToLight = vLightPosLocal - vPos;
    float fDistance2 = dot(vToLight, vToLight);
    float fAttenuation = 100.0 / (fDistance2);
    vec3 vLightDir = normalize(vToLight);
    
    vec3 vShadowRayDir = vLightDir;
    vec3 vShadowRayOrigin = vPos + vShadowRayDir * 0.01;
    float fShadowFactor = TraceShadow(vShadowRayOrigin, vShadowRayDir, length(vToLight));
    
    AddLighting(vDiffuseLight, vSpecularLight, vViewDir, vLightDir, vNormal, fSmoothness, vLightColourLocal * fShadowFactor * fAttenuation);
}

float AddDirectionalLight(inout vec3 vDiffuseLight, inout vec3 vSpecularLight, vec3 vViewDir, vec3 vPos, vec3 vNormal, float fSmoothness, vec3 vLightDir, vec3 vLightColourLocal) {
    float fAttenuation = 1.0;

    vec3 vShadowRayDir = -vLightDir;
    vec3 vShadowRayOrigin = vPos + vShadowRayDir * 0.01;
    float fShadowFactor = TraceShadow(vShadowRayOrigin, vShadowRayDir, 10.0);
    
    AddLighting(vDiffuseLight, vSpecularLight, vViewDir, -vLightDir, vNormal, fSmoothness, vLightColourLocal * fShadowFactor * fAttenuation);
    
    return fShadowFactor;
}

void AddDirectionalLightFlareToFog(inout vec3 vFogColour, vec3 vRayDir, vec3 vLightDir, vec3 vLightColourLocal) {
    float fDirDot = clamp(dot(-vLightDir, vRayDir), 0.0, 1.0);
    float kSpreadPower = 4.0;
    vFogColour += vLightColourLocal * pow(fDirDot, kSpreadPower);
}

// SCENE MATERIALS
void GetSurfaceInfo(out vec3 vOutAlbedo, out vec3 vOutR0, out float fOutSmoothness, out vec3 vOutBumpNormal, C_Intersection intersection) {
    vOutBumpNormal = intersection.vNormal;
    
    if(intersection.fObjectId == 1.0) {
        // Floor - smooth dark ground
        vOutAlbedo = vec3(0.15, 0.12, 0.1);
        fOutSmoothness = 0.3;
        vOutR0 = vec3(0.02);
    }
    else if(intersection.fObjectId == 2.0) {
        // Stalk - brown/green
        vOutAlbedo = vec3(0.4, 0.35, 0.15);
        fOutSmoothness = 0.3;
        vOutR0 = vec3(0.04);
    }
    else if(intersection.fObjectId == 3.0) {
        // Pumpkin surface - smooth orange gradient based on height
        float heightFactor = intersection.vUVW.y * 0.5 + 0.5;
        vec3 vCol1 = vec3(1.0, 0.5, 0.0);  // Bright orange
        vec3 vCol2 = vec3(0.6, 0.2, 0.0);  // Dark orange
        vOutAlbedo = mix(vCol2, vCol1, heightFactor);
        fOutSmoothness = 0.6;
        vOutR0 = vec3(0.05);
    }
    else if(intersection.fObjectId == 4.0) {
        // Inside carved area
        vOutAlbedo = vec3(1.0, 0.824, 0.301);
        fOutSmoothness = 0.4;
        vOutR0 = vec3(0.05);
    }
    else {
        vOutAlbedo = vec3(0.5);
        fOutSmoothness = 0.5;
        vOutR0 = vec3(0.04);
    }
}

vec3 GetSkyColour(vec3 vDir) {
    vec3 vResult = mix(vec3(0.02, 0.04, 0.06), vec3(0.1, 0.5, 0.8), abs(vDir.y));
    return vResult;
}

float GetFogFactor(float fDistParam) {
    float kFogDensity = 0.025;
    return exp(fDistParam * -kFogDensity);
}

vec3 GetFogColour(vec3 vDir) {
    return vec3(0.01);
}

vec3 vSunLightColour = vec3(0.1, 0.2, 0.3) * 5.0;
vec3 vSunLightDir = normalize(vec3(0.4, -0.3, -0.5));

void ApplyAtmosphere(inout vec3 vColour, float fDistParam, vec3 vRayOrigin, vec3 vRayDir) {
    float fFogFactor = GetFogFactor(fDistParam);
    vec3 vFogColour = GetFogColour(vRayDir);
    AddDirectionalLightFlareToFog(vFogColour, vRayDir, vSunLightDir, vSunLightColour);
    
    vColour = mix(vFogColour, vColour, fFogFactor);
}

// TRACING LOOP
vec3 GetSceneColour(vec3 _vRayOrigin, vec3 _vRayDir, vec3 vLightColourLocal) {
    vec3 vRayOrigin = _vRayOrigin;
    vec3 vRayDir = _vRayDir;
    vec3 vColour = vec3(0.0);
    vec3 vRemaining = vec3(1.0);
    
    float fLastShadow = 1.0;
    
    for(int i = 0; i < kBounceCount; i++) {
        vec3 vCurrRemaining = vRemaining;
        float fShouldApply = 1.0;
        
        C_Intersection intersection;
        TraceScene(intersection, vRayOrigin, vRayDir);

        vec3 vResult = vec3(0.0);
        vec3 vBlendFactor = vec3(0.0);
                        
        if(intersection.fObjectId == 0.0) {
            vBlendFactor = vec3(1.0);
            fShouldApply = 0.0;
        }
        else {
            vec3 vAlbedo;
            vec3 vR0;
            float fSmoothness;
            vec3 vBumpNormal;
            
            GetSurfaceInfo(vAlbedo, vR0, fSmoothness, vBumpNormal, intersection);
        
            vec3 vDiffuseLight = vec3(0.0);
            vec3 vSpecularLight = vec3(0.0);

            fLastShadow = AddDirectionalLight(vDiffuseLight, vSpecularLight, vRayDir, intersection.vPos, vBumpNormal, fSmoothness, vSunLightDir, vSunLightColour);

            vec3 vPointLightPos = vLightPos;
            
            AddPointLight(vDiffuseLight, vSpecularLight, vRayDir, intersection.vPos, vBumpNormal, fSmoothness, vPointLightPos, vLightColourLocal);

            if(intersection.fObjectId >= 3.0) {
                vDiffuseLight += GetSSS(intersection.vPos, vPointLightPos) * vLightColourLocal;
            }
            else {
                vec3 vToLight = vPointLightPos - intersection.vPos;
                float fNdotL = dot(normalize(vToLight), vBumpNormal) * 0.5 + 0.5;
                vDiffuseLight += max(0.0, 1.0 - length(vToLight)/5.0) * vLightColourLocal * fNdotL;
            }

            float fSmoothFactor = fSmoothness * 0.9 + 0.1;
            float fFresnelClamp = 0.25;
            float fNdotD = clamp(dot(vBumpNormal, -vRayDir), fFresnelClamp, 1.0);
            vec3 vFresnel = vR0 + (1.0 - vR0) * pow(1.0 - fNdotD, 5.0) * fSmoothFactor;

            vResult = mix(vAlbedo * vDiffuseLight, vSpecularLight, vFresnel);
            vBlendFactor = vFresnel;
            
            ApplyAtmosphere(vResult, intersection.fDist, vRayOrigin, vRayDir);
            
            vRemaining *= vBlendFactor;
            vRayDir = normalize(reflect(vRayDir, vBumpNormal));
            vRayOrigin = intersection.vPos;
        }

        vColour += vResult * vCurrRemaining * fShouldApply;
    }

    vec3 vSkyColor = GetSkyColour(vRayDir);
    
    ApplyAtmosphere(vSkyColor, kFarClip, vRayOrigin, vRayDir);
    
    vSkyColor *= fLastShadow;
    
    vColour += vSkyColor * vRemaining;
    
    // Face glow
    float t = -(_vRayOrigin.z + 2.8) / _vRayDir.z;
    
    if(t > 0.0) {
        vec3 vPos = _vRayOrigin + _vRayDir * t;

        float fDistGlow = abs(GetCarving2dDistance(vPos.xy * vec2(1.0, 1.0)));
        float fDot = max(0.0, _vRayDir.z);
        fDot = fDot * fDot;
        vColour += exp2(-fDistGlow * 10.0) * fDot * vLightColourLocal * 0.25;
    }
    
    return vColour;
}

void main() {
    vec2 vUV = v_tex_coords;
    vUV.y = 1.0 - vUV.y; // Flip Y for correct orientation
    
    // Flickering light effect
    vec3 vLightColourAnimated = vLightColour * (hash(time) * 0.2 + 0.8);
    
    float fDistCam = 7.0;
    float fAngle = radians(190.0) + sin(time * 0.25) * 0.2;
    float fHeight = 2.0 + sin(time * 0.1567) * 1.5;
    
    vec3 vCameraPos = vec3(sin(fAngle) * fDistCam, fHeight, cos(fAngle) * fDistCam);
    vec3 vCameraTarget = vec3(0.0, -0.5, 0.0);

    vec3 vRayOrigin = vCameraPos;
    vec3 vRayDir = GetCameraRayDir(GetWindowCoord(vUV), vCameraPos, vCameraTarget);
    
    vec3 vResult = GetSceneColour(vRayOrigin, vRayDir, vLightColourAnimated);
        
    vec3 vFinal = ApplyPostFX(vUV, vResult);
    
    // Person composition
    float mask = texture(sampler2D(t_mask, s_sampler), v_tex_coords).r;
    vec4 person_color = texture(sampler2D(t_texture, s_sampler), v_tex_coords);
    mask = smoothstep(0.4, 0.6, mask);
    
    f_color = vec4(mix(vFinal, person_color.rgb, mask), 1.0);
}
