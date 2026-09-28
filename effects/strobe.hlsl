/*@evj
{ "name": "Strobe", "params": [
    { "name": "amount", "min": 0, "max": 1, "default": 0 },
    { "name": "rate", "min": 0.25, "max": 8, "default": 1 },
    { "name": "white", "min": 0, "max": 1, "default": 0 }
] }
*/
// Flashes on the beat: `rate` flashes per beat, to black (white = 0) or white (white = 1).
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    float on = frac(BEAT * rate) < 0.5 ? 0 : 1;
    return float4(lerp(c.rgb, white.xxx, on * amount), c.a);
}
