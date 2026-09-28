/*@evj { "name": "Blur Fade", "kind": "transition", "params": [
    { "name": "radius", "min": 0, "max": 60, "default": 24 }
] } */
float4 blurred(float2 uv, float r, bool dst) {
    float2 px = r / RESOLUTION;
    float4 sum = 0;
    for (int i = 0; i < 12; i++) {
        float k = sqrt((i + 0.5) / 12.0);
        float a = i * 2.39996323;
        float2 o = uv + float2(cos(a), sin(a)) * k * px;
        sum += dst ? DST(o) : SRC(o);
    }
    return sum / 12.0;
}
float4 effect(float2 uv) {
    float r = sin(PROGRESS * PI) * radius;
    if (r < 0.01) return PROGRESS < 0.5 ? SRC(uv) : DST(uv);
    return lerp(blurred(uv, r, false), blurred(uv, r, true), PROGRESS);
}
