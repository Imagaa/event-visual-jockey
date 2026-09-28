/*@evj
{ "name": "RGB Shift", "params": [
    { "name": "amount", "min": 0, "max": 60, "default": 0 },
    { "name": "angle", "min": -180, "max": 180, "default": 0 }
] }
*/
float4 effect(float2 uv) {
    float a = radians(angle);
    float2 d = float2(cos(a), sin(a)) * amount / RESOLUTION;
    float4 c = SRC(uv);
    return float4(SRC(uv + d).r, c.g, SRC(uv - d).b, c.a);
}
