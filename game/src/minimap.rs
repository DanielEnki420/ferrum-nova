//! Minikarte unten rechts: Gelände, Felder, Gebäude, Einheiten, Sichtbereich der Kamera, Angriffsmeldungen.
//! Linksklick (auch ziehen) bewegt die Kamera, Rechtsklick schickt die Auswahl dorthin.

use crate::components::*;
use crate::hud::BOTTOM_PANEL_H;
use crate::input::{ground_point, CamRig, MainCamera};
use crate::orders::{issue_group_move, Alerts, Member, MoveKind};
use crate::terrain;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;
use bevy::window::PrimaryWindow;

/// Kantenlänge der Minikarte im HUD (logische Pixel) und Auflösung des Bildes
pub const MINIMAP_SIZE: f32 = 176.0;
pub const MINIMAP_MARGIN: f32 = 8.0;
const RES: usize = 128;

#[derive(Component)]
pub struct Minimap;

#[derive(Resource)]
pub struct MinimapImage {
    handle: Handle<Image>,
    base: Vec<u8>,
    timer: f32,
}

/// Bildschirmrechteck der Minikarte (physische Pixel, wie `cursor_position`)
pub fn minimap_rect(win: &Window, scale: f32) -> Rect {
    let size = MINIMAP_SIZE * scale;
    let m = MINIMAP_MARGIN * scale;
    let bottom = win.height() - (BOTTOM_PANEL_H + MINIMAP_MARGIN) * scale;
    Rect::new(win.width() - m - size, bottom - size, win.width() - m, bottom)
}

/// Weltposition -> Kartenpixel (x nach rechts, z nach unten)
pub fn world_to_map(p: Vec3) -> Vec2 {
    let k = RES as f32 / (2.0 * MAP_HALF);
    Vec2::new((p.x + MAP_HALF) * k, (p.z + MAP_HALF) * k)
}

/// Relative Position auf der Karte (0..1) -> Weltposition am Boden
pub fn map_to_world(uv: Vec2) -> Vec3 {
    let x = (uv.x.clamp(0.0, 1.0) * 2.0 - 1.0) * MAP_HALF;
    let z = (uv.y.clamp(0.0, 1.0) * 2.0 - 1.0) * MAP_HALF;
    terrain::on_ground(Vec3::new(x, 0.0, z))
}

/// Grundbild: Geländehöhe mit Schräglicht
fn terrain_base() -> Vec<u8> {
    let mut data = vec![0u8; RES * RES * 4];
    let cell = 2.0 * MAP_HALF / RES as f32;
    for y in 0..RES {
        for x in 0..RES {
            let wx = -MAP_HALF + (x as f32 + 0.5) * cell;
            let wz = -MAP_HALF + (y as f32 + 0.5) * cell;
            let h = terrain::height(wx, wz);
            let shade = (terrain::height(wx - cell, wz - cell) - h) * 0.35;
            let k = (0.55 + h * 0.03 + shade).clamp(0.25, 1.0);
            let col = [0.30 * k + 0.04, 0.34 * k + 0.04, 0.24 * k + 0.03];
            let i = (y * RES + x) * 4;
            for c in 0..3 {
                data[i + c] = (col[c] * 255.0) as u8;
            }
            data[i + 3] = 255;
        }
    }
    data
}

pub fn setup_minimap(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let base = terrain_base();
    let mut image = Image::new_fill(
        Extent3d { width: RES as u32, height: RES as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.data = Some(base.clone());
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(MINIMAP_MARGIN),
            bottom: px(BOTTOM_PANEL_H + MINIMAP_MARGIN),
            width: px(MINIMAP_SIZE),
            height: px(MINIMAP_SIZE),
            border: UiRect::all(px(2)),
            ..default()
        },
        BorderColor::all(Color::srgb(0.35, 0.5, 0.65)),
        BackgroundColor(Color::BLACK),
        ImageNode::new(handle.clone()),
        RelativeCursorPosition::default(),
        Minimap,
    ));
    commands.insert_resource(MinimapImage { handle, base, timer: 0.0 });
}

fn put(data: &mut [u8], x: i32, y: i32, col: [u8; 3]) {
    if x < 0 || y < 0 || x >= RES as i32 || y >= RES as i32 {
        return;
    }
    let i = (y as usize * RES + x as usize) * 4;
    data[i..i + 3].copy_from_slice(&col);
}

fn square(data: &mut [u8], c: Vec2, half: i32, col: [u8; 3]) {
    for dy in -half..=half {
        for dx in -half..=half {
            put(data, c.x as i32 + dx, c.y as i32 + dy, col);
        }
    }
}

fn line(data: &mut [u8], a: Vec2, b: Vec2, col: [u8; 3]) {
    let n = a.distance(b).ceil().max(1.0) as i32;
    for i in 0..=n {
        let p = a.lerp(b, i as f32 / n as f32);
        put(data, p.x as i32, p.y as i32, col);
    }
}

fn team_color(team: Team, selected: bool) -> [u8; 3] {
    match (team == PLAYER, selected) {
        (true, true) => [190, 255, 200],
        (true, false) => [70, 200, 255],
        (false, _) => [255, 70, 55],
    }
}

/// Bild etwa fünfmal pro Sekunde neu zeichnen
pub fn update_minimap(
    time: Res<Time>,
    mut mm: ResMut<MinimapImage>,
    mut images: ResMut<Assets<Image>>,
    alerts: Res<Alerts>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    units: Query<(&Transform, &Team, Has<Selected>), (With<Unit>, Without<Fogged>)>,
    buildings: Query<(&Transform, &Team, &Building, Has<Selected>), Without<Fogged>>,
    nodes: Query<(&Transform, &ResourceNode)>,
    fog: Option<Res<crate::fog::FogGrid>>,
) {
    mm.timer -= time.delta_secs();
    if mm.timer > 0.0 {
        return;
    }
    mm.timer = 0.2;
    let mut data = mm.base.clone();
    // Nebel des Krieges: Unerkundetes fast schwarz, Erkundetes gedämpft
    if let Some(fog) = &fog {
        for y in 0..RES {
            for x in 0..RES {
                let p = Vec3::new(((x as f32 + 0.5) / RES as f32 * 2.0 - 1.0) * MAP_HALF, 0.0, ((y as f32 + 0.5) / RES as f32 * 2.0 - 1.0) * MAP_HALF);
                let k = 0.12 + 0.88 * fog.shade(p);
                let i = (y * RES + x) * 4;
                for c in &mut data[i..i + 3] {
                    *c = (*c as f32 * k) as u8;
                }
            }
        }
    }
    for (t, n) in &nodes {
        if n.amount == 0 {
            continue;
        }
        let col = match n.kind {
            ResKind::Ferrit => [230, 170, 60],
            ResKind::Plasma => [190, 110, 255],
        };
        square(&mut data, world_to_map(t.translation), 2, col);
    }
    let k = RES as f32 / (2.0 * MAP_HALF);
    for (t, team, b, sel) in &buildings {
        let half = (b.radius * k * 0.8).round().max(1.0) as i32;
        square(&mut data, world_to_map(t.translation), half, [20, 20, 20]);
        square(&mut data, world_to_map(t.translation), half - 1, team_color(*team, sel));
    }
    for (t, team, sel) in &units {
        let c = world_to_map(t.translation);
        square(&mut data, c, 0, team_color(*team, sel));
        put(&mut data, c.x as i32 + 1, c.y as i32, team_color(*team, sel));
        put(&mut data, c.x as i32, c.y as i32 + 1, team_color(*team, sel));
        put(&mut data, c.x as i32 + 1, c.y as i32 + 1, team_color(*team, sel));
    }
    // Angriffsmeldungen: wachsender roter Ring
    for (p, age) in &alerts.pings {
        let r = 3.0 + age * 3.0;
        let c = world_to_map(*p);
        let blink = (age * 6.0) as i32 % 2 == 0;
        if blink {
            for i in 0..24 {
                let a = i as f32 / 24.0 * std::f32::consts::TAU;
                put(&mut data, (c.x + a.cos() * r) as i32, (c.y + a.sin() * r) as i32, [255, 40, 30]);
            }
        }
    }
    // Sichtbereich der Kamera
    if let (Ok(win), Ok((camera, cam_t))) = (window.single(), cam.single()) {
        let (w, h) = (win.width(), win.height());
        let corners = [Vec2::new(0.0, 0.0), Vec2::new(w, 0.0), Vec2::new(w, h), Vec2::new(0.0, h)];
        let pts: Vec<Vec2> = corners.iter().filter_map(|c| ground_point(camera, cam_t, *c)).map(world_to_map).collect();
        if pts.len() == 4 {
            for i in 0..4 {
                line(&mut data, pts[i], pts[(i + 1) % 4], [240, 240, 240]);
            }
        }
    }
    if let Some(mut img) = images.get_mut(&mm.handle) {
        img.data = Some(data);
    }
}

/// Klicks auf die Minikarte
pub fn minimap_input(
    mut commands: Commands,
    mut clicks: MessageReader<MouseButtonInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    map: Query<&RelativeCursorPosition, With<Minimap>>,
    mut rig: ResMut<CamRig>,
    mut dragging: Local<bool>,
    units: Query<(Entity, &Transform, &Team, &Unit, Has<Weapon>, Has<Harvester>), With<Selected>>,
) {
    let Ok(rel) = map.single() else { return };
    let mut right_click = false;
    for c in clicks.read() {
        if c.state != ButtonState::Pressed || !rel.cursor_over() {
            continue;
        }
        match c.button {
            MouseButton::Left => *dragging = true,
            MouseButton::Right => right_click = true,
            _ => {}
        }
    }
    if !mouse.pressed(MouseButton::Left) {
        *dragging = false;
    }
    let Some(n) = rel.normalized else { return };
    let uv = n + Vec2::splat(0.5);
    if *dragging {
        let p = map_to_world(uv);
        let lim = MAP_HALF - 15.0;
        rig.focus.x = p.x.clamp(-lim, lim);
        rig.focus.z = p.z.clamp(-lim, lim);
    }
    if right_click {
        let members: Vec<Member> = units
            .iter()
            .filter(|(_, _, t, _, _, _)| **t == PLAYER)
            .map(|(e, t, _, u, armed, harvester)| Member { e, pos: t.translation, speed: u.speed, radius: u.radius, armed, harvester })
            .collect();
        let (queue, ctrl, alt) = crate::input::modifiers(&keys);
        let kind = if alt {
            MoveKind::Patrol
        } else if ctrl {
            MoveKind::AttackMove
        } else {
            MoveKind::Move
        };
        issue_group_move(&mut commands, &members, map_to_world(uv), None, kind, queue);
    }
}
