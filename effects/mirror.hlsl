/*@evj
{ "name": "Mirror", "params": [
    { "name": "mode", "min": 0, "max": 2, "default": 0 }
] }
*/
// mode 0: left half mirrored to the right, 1: top half mirrored down, 2: quad
float4 effect(float2 uv) {
    int m = (int)round(mode);
    float2 p = uv;
    if (m == 0 || m == 2) p.x = p.x > 0.5 ? 1 - p.x : p.x;
    if (m == 1 || m == 2) p.y = p.y > 0.5 ? 1 - p.y : p.y;
    return SRC(p);
}
