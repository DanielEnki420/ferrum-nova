// Ferrum Nova – Gelände-Splatting
// Schichten im Array: 0 Erde, 1 Geröll, 2 Fels, 3 Gras. Gewichte kommen aus der Vertexfarbe.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};

struct TerrainParams {
    tile: f32,
    macro_scale: f32,
    normal_strength: f32,
    rock_tile: f32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> params: TerrainParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var albedo_tex: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var tex_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var normal_tex: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var arm_tex: texture_2d_array<f32>;

struct Layer {
    albedo: vec3<f32>,
    // Störung der Normalen in Weltkoordinaten (noch ohne Grundnormale)
    bump: vec3<f32>,
    ao: f32,
    rough: f32,
};

fn tangent_normal(uv: vec2<f32>, i: i32) -> vec2<f32> {
    return textureSample(normal_tex, tex_sampler, uv, i).xy * 2.0 - 1.0;
}

// Draufsicht-Projektion (u = +x, v = +z)
fn planar(uv: vec2<f32>, i: i32) -> Layer {
    var l: Layer;
    l.albedo = textureSample(albedo_tex, tex_sampler, uv, i).rgb;
    let t = tangent_normal(uv, i);
    l.bump = vec3(t.x, 0.0, -t.y);
    let arm = textureSample(arm_tex, tex_sampler, uv, i);
    l.ao = arm.r;
    l.rough = arm.g;
    return l;
}

// Dreiplanare Projektion für Felsflanken
fn triplanar(p: vec3<f32>, n: vec3<f32>, i: i32) -> Layer {
    var b = pow(abs(n), vec3(4.0));
    b = b / (b.x + b.y + b.z);
    let uvx = vec2(p.z, -p.y) * params.rock_tile;
    let uvy = p.xz * params.rock_tile;
    let uvz = vec2(p.x, -p.y) * params.rock_tile;

    var l: Layer;
    l.albedo = textureSample(albedo_tex, tex_sampler, uvx, i).rgb * b.x
        + textureSample(albedo_tex, tex_sampler, uvy, i).rgb * b.y
        + textureSample(albedo_tex, tex_sampler, uvz, i).rgb * b.z;
    let tx = tangent_normal(uvx, i);
    let ty = tangent_normal(uvy, i);
    let tz = tangent_normal(uvz, i);
    l.bump = vec3(0.0, tx.y, tx.x) * b.x + vec3(ty.x, 0.0, -ty.y) * b.y + vec3(tz.x, tz.y, 0.0) * b.z;
    let ax = textureSample(arm_tex, tex_sampler, uvx, i);
    let ay = textureSample(arm_tex, tex_sampler, uvy, i);
    let az = textureSample(arm_tex, tex_sampler, uvz, i);
    let arm = ax * b.x + ay * b.y + az * b.z;
    l.ao = arm.r;
    l.rough = arm.g;
    return l;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let p = in.world_position.xyz;
    let gn = normalize(in.world_normal);
    let uv = p.xz * params.tile;

    var dirt = planar(uv, 0);
    let gravel = planar(uv * 1.3 + vec2(0.37, 0.11), 1);
    let rock = triplanar(p, gn, 2);
    var grass = planar(uv * 1.1 + vec2(0.71, 0.53), 3);
    // Moos/Gras trockener und weniger gesättigt, passend zur kargen Hochebene
    let gl = luma(grass.albedo);
    grass.albedo = mix(vec3(gl), grass.albedo, 0.6) * vec3(0.92, 0.88, 0.70);

    // Kachelung brechen: Erde zusätzlich gedreht und größer abtasten
    let rot = mat2x2<f32>(0.8, -0.6, 0.6, 0.8);
    let dirt2 = textureSample(albedo_tex, tex_sampler, rot * uv * 0.43, 0).rgb;
    let macro_n = textureSample(albedo_tex, tex_sampler, p.xz * params.macro_scale, 3).rgb;
    let mix_k = smoothstep(0.35, 0.65, luma(macro_n) * 2.2);
    dirt.albedo = mix(dirt.albedo, dirt2, mix_k * 0.55);

    // Höhenbasierte Überblendung: die Umgebungsverdeckung dient als Höhenersatz,
    // so wachsen Steine und Grasbüschel scharf aus dem Boden statt weich zu verlaufen
    let w = max(in.color, vec4(0.0));
    let h = vec4(dirt.ao, gravel.ao, rock.ao, grass.ao);
    let hw = w * (h + 0.35);
    let top = max(max(hw.x, hw.y), max(hw.z, hw.w));
    var bw = max(hw - (top - 0.16), vec4(0.0));
    bw = bw / max(bw.x + bw.y + bw.z + bw.w, 1e-4);

    var albedo = dirt.albedo * bw.x + gravel.albedo * bw.y + rock.albedo * bw.z + grass.albedo * bw.w;
    let bump = dirt.bump * bw.x + gravel.bump * bw.y + rock.bump * bw.z + grass.bump * bw.w;
    let ao = dirt.ao * bw.x + gravel.ao * bw.y + rock.ao * bw.z + grass.ao * bw.w;
    let rough = dirt.rough * bw.x + gravel.rough * bw.y + rock.rough * bw.z + grass.rough * bw.w;

    // großflächige Helligkeitswolken gegen Gleichförmigkeit
    albedo = albedo * mix(0.72, 1.12, smoothstep(0.12, 0.55, luma(macro_n)));
    // leicht entsättigt: staubige, kühle Hochebene statt Wüstenorange
    albedo = mix(vec3(luma(albedo)), albedo, 0.78);
    // Mikroschatten der Textur auch im direkten Licht andeuten
    albedo = albedo * mix(0.75, 1.0, ao);

    pbr_input.material.base_color = vec4(albedo, 1.0);
    pbr_input.material.perceptual_roughness = clamp(rough, 0.35, 1.0);
    pbr_input.diffuse_occlusion = pbr_input.diffuse_occlusion * mix(1.0, ao, 0.8);
    pbr_input.N = normalize(gn + bump * params.normal_strength);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
