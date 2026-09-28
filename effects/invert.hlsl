/*@evj
{ "name": "Invert", "params": [
    { "name": "amount", "min": 0, "max": 1, "default": 1 }
] }
*/
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    return float4(lerp(c.rgb, 1 - c.rgb, amount), c.a);
}
