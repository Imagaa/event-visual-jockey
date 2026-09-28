/*@evj
{ "name": "Vignette", "params": [
    { "name": "amount", "min": 0, "max": 1, "default": 0 },
    { "name": "softness", "min": 0.05, "max": 1, "default": 0.45 }
] }
*/
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    float d = length((uv - 0.5) * float2(RESOLUTION.x / RESOLUTION.y, 1));
    float v = smoothstep(0.8, 0.8 - softness, d);
    return float4(c.rgb * lerp(1, v, amount), c.a);
}
