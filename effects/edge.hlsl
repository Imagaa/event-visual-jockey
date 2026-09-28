/*@evj
{ "name": "Edge", "params": [
    { "name": "amount", "min": 0, "max": 1, "default": 0 },
    { "name": "gain", "min": 0.5, "max": 8, "default": 2 }
] }
*/
float lum(float2 uv) { return dot(SRC(uv).rgb, float3(0.2126, 0.7152, 0.0722)); }
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    if (amount <= 0.001) return c;
    float2 d = 1 / RESOLUTION;
    float gx = lum(uv + float2(d.x, 0)) - lum(uv - float2(d.x, 0));
    float gy = lum(uv + float2(0, d.y)) - lum(uv - float2(0, d.y));
    float e = saturate(length(float2(gx, gy)) * gain);
    return float4(lerp(c.rgb, e.xxx, amount), c.a);
}
