/*@evj
{ "name": "Hue Shift", "params": [
    { "name": "hue", "min": -180, "max": 180, "default": 0 }
] }
*/
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    // Rotate chroma in YIQ space.
    float3 yiq = float3(dot(c.rgb, float3(0.299, 0.587, 0.114)),
                        dot(c.rgb, float3(0.596, -0.274, -0.322)),
                        dot(c.rgb, float3(0.211, -0.523, 0.312)));
    float a = radians(hue);
    float2 iq = float2(yiq.y * cos(a) - yiq.z * sin(a), yiq.y * sin(a) + yiq.z * cos(a));
    float3 rgb = float3(yiq.x + 0.956 * iq.x + 0.621 * iq.y,
                        yiq.x - 0.272 * iq.x - 0.647 * iq.y,
                        yiq.x - 1.106 * iq.x + 1.703 * iq.y);
    return float4(saturate(rgb), c.a);
}
