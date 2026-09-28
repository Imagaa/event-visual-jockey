/*@evj
{ "name": "Posterize", "params": [
    { "name": "levels", "min": 2, "max": 256, "default": 256 }
] }
*/
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    float n = max(floor(levels) - 1, 1);
    return float4(round(c.rgb * n) / n, c.a);
}
