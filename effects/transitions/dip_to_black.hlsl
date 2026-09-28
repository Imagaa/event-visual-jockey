/*@evj { "name": "Dip to Black", "kind": "transition", "params": [] } */
// Out to black, then in from black (stays opaque, so layers below do not show through).
float4 effect(float2 uv) {
    float4 a = SRC(uv), b = DST(uv);
    float alpha = max(a.a, b.a);
    return PROGRESS < 0.5 ? float4(a.rgb * (1 - 2 * PROGRESS), lerp(a.a, alpha, 2 * PROGRESS))
                          : float4(b.rgb * (2 * PROGRESS - 1), lerp(alpha, b.a, 2 * PROGRESS - 1));
}
