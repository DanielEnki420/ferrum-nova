// Ferrum Nova – Fragment-Shader für Pflanzen und Felsen (Wind-Material): übliche Beleuchtung,
// danach Nebel des Krieges. Eigene Datei, weil WebGL Vertex- und Fragment-Teil getrennt braucht.

import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};

@group(constants::MATERIAL_BIND_GROUP) @binding(101) var fog_tex: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var fog_sampler: sampler;

// Nebel des Krieges: Helligkeit je Meter aus einer kleinen Textur (0 unerkundet, 0.5 erkundet, 1 in Sicht)
const FOG_HALF: f32 = 70.0;

fn fog_of_war(xz: vec2<f32>) -> f32 {
    let uv = (xz + FOG_HALF) / (2.0 * FOG_HALF);
    let f = textureSampleLevel(fog_tex, fog_sampler, clamp(uv, vec2(0.0), vec2(1.0)), 0.0).r;
    // außerhalb des Spielfelds: gedämpft wie erkundetes Gelände
    let inside = all(uv >= vec2(0.0)) && all(uv <= vec2(1.0));
    return select(0.5, f, inside);
}

// Abdunkeln und (im nur erkundeten Gelände) entsättigen
fn fog_shade(c: vec3<f32>, xz: vec2<f32>) -> vec3<f32> {
    let f = fog_of_war(xz);
    let grey = vec3(dot(c, vec3(0.2126, 0.7152, 0.0722)));
    return mix(grey, c, 0.35 + 0.65 * f) * (0.06 + 0.94 * pow(f, 1.2));
}


// Übliche Beleuchtung, danach Nebel des Krieges
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    out.color = vec4(fog_shade(out.color.rgb, in.world_position.xz), out.color.a);
    return out;
}
