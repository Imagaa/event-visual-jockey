/*@evj
{ "name": "Transform", "params": [
    { "name": "scale", "min": 0.05, "max": 8, "default": 1 },
    { "name": "rotate", "min": -180, "max": 180, "default": 0 },
    { "name": "pos_x", "min": -1, "max": 1, "default": 0 },
    { "name": "pos_y", "min": -1, "max": 1, "default": 0 }
] }
*/
float4 effect(float2 uv) {
    float aspect = RESOLUTION.x / RESOLUTION.y;
    float2 p = (uv - 0.5 - float2(pos_x, -pos_y) * 0.5) * float2(aspect, 1);
    float a = radians(rotate);
    p = float2(p.x * cos(a) - p.y * sin(a), p.x * sin(a) + p.y * cos(a)) / scale;
    p = p / float2(aspect, 1) + 0.5;
    if (any(p < 0) || any(p > 1)) return 0;
    return SRC(p);
}
