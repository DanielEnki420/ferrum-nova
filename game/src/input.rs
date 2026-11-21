use crate::components::*;
use crate::hud::{BOTTOM_PANEL_H, TOP_BAR_H};
use crate::terrain;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

#[derive(Resource)]
pub struct CamRig {
    pub focus: Vec3,
    pub dist: f32,
    pub yaw: f32,
}

impl Default for CamRig {
    fn default() -> Self {
        Self {
            focus: PLAYER_HQ_POS + Vec3::new(10.0, 0.0, -10.0),
            dist: 55.0,
            yaw: std::f32::consts::FRAC_PI_4,
        }
    }
}

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource, Default)]
pub struct DragState {
    pub start: Option<Vec2>,
}

/// Rechteck für die Auswahlbox (UI-Element)
#[derive(Component)]
pub struct SelectionBox;

/// Dunst-Farbe am Horizont (auch Hintergrundfarbe)
pub const HAZE: Color = Color::srgb(0.66, 0.64, 0.60);

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        Transform::default(),
        Tonemapping::TonyMcMapface,
        Bloom { intensity: 0.12, ..Bloom::NATURAL },
        DistanceFog {
            color: HAZE,
            directional_light_color: Color::srgba(1.0, 0.85, 0.65, 0.35),
            directional_light_exponent: 24.0,
            falloff: FogFalloff::Linear { start: 90.0, end: 280.0 },
        },
    ));
}

/// Schattenbereich an den Zoom anpassen: nah = scharfe Schatten, weit = alles im Schatten
pub fn update_shadows(rig: Res<CamRig>, mut q: Query<&mut CascadeShadowConfig>) {
    if !rig.is_changed() {
        return;
    }
    let max = rig.dist * 2.4 + 20.0;
    for mut c in &mut q {
        *c = CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 1.0,
            maximum_distance: max,
            first_cascade_far_bound: max,
            ..default()
        }
        .build();
    }
}

fn cam_offset(rig: &CamRig) -> Vec3 {
    let pitch = (42.0 + rig.dist * 0.18).min(62.0).to_radians();
    let horiz = rig.dist * pitch.cos();
    Vec3::new(horiz * rig.yaw.sin(), rig.dist * pitch.sin(), horiz * rig.yaw.cos())
}

pub fn camera_control(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut rig: ResMut<CamRig>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) {
    let dt = time.delta_secs();
    let mut dir = Vec2::ZERO;
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        dir.y -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        dir.y += 1.0;
    }
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        dir.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        dir.x += 1.0;
    }
    if let Ok(win) = window.single()
        && let Some(c) = win.cursor_position() {
            let m = 6.0;
            if c.x < m {
                dir.x -= 1.0;
            }
            if c.x > win.width() - m {
                dir.x += 1.0;
            }
            if c.y < m {
                dir.y -= 1.0;
            }
            if c.y > win.height() - m {
                dir.y += 1.0;
            }
        }
    if keys.pressed(KeyCode::KeyQ) {
        rig.yaw += 1.5 * dt;
    }
    if keys.pressed(KeyCode::KeyE) {
        rig.yaw -= 1.5 * dt;
    }
    if keys.just_pressed(KeyCode::Space) {
        rig.focus = PLAYER_HQ_POS + Vec3::new(10.0, 0.0, -10.0);
    }
    for ev in wheel.read() {
        let amount = match ev.unit {
            MouseScrollUnit::Line => ev.y * 4.0,
            MouseScrollUnit::Pixel => ev.y * 0.08,
        };
        rig.dist = (rig.dist - amount).clamp(20.0, 110.0);
    }
    if dir != Vec2::ZERO {
        let d = dir.normalize();
        // Bildschirm-Richtungen relativ zur Kameradrehung
        let fwd = Vec3::new(rig.yaw.sin(), 0.0, rig.yaw.cos());
        let right = Vec3::new(fwd.z, 0.0, -fwd.x);
        let speed = 0.9 * rig.dist;
        let delta = (right * d.x + fwd * d.y) * speed * dt;
        let lim = MAP_HALF - 15.0;
        rig.focus = (rig.focus + delta).clamp(Vec3::splat(-lim), Vec3::splat(lim));
    }
    // Blickpunkt sanft der Geländehöhe nachführen
    let ground = terrain::height(rig.focus.x, rig.focus.z);
    rig.focus.y += (ground - rig.focus.y) * (6.0 * dt).min(1.0);
    if let Ok(mut t) = cam.single_mut() {
        *t = Transform::from_translation(rig.focus + cam_offset(&rig)).looking_at(rig.focus, Vec3::Y);
    }
}

/// Bildschirmpunkt -> Punkt auf dem Gelände (Strahl schrittweise auf die Höhe angenähert)
pub fn ground_point(cam: &Camera, cam_t: &GlobalTransform, screen: Vec2) -> Option<Vec3> {
    let ray = cam.viewport_to_world(cam_t, screen).ok()?;
    let d = *ray.direction;
    if d.y > -1e-3 {
        return None;
    }
    let mut t = -ray.origin.y / d.y;
    for _ in 0..8 {
        let p = ray.origin + d * t;
        t = (terrain::height(p.x, p.z) - ray.origin.y) / d.y;
    }
    (t > 0.0).then(|| ray.origin + d * t)
}

/// Wählt das Objekt, dessen Bildschirmposition dem Cursor am nächsten liegt.
/// Arbeitet im Bildschirmraum, damit Klicks auf die Oberseite eines 3D-Modells treffen.
fn pick(
    cam: &Camera,
    cam_t: &GlobalTransform,
    cursor: Vec2,
    cands: impl Iterator<Item = (Entity, Vec3, f32)>,
) -> Option<Entity> {
    let right = cam_t.right().as_vec3();
    let mut best: Option<(Entity, f32)> = None;
    for (e, pos, radius) in cands {
        let base = pos + Vec3::Y * 0.7;
        let (Ok(center), Ok(edge)) = (
            cam.world_to_viewport(cam_t, base),
            cam.world_to_viewport(cam_t, base + right * radius),
        ) else {
            continue;
        };
        let px_radius = center.distance(edge) * 1.25 + 6.0;
        let d = center.distance(cursor);
        if d < px_radius && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((e, d));
        }
    }
    best.map(|(e, _)| e)
}

fn over_ui(win: &Window, c: Vec2, scale: f32) -> bool {
    c.y < TOP_BAR_H * scale || c.y > win.height() - BOTTOM_PANEL_H * scale
}

#[allow(clippy::too_many_arguments)]
pub fn mouse_commands(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut drag: ResMut<DragState>,
    outcome: Res<Outcome>,
    units: Query<(Entity, &GlobalTransform, &Team, &Unit, Has<Selected>, Has<Weapon>, Has<Harvester>)>,
    targets: Query<(Entity, &GlobalTransform, &Team, Option<&Unit>, Option<&Building>), With<Health>>,
    nodes: Query<(Entity, &GlobalTransform), With<ResourceNode>>,
    mut harvesters: Query<&mut Harvester>,
    mut sel_box: Query<(&mut Node, &mut Visibility), With<SelectionBox>>,
    ui_scale: Res<UiScale>,
) {
    let Ok(win) = window.single() else { return };
    let Ok((camera, cam_t)) = cam.single() else { return };
    let cursor = win.cursor_position();
    if *outcome != Outcome::Running {
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft]);

    // --- Linke Maustaste: Auswahl ---
    if mouse.just_pressed(MouseButton::Left)
        && let Some(c) = cursor
            && !over_ui(win, c, ui_scale.0) {
                drag.start = Some(c);
            }
    if let (Some(start), Some(c)) = (drag.start, cursor)
        && let Ok((mut node, mut vis)) = sel_box.single_mut() {
            let min = start.min(c);
            let size = (start - c).abs();
            if size.length() > 6.0 {
                let k = ui_scale.0.max(0.01);
                node.left = px(min.x / k);
                node.top = px(min.y / k);
                node.width = px(size.x / k);
                node.height = px(size.y / k);
                *vis = Visibility::Visible;
            }
        }
    if mouse.just_released(MouseButton::Left) {
        if let Ok((_, mut vis)) = sel_box.single_mut() {
            *vis = Visibility::Hidden;
        }
        if let (Some(start), Some(c)) = (drag.start.take(), cursor) {
            if !shift {
                for (e, _, _, _, sel, _, _) in &units {
                    if sel {
                        commands.entity(e).remove::<Selected>();
                    }
                }
            }
            if (start - c).length() > 6.0 {
                let min = start.min(c);
                let max = start.max(c);
                for (e, gt, team, _, _, _, _) in &units {
                    if *team != PLAYER {
                        continue;
                    }
                    if let Ok(p) = camera.world_to_viewport(cam_t, gt.translation())
                        && p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y {
                            commands.entity(e).insert(Selected);
                        }
                }
            } else {
                let cands = units
                    .iter()
                    .filter(|(_, _, t, _, _, _, _)| **t == PLAYER)
                    .map(|(e, gt, _, u, _, _, _)| (e, gt.translation(), u.radius));
                if let Some(e) = pick(camera, cam_t, c, cands) {
                    commands.entity(e).insert(Selected);
                }
            }
        }
    }

    // --- Rechte Maustaste: Befehle ---
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    let Some(c) = cursor else { return };
    if over_ui(win, c, ui_scale.0) {
        return;
    }
    let Some(gp) = ground_point(camera, cam_t, c) else { return };

    let selected: Vec<(Entity, bool, bool)> = units
        .iter()
        .filter(|(_, _, t, _, s, _, _)| *s && **t == PLAYER)
        .map(|(e, _, _, _, _, w, h)| (e, w, h))
        .collect();
    if selected.is_empty() {
        return;
    }

    // Gegner unter dem Cursor?
    let enemy = pick(
        camera,
        cam_t,
        c,
        targets.iter().filter(|(_, _, t, _, _)| **t != PLAYER).map(|(e, gt, _, u, b)| {
            let r = u.map(|u| u.radius).or(b.map(|b| b.radius)).unwrap_or(1.0);
            (e, gt.translation(), r)
        }),
    )
    .map(|e| (e, 0.0));
    // Ressourcenfeld unter dem Cursor?
    let mut node: Option<Entity> = None;
    for (e, gt) in &nodes {
        if flat(gt.translation()).distance(flat(gp)) < 3.5 {
            node = Some(e);
        }
    }

    let n = selected.len();
    let cols = (n as f32).sqrt().ceil() as usize;
    for (i, (e, armed, harv)) in selected.into_iter().enumerate() {
        let mut ec = commands.entity(e);
        if let (Some((target, _)), true) = (enemy, armed) {
            ec.remove::<MoveTo>().remove::<AttackMove>().insert(AttackTarget(target));
            continue;
        }
        if let (Some(node_e), true) = (node, harv) {
            if let Ok(mut h) = harvesters.get_mut(e) {
                h.node = Some(node_e);
                h.state = HarvestState::ToNode;
            }
            ec.remove::<MoveTo>();
            continue;
        }
        // Formation: Gitter um den Zielpunkt
        let row = i / cols;
        let col = i % cols;
        let off = Vec3::new(
            (col as f32 - (cols as f32 - 1.0) / 2.0) * 3.0,
            0.0,
            (row as f32 - (n.div_ceil(cols) as f32 - 1.0) / 2.0) * 3.0,
        );
        ec.remove::<AttackTarget>().insert(MoveTo(gp + off));
        if ctrl && armed {
            ec.insert(AttackMove);
        } else {
            ec.remove::<AttackMove>();
        }
        if harv
            && let Ok(mut h) = harvesters.get_mut(e) {
                h.state = HarvestState::Idle;
                h.node = None;
            }
    }
}

/// Auswahlkreise und Lebensbalken
pub fn draw_overlays(
    mut gizmos: Gizmos,
    units: Query<(&GlobalTransform, &Health, &Team, Option<&Unit>, Option<&Building>, Has<Selected>)>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
) {
    let Ok(cam_t) = cam.single() else { return };
    let right = cam_t.right().with_y(0.0).normalize_or_zero();
    for (gt, hp, team, unit, bld, sel) in &units {
        let pos = gt.translation();
        let r = unit.map(|u| u.radius).or(bld.map(|b| b.radius)).unwrap_or(1.0);
        if sel {
            gizmos.circle(
                Isometry3d::new(pos + Vec3::Y * 0.15, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                r + 0.3,
                Color::srgb(0.4, 1.0, 0.5),
            );
        }
        let frac = (hp.hp / hp.max).clamp(0.0, 1.0);
        if sel || frac < 0.999 {
            let height = if bld.is_some() { 8.2 } else { 2.6 };
            let w = r * 1.6;
            let start = pos + Vec3::Y * height - right * w * 0.5;
            let col = if *team == PLAYER {
                Color::srgb(0.3, 1.0, 0.4)
            } else {
                Color::srgb(1.0, 0.3, 0.3)
            };
            gizmos.line(start, start + right * w, Color::srgb(0.15, 0.15, 0.15));
            gizmos.line(start + Vec3::Y * 0.05, start + Vec3::Y * 0.05 + right * w * frac, col);
        }
    }
}

/// F: alle eigenen Kampfeinheiten auswählen
pub fn select_army(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    units: Query<(Entity, &Team, Has<Weapon>, Has<Selected>)>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    for (e, team, armed, sel) in &units {
        if *team != PLAYER {
            continue;
        }
        if armed {
            commands.entity(e).insert(Selected);
        } else if sel {
            commands.entity(e).remove::<Selected>();
        }
    }
}
