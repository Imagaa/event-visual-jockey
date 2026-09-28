# Custom effects and transitions

Put `.hlsl` files in this folder (next to `evj.exe`) or in `%APPDATA%\EVJ\effects`. EVJ picks up new and edited files within a second. A file that does not compile shows its error in *Composition → effects*, and the previous version keeps running.

## Effect

```hlsl
/*@evj
{ "name": "My Tint", "params": [
    { "name": "amount", "min": 0, "max": 1, "default": 0.5 }
] }
*/
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    return float4(lerp(c.rgb, c.rgb * float3(1, 0.6, 0.3), amount), c.a);
}
```

## Transition

Add `"kind": "transition"`. `SRC` is the outgoing picture, `DST` the incoming one, and `PROGRESS` runs from 0 to 1. Both pictures are premultiplied.

```hlsl
/*@evj { "name": "My Fade", "kind": "transition", "params": [] } */
float4 effect(float2 uv) { return lerp(SRC(uv), DST(uv), PROGRESS); }
```

## Available names

| Name | Meaning |
|---|---|
| `SRC(uv)` | input picture (for a transition: the outgoing clip) |
| `DST(uv)` | incoming clip (transitions only) |
| `PREV(uv)` | this effect's previous frame (effects with `"feedback": true`) |
| `TIME` | seconds since EVJ started |
| `BEAT` | beat counter from the BPM clock (1.0 = one beat) |
| `RESOLUTION` | picture size in pixels |
| `PROGRESS` | 0 → 1 during a transition |
| `PI` | 3.14159… |

Parameters (at most 16) become sliders, and any of them can be driven by a BPM LFO. Parameter names must not look like swizzles (`x`, `rgb`, `xy`…). Use `pos_x` instead.
