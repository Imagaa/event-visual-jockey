/*@evj
{ "name": "Test Pattern", "params": [
    { "name": "number", "min": 0, "max": 9, "default": 1 }
] }
*/
// Output setup card: border, grid, colour bars, centre circle and a big output number.
float seg(float2 p, float2 a, float2 b) {
    float2 pa = p - a, ba = b - a;
    float h = saturate(dot(pa, ba) / dot(ba, ba));
    return length(pa - ba * h);
}
float digit(float2 p, int n) {
    // segments a..g as bits 0..6
    static const int masks[10] = { 0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F };
    int m = masks[clamp(n, 0, 9)];
    float d = 1e3;
    if (m & 1)  d = min(d, seg(p, float2(-0.5, 1), float2(0.5, 1)));
    if (m & 2)  d = min(d, seg(p, float2(0.5, 1), float2(0.5, 0)));
    if (m & 4)  d = min(d, seg(p, float2(0.5, 0), float2(0.5, -1)));
    if (m & 8)  d = min(d, seg(p, float2(-0.5, -1), float2(0.5, -1)));
    if (m & 16) d = min(d, seg(p, float2(-0.5, 0), float2(-0.5, -1)));
    if (m & 32) d = min(d, seg(p, float2(-0.5, 1), float2(-0.5, 0)));
    if (m & 64) d = min(d, seg(p, float2(-0.5, 0), float2(0.5, 0)));
    return d;
}
float4 effect(float2 uv) {
    float2 px = uv * RESOLUTION;
    float3 c = float3(0.12, 0.12, 0.14);
    // grid every 1/16 of the height
    float cell = RESOLUTION.y / 16;
    float2 g = abs(frac(px / cell + 0.5) - 0.5) * cell;
    if (min(g.x, g.y) < 1) c = 0.45;
    // colour bars across the top eighth
    if (uv.y < 0.125) {
        static const float3 bars[7] = { float3(1,1,1), float3(1,1,0), float3(0,1,1), float3(0,1,0), float3(1,0,1), float3(1,0,0), float3(0,0,1) };
        c = bars[min((int)(uv.x * 7), 6)] * 0.75;
    }
    // centre circle (aspect corrected) and cross
    float2 p = (uv - 0.5) * float2(RESOLUTION.x / RESOLUTION.y, 1);
    if (abs(length(p) - 0.4) < 0.004) c = 1;
    if (min(abs(p.x), abs(p.y)) < 0.0015) c = 0.8;
    // border
    if (px.x < 3 || px.y < 3 || px.x > RESOLUTION.x - 4 || px.y > RESOLUTION.y - 4) c = float3(1, 0.2, 0.2);
    // big number
    float d = digit(float2(p.x, -p.y) / 0.14, (int)round(number));
    if (d < 0.12) c = float3(1, 0.85, 0.2);
    return float4(c, 1);
}
