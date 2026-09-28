/*@evj
{ "name": "Wave", "params": [
    { "name": "amplitude", "min": 0, "max": 0.2, "default": 0 },
    { "name": "frequency", "min": 0, "max": 30, "default": 4 },
    { "name": "speed", "min": 0, "max": 10, "default": 1 }
] }
*/
float4 effect(float2 uv) {
    float2 p = uv;
    p.x += sin(uv.y * frequency * 2 * PI + TIME * speed) * amplitude;
    p.y += cos(uv.x * frequency * 2 * PI + TIME * speed) * amplitude * 0.5;
    return SRC(p);
}
