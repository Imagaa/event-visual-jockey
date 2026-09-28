/*@evj
{ "name": "Pointer", "hidden": true, "params": [
    { "name": "px", "min": 0, "max": 1, "default": 0.5 },
    { "name": "py", "min": 0, "max": 1, "default": 0.5 },
    { "name": "spotlight", "min": 0, "max": 1, "default": 0 },
    { "name": "size", "min": 0.005, "max": 0.5, "default": 0.02 },
    { "name": "dim", "min": 0, "max": 1, "default": 0.65 }
] }
*/
// Digital presenter pointer: a red laser dot with glow, or a spotlight that dims everything else.
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    float r = length((uv - float2(px, py)) * float2(RESOLUTION.x / RESOLUTION.y, 1));
    if (spotlight > 0.5) {
        float inside = smoothstep(size * 1.15, size, r);
        return float4(c.rgb * lerp(1 - dim, 1, inside), c.a);
    }
    float core = smoothstep(size, size * 0.55, r);
    float glow = smoothstep(size * 2.5, size, r) * 0.45;
    float a = max(core, glow);
    return float4(lerp(c.rgb, float3(1, 0.12, 0.1), a), max(c.a, a));
}
