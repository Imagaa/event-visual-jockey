/*@evj
{ "name": "Kaleidoscope", "params": [
    { "name": "segments", "min": 2, "max": 16, "default": 6 },
    { "name": "rotation", "min": -180, "max": 180, "default": 0 },
    { "name": "zoom", "min": 0.25, "max": 4, "default": 1 }
] }
*/
float4 effect(float2 uv) {
    float aspect = RESOLUTION.x / RESOLUTION.y;
    float2 p = (uv - 0.5) * float2(aspect, 1) / zoom;
    float r = length(p);
    float a = atan2(p.y, p.x) + radians(rotation);
    float seg = 2 * PI / floor(segments);
    a = abs(fmod(abs(a), seg) - seg * 0.5);
    p = float2(cos(a), sin(a)) * r;
    return SRC(p / float2(aspect, 1) + 0.5);
}
