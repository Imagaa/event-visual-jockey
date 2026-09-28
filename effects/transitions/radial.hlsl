/*@evj { "name": "Radial", "kind": "transition", "params": [
    { "name": "softness", "min": 0, "max": 0.2, "default": 0.02 }
] } */
// Clock wipe from 12 o'clock, clockwise.
float4 effect(float2 uv) {
    float2 p = (uv - 0.5) * float2(RESOLUTION.x / RESOLUTION.y, 1);
    float a = frac(atan2(p.x, -p.y) / (2 * PI) + 1);
    float s = max(softness, 1e-4);
    float t = lerp(-s, 1 + s, PROGRESS);
    float m = 1 - smoothstep(t - s, t + s, a);
    return lerp(SRC(uv), DST(uv), m);
}
