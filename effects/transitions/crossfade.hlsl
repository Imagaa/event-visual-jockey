/*@evj { "name": "Crossfade", "kind": "transition", "params": [] } */
float4 effect(float2 uv) {
    return lerp(SRC(uv), DST(uv), PROGRESS);
}
