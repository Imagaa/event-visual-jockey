/*@evj { "name": "Additive", "kind": "transition", "params": [] } */
// Both clips add up in the middle (bright, energetic).
float4 effect(float2 uv) {
    float4 a = SRC(uv) * saturate(2 - 2 * PROGRESS);
    float4 b = DST(uv) * saturate(2 * PROGRESS);
    return float4(saturate(a.rgb + b.rgb), max(a.a, b.a));
}
