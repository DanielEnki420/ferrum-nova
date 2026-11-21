// Ferrum Nova – Wind für Gras, Büsche und Bäume
// uv.y = Biegsamkeit (0 an der Wurzel, 1 an der Spitze). Die Böen laufen als Wellen über die Karte.

import bevy_pbr::render::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, decompress_vertex},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::globals,
};

struct WindParams {
    strength: f32,
    speed: f32,
    flutter: f32,
    _pad: f32,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> wind: WindParams;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vert = decompress_vertex(v, v.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    var wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vert.position, 1.0));

    let bend = vert.uv.y * vert.uv.y;
    let t = globals.time * wind.speed;
    // große Böen, die schräg über die Ebene wandern, plus feines Zittern
    let wave = dot(wp.xz, vec2(0.11, 0.07));
    let gust = sin(t * 1.1 - wave) * 0.6 + sin(t * 2.3 - wave * 1.7) * 0.3 + 0.35;
    let flutter = sin(t * 7.0 + wp.x * 3.1 + wp.z * 2.3) * wind.flutter;
    let dir = normalize(vec2(1.0, 0.55));
    let push = (gust + flutter) * wind.strength * bend;
    wp.x = wp.x + dir.x * push;
    wp.z = wp.z + dir.y * push;
    wp.y = wp.y - abs(push) * 0.25 * vert.uv.y;

    out.world_position = wp;
    out.position = position_world_to_clip(wp.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vert.normal, v.instance_index);
    out.uv = vert.uv;
    out.color = vert.color;
    return out;
}
