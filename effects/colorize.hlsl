/*@evj
{ "name": "Colorize", "params": [
    { "name": "amount", "min": 0, "max": 1, "default": 0 },
    { "name": "red", "min": 0, "max": 1, "default": 1 },
    { "name": "green", "min": 0, "max": 1, "default": 0.6 },
    { "name": "blue", "min": 0, "max": 1, "default": 0.1 }
] }
*/
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    float l = dot(c.rgb, float3(0.2126, 0.7152, 0.0722));
    return float4(lerp(c.rgb, l * float3(red, green, blue), amount), c.a);
}
