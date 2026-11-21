// Ferrum Nova – Zeitsprung 1996: kleines Bild mit harten Pixeln, feste 32-Farben-Palette mit
// geordnetem Dithering (Bayer 4x4), Röhren-Zeilen und dunkle Bildränder.

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

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
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
