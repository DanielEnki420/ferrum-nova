// Ferrum Nova – Zeitsprung (params.w wählt die Darstellung):
// 1996: kleines Bild mit harten Pixeln, feste 32-Farben-Palette mit geordnetem Dithering
//       (Bayer 4x4), Röhren-Zeilen und dunkle Bildränder.
// 2026 sparsam (params.w = 2): einfache Auflösung, weich hochgerechnet (Safari, ?spar=1).
// 2000: 480 Zeilen, weich hochgerechnet, 16 Bit Farbtiefe (RGB565) mit feinem 4x4-Dithering,
//       dadurch sichtbare Farbstufen; kaum dunkle Ränder.

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0) var<uniform> params: vec4<f32>;
@group(1) @binding(1) var screen_tex: texture_2d<f32>;
@group(1) @binding(2) var screen_sampler: sampler;

// Palette (sRGB): Grautöne, Erde, Gras, Spielerblau, Gegnerrot, Feuer, Ferrit, Plasma, Dunst
const PALETTE = array<vec3<f32>, 32>(
    vec3(0.047, 0.047, 0.063), vec3(0.141, 0.141, 0.165), vec3(0.267, 0.267, 0.298), vec3(0.424, 0.424, 0.455),
    vec3(0.612, 0.612, 0.627), vec3(0.816, 0.816, 0.784), vec3(0.973, 0.973, 0.941), vec3(0.690, 0.675, 0.627),
    vec3(0.173, 0.118, 0.078), vec3(0.290, 0.196, 0.125), vec3(0.431, 0.298, 0.173), vec3(0.580, 0.408, 0.235),
    vec3(0.722, 0.549, 0.345), vec3(0.847, 0.706, 0.486), vec3(0.110, 0.173, 0.078), vec3(0.204, 0.282, 0.110),
    vec3(0.314, 0.408, 0.173), vec3(0.471, 0.565, 0.251), vec3(0.110, 0.173, 0.361), vec3(0.188, 0.329, 0.627),
    vec3(0.361, 0.533, 0.816), vec3(0.627, 0.769, 0.941), vec3(0.361, 0.094, 0.063), vec3(0.627, 0.173, 0.110),
    vec3(0.878, 0.314, 0.188), vec3(0.941, 0.627, 0.188), vec3(0.973, 0.878, 0.439), vec3(0.063, 0.502, 0.424),
    vec3(0.251, 0.878, 0.690), vec3(0.627, 0.173, 0.690), vec3(0.878, 0.439, 0.941), vec3(0.502, 0.565, 0.659),
);

const BAYER = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

// Ein Bildpunkt so, wie ihn eine 16-Bit-Grafikkarte im Bildspeicher ablegt (sRGB, 5/6/5 Bit)
fn texel_565(p: vec2<i32>, size: vec2<i32>) -> vec3<f32> {
    let q = clamp(p, vec2(0), size - 1);
    let lin = textureLoad(screen_tex, q, 0).rgb;
    let c = to_srgb(clamp(lin, vec3(0.0), vec3(1.0)));
    var bayer = BAYER;
    let d = (bayer[u32(q.y % 4) * 4u + u32(q.x % 4)] + 0.5) / 16.0 - 0.5;
    let steps = vec3(31.0, 63.0, 31.0);
    return clamp(floor(c * steps + 0.5 + d), vec3(0.0), steps) / steps;
}

fn fragment_2000(in: UiVertexOutput) -> vec4<f32> {
    let size = params.xy;
    let isize = vec2<i32>(size);
    // Bilineare Hochrechnung zwischen den bereits auf 16 Bit gebrachten Bildpunkten
    let px = in.uv * size - 0.5;
    let base = vec2<i32>(floor(px));
    let f = fract(px);
    let a = mix(texel_565(base, isize), texel_565(base + vec2(1, 0), isize), f.x);
    let b = mix(texel_565(base + vec2(0, 1), isize), texel_565(base + vec2(1, 1), isize), f.x);
    let c = mix(a, b, f.y);
    let q = in.uv - 0.5;
    let vignette = 1.0 - dot(q, q) * 0.18;
    return vec4(to_linear(clamp(c * vignette, vec3(0.0), vec3(1.0))), 1.0);
}

// 2026 im Speichersparmodus: Bild in einfacher Auflösung weich hochrechnen, dunkle Ränder wie sonst
fn fragment_2026(in: UiVertexOutput) -> vec4<f32> {
    let lin = textureSampleLevel(screen_tex, screen_sampler, in.uv, 0.0).rgb;
    let d = length(in.uv - 0.5) * 1.414;
    let shade = mix(1.0, 0.6, smoothstep(0.45, 1.15, d));
    return vec4(lin * shade, 1.0);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    if params.w > 1.5 {
        return fragment_2026(in);
    }
    if params.w > 0.5 {
        return fragment_2000(in);
    }
    let size = params.xy;
    let px = in.uv * size;
    let cell = min(floor(px), size - 1.0);
    let lin = textureSampleLevel(screen_tex, screen_sampler, (cell + 0.5) / size, 0.0).rgb;
    var c = to_srgb(clamp(lin, vec3(0.0), vec3(1.0)));

    // Geordnetes Dithering: Schwelle je Pixel aus der Bayer-Matrix
    var bayer = BAYER;
    let ix = u32(cell.x) % 4u;
    let iy = u32(cell.y) % 4u;
    c += (bayer[iy * 4u + ix] + 0.5) / 16.0 * 0.11 - 0.055;

    // Nächste Palettenfarbe (grob nach Helligkeitsempfinden gewichtet)
    var pal = PALETTE;
    var best = pal[0];
    var best_d = 1e9;
    for (var i = 0u; i < 32u; i++) {
        let e = c - pal[i];
        let d = dot(e * e, vec3(2.0, 4.0, 3.0));
        if d < best_d {
            best_d = d;
            best = pal[i];
        }
    }

    // Röhre: dunkle Fugen zwischen den Bildzeilen, leichtes Flimmern, dunkle Ränder
    let scan = 0.80 + 0.20 * sin(3.14159 * fract(px.y));
    let flicker = 1.0 + 0.012 * sin(params.z * 60.0);
    let q = in.uv - 0.5;
    let vignette = 1.0 - dot(q, q) * 0.7;
    let out = best * scan * flicker * vignette;
    return vec4(to_linear(clamp(out, vec3(0.0), vec3(1.0))), 1.0);
}
