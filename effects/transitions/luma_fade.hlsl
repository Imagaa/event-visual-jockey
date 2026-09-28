/*@evj { "name": "Luma Fade", "kind": "transition", "params": [
    { "name": "softness", "min": 0.01, "max": 0.5, "default": 0.15 }
] } */
// Dark areas of the old clip give way first.
float4 effect(float2 uv) {
    float4 a = SRC(uv);
    float l = dot(a.rgb, float3(0.2126, 0.7152, 0.0722));
    float t = lerp(-softness, 1 + softness, PROGRESS);
    float m = smoothstep(l - softness, l + softness, t);
    return lerp(a, DST(uv), m);
}
