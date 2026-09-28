/*@evj { "name": "Slide", "kind": "transition", "params": [
    { "name": "direction", "min": 0, "max": 3, "default": 0 }
] } */
// The new clip pushes the old one out. direction 0: to the left, 1: right, 2: up, 3: down
float4 effect(float2 uv) {
    int d = (int)round(direction);
    float2 dir = d == 0 ? float2(1, 0) : d == 1 ? float2(-1, 0) : d == 2 ? float2(0, 1) : float2(0, -1);
    float2 a = uv + dir * PROGRESS;
    float2 b = uv + dir * (PROGRESS - 1);
    bool inA = all(a >= 0) && all(a <= 1);
    return inA ? SRC(a) : DST(b);
}
