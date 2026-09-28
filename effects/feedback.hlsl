/*@evj
{ "name": "Feedback", "feedback": true, "params": [
    { "name": "amount", "min": 0, "max": 0.98, "default": 0 },
    { "name": "zoom", "min": 0.9, "max": 1.1, "default": 1 },
    { "name": "rotate", "min": -5, "max": 5, "default": 0 }
] }
*/
// Trails: the previous output, slightly zoomed/rotated and faded, stays under the new image.
float4 effect(float2 uv) {
    float4 c = SRC(uv);
    float2 p = (uv - 0.5) / zoom;
    float a = radians(rotate);
    p = float2(p.x * cos(a) - p.y * sin(a), p.x * sin(a) + p.y * cos(a)) + 0.5;
    float4 prev = PREV(p) * amount;
    return float4(max(c.rgb, prev.rgb), max(c.a, prev.a));
}
