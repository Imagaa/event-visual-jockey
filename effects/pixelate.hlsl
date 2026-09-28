/*@evj
{ "name": "Pixelate", "params": [
    { "name": "size", "min": 1, "max": 160, "default": 1 }
] }
*/
float4 effect(float2 uv) {
    float2 cells = RESOLUTION / max(size, 1);
    return SRC((floor(uv * cells) + 0.5) / cells);
}
