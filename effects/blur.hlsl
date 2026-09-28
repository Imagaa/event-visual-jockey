/*@evj
{ "name": "Blur", "params": [
    { "name": "radius", "min": 0, "max": 40, "default": 0 }
] }
*/
float4 effect(float2 uv) {
    if (radius <= 0.001) return SRC(uv);
    // 16-tap Vogel disk: cheap, smooth enough for VJ use.
    float2 px = radius / RESOLUTION;
    float4 sum = 0;
    for (int i = 0; i < 16; i++) {
        float r = sqrt((i + 0.5) / 16.0);
        float a = i * 2.39996323;
        sum += SRC(uv + float2(cos(a), sin(a)) * r * px);
    }
    return sum / 16.0;
}
