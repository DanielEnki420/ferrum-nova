//! Kamera, Auswahl, Kontrollgruppen und Befehle per Maus und Tastatur.

use crate::components::*;
use crate::hud::{SelCard, BOTTOM_PANEL_H, TOP_BAR_H};
use crate::minimap::minimap_rect;
use crate::orders::*;
use crate::terrain;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::ButtonState;
use bevy::input::mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};
use std::f32::consts::FRAC_PI_2;

#[derive(Resource)]
pub struct CamRig {
    pub focus: Vec3,
    pub dist: f32,
    pub yaw: f32,
}

pub fn base_focus() -> Vec3 {
    PLAYER_HQ_POS + Vec3::new(10.0, 0.0, -10.0)
}

impl Default for CamRig {
    fn default() -> Self {
        Self { focus: base_focus(), dist: 55.0, yaw: std::f32::consts::FRAC_PI_4 }
    }
}

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource, Default)]
pub struct DragState {
    /// Linke Taste: Beginn der Auswahlbox
    pub start: Option<Vec2>,
    /// Rechte Taste: (Bildschirm, Boden) beim Drücken
    pub right: Option<(Vec2, Vec3)>,
    /// Rechte Taste gehalten: aktueller Bodenpunkt (Vorschau der Frontlinie)
    pub right_now: Option<Vec3>,
    /// Mittlere Taste: letzte Cursorposition beim Verschieben
    pub mid: Option<Vec2>,
}

/// Ab dieser Länge (Meter) wird aus einem Rechtsklick eine gezogene Frontlinie
pub const LINE_MIN: f32 = 3.0;

/// Rechteck für die Auswahlbox (UI-Element)
#[derive(Component)]
pub struct SelectionBox;

/// Kontrollgruppen 1–9
#[derive(Resource, Default)]
pub struct ControlGroups {
    pub groups: [Vec<Entity>; 10],
    last: Option<(usize, f64)>,
}

/// Objekt unter dem Mauszeiger
#[derive(Resource, Default)]
pub struct Hover(pub Option<(Entity, HoverKind)>);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HoverKind {
    Own,
    Enemy,
    Node,
}

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

fn pitch(dist: f32) -> f32 {
    (42.0 + dist * 0.18).min(62.0).to_radians()
}

fn cam_offset(rig: &CamRig) -> Vec3 {
    let p = pitch(rig.dist);
    let horiz = rig.dist * p.cos();
    Vec3::new(horiz * rig.yaw.sin(), rig.dist * p.sin(), horiz * rig.yaw.cos())
}

const CAM_LIM: f32 = MAP_HALF - 15.0;

pub fn camera_control(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut rig: ResMut<CamRig>,
    mut drag: ResMut<DragState>,
    alerts: Res<Alerts>,
    ui_scale: Res<UiScale>,
    mut cam: Query<(&Camera, &GlobalTransform, &mut Transform), With<MainCamera>>,
) {
    let dt = time.delta_secs();
    let Ok((camera, cam_gt, mut cam_t)) = cam.single_mut() else { return };
    let win = window.single().ok();
    let cursor = win.and_then(|w| w.cursor_position());
    let fwd = Vec3::new(rig.yaw.sin(), 0.0, rig.yaw.cos());
    let right = Vec3::new(fwd.z, 0.0, -fwd.x);

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
    if let (Some(win), Some(c)) = (win, cursor)
        && drag.mid.is_none()
    {
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
    // Leertaste: zum letzten Angriff, sonst zur Basis. Pos1: immer zur Basis.
    if keys.just_pressed(KeyCode::Space) {
        rig.focus = alerts.recent().unwrap_or_else(base_focus);
    }
    if keys.just_pressed(KeyCode::Home) {
        rig.focus = base_focus();
    }
    // Zoom zum Mauszeiger hin: der Bodenpunkt unter dem Cursor bleibt (fast) stehen
    let over_ui = match (win, cursor) {
        (Some(w), Some(c)) => over_ui(w, c, ui_scale.0),
        _ => true,
    };
    for ev in wheel.read() {
        let amount = match ev.unit {
            MouseScrollUnit::Line => ev.y * 4.0,
            MouseScrollUnit::Pixel => ev.y * 0.08,
        };
        let old = rig.dist;
        rig.dist = (rig.dist - amount).clamp(20.0, 110.0);
        if !over_ui
            && let Some(gp) = cursor.and_then(|c| ground_point(camera, cam_gt, c))
        {
            let k = rig.dist / old;
            let to = flat(rig.focus - gp) * k;
            rig.focus = (flat(gp) + to).with_y(rig.focus.y);
        }
    }
    // Mittlere Maustaste: Karte greifen und verschieben
    if mouse.just_pressed(MouseButton::Middle) && !over_ui {
        drag.mid = cursor;
    }
    if !mouse.pressed(MouseButton::Middle) {
        drag.mid = None;
    }
    if let (Some(last), Some(c), Some(win)) = (drag.mid, cursor, win) {
        let d = c - last;
        // Weltmeter pro Bildschirmpixel am Blickpunkt (45° Sichtfeld)
        let k = 0.83 * rig.dist / win.height().max(1.0);
        let k_depth = k / pitch(rig.dist).sin();
        rig.focus -= right * d.x * k + fwd * d.y * k_depth;
        drag.mid = Some(c);
    }
    if dir != Vec2::ZERO {
        let d = dir.normalize();
        // Bildschirm-Richtungen relativ zur Kameradrehung
        let speed = 0.9 * rig.dist;
        rig.focus += (right * d.x + fwd * d.y) * speed * dt;
    }
    rig.focus.x = rig.focus.x.clamp(-CAM_LIM, CAM_LIM);
    rig.focus.z = rig.focus.z.clamp(-CAM_LIM, CAM_LIM);
    // Blickpunkt sanft der Geländehöhe nachführen
    let ground = terrain::height(rig.focus.x, rig.focus.z);
    rig.focus.y += (ground - rig.focus.y) * (6.0 * dt).min(1.0);
    *cam_t = Transform::from_translation(rig.focus + cam_offset(&rig)).looking_at(rig.focus, Vec3::Y);
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
pub fn pick(
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

/// Liegt der Cursor über HUD-Leisten oder der Minikarte?
pub fn over_ui(win: &Window, c: Vec2, scale: f32) -> bool {
    c.y < TOP_BAR_H * scale || c.y > win.height() - BOTTOM_PANEL_H * scale || minimap_rect(win, scale).contains(c)
}

/// Shift, Strg (bzw. Cmd), Alt – auch wenn sie im selben Frame schon wieder losgelassen wurden
/// (sehr schnelle Tastenkombinationen fallen sonst durch)
pub fn modifiers(keys: &ButtonInput<KeyCode>) -> (bool, bool, bool) {
    let held = |k: [KeyCode; 2]| keys.any_pressed(k) || keys.any_just_released(k);
    (
        held([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        held([KeyCode::ControlLeft, KeyCode::ControlRight]) || held([KeyCode::SuperLeft, KeyCode::SuperRight]),
        held([KeyCode::AltLeft, KeyCode::AltRight]),
    )
}

const DOUBLE_CLICK: f64 = 0.4;

pub fn mouse_commands(
    mut commands: Commands,
    (time, mut clicks): (Res<Time>, MessageReader<MouseButtonInput>),
    (mouse, keys): (Res<ButtonInput<MouseButton>>, Res<ButtonInput<KeyCode>>),
    (window, cam): (Query<&Window, With<PrimaryWindow>>, Query<(&Camera, &GlobalTransform), With<MainCamera>>),
    mut drag: ResMut<DragState>,
    outcome: Res<Outcome>,
    units: Query<(Entity, &GlobalTransform, &Team, &Unit, Has<Selected>, Has<Weapon>, Has<Harvester>)>,
    buildings: Query<(Entity, &GlobalTransform, &Team, &Building, Has<Hq>, Has<Selected>), (Without<Unit>, Without<Construction>)>,
    targets: Query<(Entity, &GlobalTransform, &Team, Option<&Unit>, Option<&Building>), With<Health>>,
    nodes: Query<(Entity, &GlobalTransform), With<ResourceNode>>,
    mut harvesters: Query<&mut Harvester>,
    (mut sel_box, panel): (Query<(&mut Node, &mut Visibility), With<SelectionBox>>, Query<&RelativeCursorPosition, With<SelCard>>),
    ui_scale: Res<UiScale>,
    mut placing: ResMut<crate::construction::Placing>,
    mut last_click: Local<(f64, Option<Entity>)>,
) {
    // Im Baumodus gehören Klicks der Platzierung
    if placing.kind.is_some() || placing.block_click {
        placing.block_click = false;
        drag.start = None;
        drag.right = None;
        drag.right_now = None;
        return;
    }
    let Ok(win) = window.single() else { return };
    let Ok((camera, cam_t)) = cam.single() else { return };
    let cursor = win.cursor_position();
    if *outcome != Outcome::Running {
        return;
    }
    let (shift, ctrl, alt) = modifiers(&keys);
    // ein schneller Doppelklick kann ganz in einen Frame fallen
    let left_presses = clicks.read().filter(|c| c.button == MouseButton::Left && c.state == ButtonState::Pressed).count();
    let on_panel = panel.iter().any(|p| p.cursor_over());
    let over_ui = |win: &Window, c: Vec2, scale: f32| on_panel || over_ui(win, c, scale);
    let deselect_all = |commands: &mut Commands| {
        for (e, _, _, _, sel, _, _) in &units {
            if sel {
                commands.entity(e).remove::<Selected>();
            }
        }
        for (e, _, _, _, _, sel) in &buildings {
            if sel {
                commands.entity(e).remove::<Selected>();
            }
        }
    };
    let deselect_buildings = |commands: &mut Commands| {
        for (e, _, _, _, _, sel) in &buildings {
            if sel {
                commands.entity(e).remove::<Selected>();
            }
        }
    };

    // --- Linke Maustaste: Auswahl ---
    if mouse.just_pressed(MouseButton::Left)
        && let Some(c) = cursor
        && !over_ui(win, c, ui_scale.0)
    {
        drag.start = Some(c);
    }
    if let (Some(start), Some(c)) = (drag.start, cursor)
        && let Ok((mut node, mut vis)) = sel_box.single_mut()
    {
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
            if (start - c).length() > 6.0 {
                // Auswahlbox: Shift fügt hinzu, Strg nimmt heraus
                if !shift && !ctrl {
                    deselect_all(&mut commands);
                }
                deselect_buildings(&mut commands);
                let min = start.min(c);
                let max = start.max(c);
                for (e, gt, team, _, _, _, _) in &units {
                    if *team != PLAYER {
                        continue;
                    }
                    if let Ok(p) = camera.world_to_viewport(cam_t, gt.translation())
                        && p.x >= min.x
                        && p.x <= max.x
                        && p.y >= min.y
                        && p.y <= max.y
                    {
                        if ctrl {
                            commands.entity(e).remove::<Selected>();
                        } else {
                            commands.entity(e).insert(Selected);
                        }
                    }
                }
            } else {
                let unit_hit = pick(
                    camera,
                    cam_t,
                    c,
                    units.iter().filter(|u| *u.2 == PLAYER).map(|(e, gt, _, u, _, _, _)| (e, gt.translation(), u.radius)),
                );
                let building_hit = || {
                    pick(
                        camera,
                        cam_t,
                        c,
                        buildings.iter().filter(|b| *b.2 == PLAYER).map(|(e, gt, _, b, _, _)| (e, gt.translation(), b.radius * 0.8)),
                    )
                };
                let now = time.elapsed_secs_f64();
                if let Some(e) = unit_hit {
                    let double = left_presses >= 2 || (last_click.1 == Some(e) && now - last_click.0 < DOUBLE_CLICK);
                    *last_click = (now, Some(e));
                    let selected = units.get(e).is_ok_and(|u| u.4);
                    deselect_buildings(&mut commands);
                    if double {
                        // Doppelklick: alle eigenen Einheiten dieses Typs auf dem Bildschirm
                        let kind = units.get(e).map(|u| u.3.kind).ok();
                        if !shift {
                            deselect_all(&mut commands);
                        }
                        for (oe, gt, team, u, _, _, _) in &units {
                            if *team == PLAYER
                                && Some(u.kind) == kind
                                && let Ok(p) = camera.world_to_viewport(cam_t, gt.translation())
                                && !over_ui(win, p, ui_scale.0)
                                && p.x >= 0.0
                                && p.x <= win.width()
                            {
                                commands.entity(oe).insert(Selected);
                            }
                        }
                    } else if ctrl {
                        // Strg+Klick: Einheit aus der Auswahl nehmen bzw. hinzufügen
                        if selected {
                            commands.entity(e).remove::<Selected>();
                        } else {
                            commands.entity(e).insert(Selected);
                        }
                    } else {
                        if !shift {
                            deselect_all(&mut commands);
                        }
                        commands.entity(e).insert(Selected);
                    }
                } else if let Some(b) = building_hit() {
                    *last_click = (now, None);
                    deselect_all(&mut commands);
                    commands.entity(b).insert(Selected);
                } else if !shift && !ctrl {
                    *last_click = (now, None);
                    deselect_all(&mut commands);
                }
            }
        }
    }

    // --- Rechte Maustaste: Befehle (beim Loslassen; Ziehen legt eine Frontlinie) ---
    if mouse.just_pressed(MouseButton::Right)
        && let Some(c) = cursor
        && !over_ui(win, c, ui_scale.0)
        && let Some(gp) = ground_point(camera, cam_t, c)
    {
        drag.right = Some((c, gp));
    }
    if mouse.pressed(MouseButton::Right) && drag.right.is_some() {
        drag.right_now = cursor.and_then(|c| ground_point(camera, cam_t, c));
    }
    if !mouse.just_released(MouseButton::Right) {
        if !mouse.pressed(MouseButton::Right) {
            drag.right = None;
            drag.right_now = None;
        }
        return;
    }
    let Some((c, gp)) = drag.right.take() else { return };
    let end = drag.right_now.take().unwrap_or(gp);

    let members: Vec<Member> = units
        .iter()
        .filter(|u| u.4 && *u.2 == PLAYER)
        .map(|(e, gt, _, u, _, armed, harvester)| Member { e, pos: gt.translation(), speed: u.speed, radius: u.radius, armed, harvester })
        .collect();

    // Ressourcenfeld unter dem Cursor?
    let node = nodes.iter().find(|(_, gt)| flat(gt.translation()).distance(flat(gp)) < 3.5).map(|(e, _)| e);

    if members.is_empty() {
        // HQ ausgewählt: Sammelpunkt für neue Einheiten setzen
        for (e, _, team, _, hq, sel) in &buildings {
            if sel && hq && *team == PLAYER {
                let pos = node.and_then(|n| nodes.get(n).ok()).map_or(gp, |(_, gt)| gt.translation());
                commands.entity(e).insert(Rally { pos, node });
                spawn_marker(&mut commands, MarkerKind::Rally, pos, None);
            }
        }
        return;
    }

    let kind = if alt {
        MoveKind::Patrol
    } else if ctrl {
        MoveKind::AttackMove
    } else {
        MoveKind::Move
    };
    let reset_harvesters = |harvesters: &mut Query<&mut Harvester>, list: &[Member]| {
        for m in list.iter().filter(|m| m.harvester) {
            if let Ok(mut h) = harvesters.get_mut(m.e) {
                h.state = HarvestState::Idle;
                h.node = None;
            }
        }
    };

    // Gezogene Frontlinie
    if flat(end - gp).length() > LINE_MIN {
        issue_group_move(&mut commands, &members, (gp + end) * 0.5, Some((gp, end)), kind, shift);
        if kind == MoveKind::Move && !shift {
            reset_harvesters(&mut harvesters, &members);
        }
        return;
    }

    // Gegner unter dem Cursor?
    let enemy = pick(
        camera,
        cam_t,
        c,
        targets.iter().filter(|t| *t.2 != PLAYER).map(|(e, gt, _, u, b)| {
            let r = u.map(|u| u.radius).or(b.map(|b| b.radius)).unwrap_or(1.0);
            (e, gt.translation(), r)
        }),
    );
    if let Some(target) = enemy
        && members.iter().any(|m| m.armed)
    {
        for m in members.iter().filter(|m| m.armed) {
            let mut ec = commands.entity(m.e);
            clear_orders(&mut ec);
            ec.insert(AttackTarget(target));
        }
        spawn_marker(&mut commands, MarkerKind::Attack, gp, Some(target));
        return;
    }

    // Sammler auf ein Feld: abbauen; die übrigen fahren hin
    if let Some(node_e) = node
        && members.iter().any(|m| m.harvester)
    {
        for m in members.iter().filter(|m| m.harvester) {
            if let Ok(mut h) = harvesters.get_mut(m.e) {
                h.node = Some(node_e);
                h.state = HarvestState::ToNode;
            }
            clear_orders(&mut commands.entity(m.e));
        }
        let rest: Vec<Member> = members.iter().copied().filter(|m| !m.harvester).collect();
        issue_group_move(&mut commands, &rest, gp, None, kind, shift);
        spawn_marker(&mut commands, MarkerKind::Move, gp, None);
        return;
    }

    issue_group_move(&mut commands, &members, gp, None, kind, shift);
    if kind == MoveKind::Move && !shift {
        reset_harvesters(&mut harvesters, &members);
    }
}

/// R: Stopp, T: Stellung halten
pub fn order_hotkeys(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, sel: Query<(Entity, &Team), (With<Selected>, With<Unit>)>) {
    let stop = keys.just_pressed(KeyCode::KeyR);
    let hold = keys.just_pressed(KeyCode::KeyT);
    if !stop && !hold {
        return;
    }
    for (e, team) in &sel {
        if *team != PLAYER {
            continue;
        }
        let mut ec = commands.entity(e);
        clear_orders(&mut ec);
        if hold {
            ec.insert(HoldPosition);
        }
    }
}

const GROUP_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Strg+Zahl speichert die Auswahl, Shift+Zahl ergänzt, Zahl ruft ab, zweimal Zahl springt hin
pub fn control_groups(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut groups: ResMut<ControlGroups>,
    mut rig: ResMut<CamRig>,
    units: Query<(Entity, &Team, &Transform, Has<Selected>), With<Unit>>,
    buildings: Query<Entity, (With<Selected>, Without<Unit>)>,
) {
    let (shift, ctrl, _) = modifiers(&keys);
    for (i, key) in GROUP_KEYS.into_iter().enumerate() {
        if !keys.just_pressed(key) {
            continue;
        }
        let selected: Vec<Entity> = units.iter().filter(|u| u.3 && *u.1 == PLAYER).map(|u| u.0).collect();
        if ctrl {
            groups.groups[i] = selected;
            continue;
        }
        if shift {
            for e in selected {
                if !groups.groups[i].contains(&e) {
                    groups.groups[i].push(e);
                }
            }
            continue;
        }
        groups.groups[i].retain(|e| units.contains(*e));
        if groups.groups[i].is_empty() {
            continue;
        }
        for (e, _, _, sel) in &units {
            let want = groups.groups[i].contains(&e);
            if sel && !want {
                commands.entity(e).remove::<Selected>();
            } else if want && !sel {
                commands.entity(e).insert(Selected);
            }
        }
        for b in &buildings {
            commands.entity(b).remove::<Selected>();
        }
        let now = time.elapsed_secs_f64();
        if groups.last.is_some_and(|(g, t)| g == i && now - t < DOUBLE_CLICK + 0.1) {
            let pts: Vec<Vec3> = groups.groups[i].iter().filter_map(|e| units.get(*e).ok()).map(|u| u.2.translation).collect();
            let center = pts.iter().sum::<Vec3>() / pts.len() as f32;
            rig.focus = center.with_y(rig.focus.y);
        }
        groups.last = Some((i, now));
    }
}

/// Was liegt unter dem Mauszeiger? Setzt dazu das passende Cursor-Symbol.
pub fn hover(
    mut commands: Commands,
    mut hover: ResMut<Hover>,
    window: Query<(Entity, &Window), With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    targets: Query<(Entity, &GlobalTransform, &Team, Option<&Unit>, Option<&Building>), With<Health>>,
    nodes: Query<(Entity, &GlobalTransform), With<ResourceNode>>,
    selected: Query<(&Team, Has<Weapon>, Has<Harvester>), (With<Selected>, With<Unit>)>,
    drag: Res<DragState>,
    placing: Res<crate::construction::Placing>,
    ui_scale: Res<UiScale>,
    panel: Query<&RelativeCursorPosition, With<SelCard>>,
    mut current: Local<Option<SystemCursorIcon>>,
) {
    hover.0 = None;
    let (Ok((win_e, win)), Ok((camera, cam_t))) = (window.single(), cam.single()) else { return };
    let on_panel = panel.iter().any(|p| p.cursor_over());
    let cursor = win.cursor_position().filter(|c| !on_panel && !over_ui(win, *c, ui_scale.0));
    if let Some(c) = cursor
        && placing.kind.is_none()
    {
        let cands = targets.iter().map(|(e, gt, _, u, b)| {
            let r = u.map(|u| u.radius).or(b.map(|b| b.radius * 0.8)).unwrap_or(1.0);
            (e, gt.translation(), r)
        });
        if let Some(e) = pick(camera, cam_t, c, cands) {
            let own = targets.get(e).is_ok_and(|t| *t.2 == PLAYER);
            hover.0 = Some((e, if own { HoverKind::Own } else { HoverKind::Enemy }));
        } else if let Some(gp) = ground_point(camera, cam_t, c)
            && let Some((e, _)) = nodes.iter().find(|(_, gt)| flat(gt.translation()).distance(flat(gp)) < 3.5)
        {
            hover.0 = Some((e, HoverKind::Node));
        }
    }
    let armed = selected.iter().any(|s| *s.0 == PLAYER && s.1);
    let harvester = selected.iter().any(|s| *s.0 == PLAYER && s.2);
    let icon = if drag.mid.is_some() {
        SystemCursorIcon::Grabbing
    } else {
        match hover.0.map(|h| h.1) {
            Some(HoverKind::Enemy) if armed => SystemCursorIcon::Crosshair,
            Some(HoverKind::Node) if harvester => SystemCursorIcon::Cell,
            Some(HoverKind::Own) => SystemCursorIcon::Pointer,
            _ => SystemCursorIcon::Default,
        }
    };
    if *current != Some(icon) {
        *current = Some(icon);
        commands.entity(win_e).insert(CursorIcon::from(icon));
    }
}

fn ring(gizmos: &mut Gizmos, pos: Vec3, r: f32, col: Color) {
    gizmos.circle(Isometry3d::new(pos + Vec3::Y * 0.15, Quat::from_rotation_x(FRAC_PI_2)), r, col);
}

/// Auswahlkreise, Lebensbalken, Hover-Markierung
pub fn draw_overlays(
    mut gizmos: Gizmos,
    hover: Res<Hover>,
    units: Query<(Entity, &GlobalTransform, &Health, &Team, Option<&Unit>, Option<&Building>, Has<Selected>, Has<HoldPosition>)>,
    nodes: Query<&GlobalTransform, With<ResourceNode>>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
) {
    let Ok(cam_t) = cam.single() else { return };
    let right = cam_t.right().with_y(0.0).normalize_or_zero();
    for (e, gt, hp, team, unit, bld, sel, hold) in &units {
        let pos = gt.translation();
        let r = unit.map(|u| u.radius).or(bld.map(|b| b.radius)).unwrap_or(1.0);
        let hovered = hover.0.is_some_and(|h| h.0 == e);
        if sel {
            ring(&mut gizmos, pos, r + 0.3, Color::srgb(0.4, 1.0, 0.5));
            if hold {
                // Stellung halten: zweiter, eckiger Ring
                let iso = Isometry3d::new(pos + Vec3::Y * 0.15, Quat::from_rotation_x(FRAC_PI_2));
                gizmos.rect(iso, Vec2::splat((r + 0.6) * 2.0), Color::srgb(0.4, 1.0, 0.5));
            }
        } else if hovered {
            let col = if *team == PLAYER { Color::srgba(1.0, 1.0, 1.0, 0.7) } else { Color::srgba(1.0, 0.3, 0.25, 0.85) };
            ring(&mut gizmos, pos, r + 0.3, col);
        }
        let frac = (hp.hp / hp.max).clamp(0.0, 1.0);
        if sel || hovered || frac < 0.999 {
            let height = bld.map_or(2.6, |b| 2.0 + b.radius * 1.3);
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
    if let Some((e, HoverKind::Node)) = hover.0
        && let Ok(gt) = nodes.get(e)
    {
        ring(&mut gizmos, gt.translation(), 3.2, Color::srgba(1.0, 0.85, 0.3, 0.8));
    }
}

/// Wegpunkte, Ziele, Patrouillen und Sammelpunkt der Auswahl; Vorschau der gezogenen Frontlinie
pub fn draw_orders(
    mut gizmos: Gizmos,
    drag: Res<DragState>,
    sel: Query<
        (&GlobalTransform, &Team, Option<&MoveTo>, Has<AttackMove>, Option<&Waypoints>, Option<&Patrol>, Option<&AttackTarget>, Option<&Unit>),
        With<Selected>,
    >,
    rally: Query<(&GlobalTransform, &Rally, &Team), With<Selected>>,
    targets: Query<&GlobalTransform>,
) {
    let lift = |p: Vec3| terrain::on_ground(p) + Vec3::Y * 0.3;
    let mut count = 0;
    let mut spacing: f32 = 3.0;
    for (gt, team, mv, amove, wp, patrol, atk, unit) in &sel {
        if *team != PLAYER {
            continue;
        }
        if let Some(u) = unit {
            count += 1;
            spacing = spacing.max(u.radius * 2.0 + 0.8);
        }
        let mut from = lift(gt.translation());
        if let Some(t) = atk.and_then(|a| targets.get(a.0).ok()) {
            gizmos.line(from, lift(t.translation()), MarkerKind::Attack.color().with_alpha(0.6));
        }
        if let Some(p) = patrol {
            let col = MarkerKind::Patrol.color().with_alpha(0.55);
            gizmos.line(lift(p.a), lift(p.b), col);
            ring(&mut gizmos, lift(p.a), 0.5, col);
            ring(&mut gizmos, lift(p.b), 0.5, col);
            continue;
        }
        if let Some(m) = mv {
            let kind = if amove { MarkerKind::AttackMove } else { MarkerKind::Move };
            let to = lift(m.0);
            gizmos.line(from, to, kind.color().with_alpha(0.5));
            from = to;
        }
        if let Some(w) = wp {
            for (p, attack, _) in &w.0 {
                let kind = if *attack { MarkerKind::AttackMove } else { MarkerKind::Move };
                let to = lift(*p);
                gizmos.line(from, to, kind.color().with_alpha(0.5));
                ring(&mut gizmos, to, 0.4, kind.color().with_alpha(0.7));
                from = to;
            }
        }
    }
    for (gt, r, team) in &rally {
        if *team == PLAYER {
            let col = MarkerKind::Rally.color().with_alpha(0.7);
            let to = lift(r.pos);
            gizmos.line(lift(gt.translation()), to, col);
            gizmos.line(to, to + Vec3::Y * 3.0, col);
            gizmos.line(to + Vec3::Y * 3.0, to + Vec3::new(1.2, 2.5, 0.0), col);
            gizmos.line(to + Vec3::new(1.2, 2.5, 0.0), to + Vec3::Y * 2.0, col);
        }
    }
    // Frontlinie beim Ziehen mit der rechten Maustaste
    if let (Some((_, a)), Some(b)) = (drag.right, drag.right_now)
        && count > 0
        && flat(b - a).length() > LINE_MIN
    {
        let col = Color::srgba(0.4, 1.0, 0.5, 0.8);
        gizmos.line(lift(a), lift(b), col);
        let along = flat(b - a).normalize_or(Vec3::X);
        let facing = Vec3::new(along.z, 0.0, -along.x);
        let slots = formation_slots(count, (a + b) * 0.5, facing, Some(flat(b - a).length()), spacing, true);
        for s in slots {
            ring(&mut gizmos, lift(s), 0.6, col);
        }
    }
}

/// F: alle eigenen Kampfeinheiten auswählen
pub fn select_army(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    units: Query<(Entity, &Team, Has<Weapon>, Has<Selected>, Has<Unit>)>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    for (e, team, armed, sel, is_unit) in &units {
        if *team != PLAYER {
            continue;
        }
        if armed && is_unit {
            commands.entity(e).insert(Selected);
        } else if sel {
            commands.entity(e).remove::<Selected>();
        }
    }
}
