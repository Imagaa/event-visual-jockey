/*@evj { "name": "Pixelate", "kind": "transition", "params": [
    { "name": "size", "min": 2, "max": 200, "default": 60 }
] } */
float4 effect(float2 uv) {
    float s = 1 + sin(PROGRESS * PI) * (size - 1);
    float2 cells = RESOLUTION / s;
    float2 p = s < 1.01 ? uv : (floor(uv * cells) + 0.5) / cells;
    return PROGRESS < 0.5 ? SRC(p) : DST(p);
}
