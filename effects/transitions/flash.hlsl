/*@evj { "name": "Flash", "kind": "transition", "params": [] } */
// Through white.
float4 effect(float2 uv) {
    float4 a = SRC(uv), b = DST(uv);
    float alpha = max(a.a, b.a);
    return PROGRESS < 0.5 ? float4(lerp(a.rgb, 1, 2 * PROGRESS), lerp(a.a, alpha, 2 * PROGRESS))
                          : float4(lerp(1, b.rgb, 2 * PROGRESS - 1), lerp(alpha, b.a, 2 * PROGRESS - 1));
}
