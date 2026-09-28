/*@evj { "name": "Wipe", "kind": "transition", "params": [
    { "name": "direction", "min": 0, "max": 3, "default": 0 },
    { "name": "softness", "min": 0, "max": 0.3, "default": 0.03 }
] } */
// direction 0: left to right, 1: right to left, 2: top to bottom, 3: bottom to top
float4 effect(float2 uv) {
    int d = (int)round(direction);
    float c = d == 0 ? uv.x : d == 1 ? 1 - uv.x : d == 2 ? uv.y : 1 - uv.y;
    float s = max(softness, 1e-4);
    float edge = lerp(-s, 1 + s, PROGRESS);
    float m = 1 - smoothstep(edge - s, edge + s, c);
    return lerp(SRC(uv), DST(uv), m);
}
