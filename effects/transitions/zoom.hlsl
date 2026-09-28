/*@evj { "name": "Zoom", "kind": "transition", "params": [
    { "name": "strength", "min": 0.2, "max": 4, "default": 1.5 }
] } */
// The old clip flies towards the viewer while the new one fades in.
float4 effect(float2 uv) {
    float2 p = (uv - 0.5) / (1 + PROGRESS * strength) + 0.5;
    return lerp(SRC(p), DST(uv), PROGRESS);
}
