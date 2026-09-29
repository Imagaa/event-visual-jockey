/*@evj
{ "name": "Chroma Key", "params": [
    { "name": "key_r", "min": 0, "max": 1, "default": 0 },
    { "name": "key_g", "min": 0, "max": 1, "default": 0.69 },
    { "name": "key_b", "min": 0, "max": 1, "default": 0.25 },
    { "name": "tolerance", "min": 0, "max": 0.3, "default": 0.12 },
    { "name": "softness", "min": 0, "max": 0.3, "default": 0.08 },
    { "name": "spill", "min": 0, "max": 1, "default": 0.5 }
] }
*/
// Removes one colour. Distance in YCbCr (BT.709): a coloured key (green / blue screen) compares
// hue and saturation only, so shadows on the screen go too; a neutral key (white / black / grey
// backgrounds) also compares brightness, so grey text stays. Spill takes the key's colour out of
// what remains (green fringes on hair and edges).

float3 to_ycc(float3 c) {
    float y = dot(c, float3(0.2126, 0.7152, 0.0722));
    return float3(y, (c.b - y) / 1.8556, (c.r - y) / 1.5748);
}

float3 to_rgb(float3 ycc) {
    float r = ycc.x + 1.5748 * ycc.z;
    float b = ycc.x + 1.8556 * ycc.y;
    float g = (ycc.x - 0.2126 * r - 0.0722 * b) / 0.7152;
    return float3(r, g, b);
}

float4 effect(float2 uv) {
    float4 c = SRC(uv);
    float3 k = to_ycc(float3(key_r, key_g, key_b));
    float3 p = to_ycc(c.rgb);
    float chroma = length(k.yz);
    float coloured = saturate(chroma * 4);
    float d = length(p.yz - k.yz) + (1 - coloured) * abs(p.x - k.x);
    float alpha = smoothstep(tolerance, tolerance + softness + 1e-4, d);
    // Spill: remove the part of the pixel's colour that points towards the key's colour.
    float2 dir = chroma > 1e-4 ? k.yz / chroma : float2(0, 0);
    float toward = max(dot(p.yz, dir), 0);
    p.yz -= dir * toward * spill * coloured;
    return float4(saturate(to_rgb(p)), c.a * alpha);
}
