/*@evj
{ "name": "Brightness/Contrast", "params": [
    { "name": "brightness", "min": -1, "max": 1, "default": 0 },
    { "name": "contrast", "min": 0, "max": 3, "default": 1 },
    { "name": "saturation", "min": 0, "max": 3, "default": 1 }
] }
*/
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    float3 rgb = (c.rgb - 0.5) * contrast + 0.5 + brightness;
    float l = dot(rgb, float3(0.2126, 0.7152, 0.0722));
    return float4(saturate(lerp(l.xxx, rgb, saturation)), c.a);
}
