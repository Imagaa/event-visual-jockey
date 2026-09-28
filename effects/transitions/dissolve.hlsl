/*@evj { "name": "Dissolve", "kind": "transition", "params": [
    { "name": "grain", "min": 1, "max": 32, "default": 2 }
] } */
float hash(float2 p) { return frac(sin(dot(p, float2(12.9898, 78.233))) * 43758.5453); }
float4 effect(float2 uv) {
    float n = hash(floor(uv * RESOLUTION / grain));
    return n < PROGRESS ? DST(uv) : SRC(uv);
}
