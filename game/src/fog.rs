//! Nebel des Krieges: Was die eigenen Einheiten und Gebäude gerade sehen, ist hell. Schon
//! erkundetes Gelände bleibt gedämpft sichtbar, Unerkundetes liegt im Dunkeln.
//!
//! Ein Gitter mit 1 m Zellen über das Spielfeld. Daraus entsteht eine kleine Textur, die der
//! Gelände-Shader und der Pflanzen-/Fels-Shader abtasten (fest vergebene Handle-ID, damit die
//! Materialien sie schon beim Erzeugen kennen). Gegnerische Einheiten außerhalb der Sicht werden
//! ausgeblendet und als `Fogged` markiert; gegnerische Gebäude erscheinen, sobald ihr Platz
//! einmal erkundet ist. Die Gegner-KI ist davon nicht betroffen, sie hat ihr eigenes Gedächtnis.

use crate::components::*;
use bevy::asset::{RenderAssetUsages, uuid_handle};
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Die Nebel-Textur (R: Helligkeit 0 = unerkundet, ~0.5 = erkundet, 1 = in Sicht)
pub const FOG_TEXTURE: Handle<Image> = uuid_handle!("6f1d2c3a-8b4e-4f7a-9c2d-1e5f3a7b9c11");

/// Zellen je Kante (1 m je Zelle über das ganze Spielfeld)
pub const N: usize = (2.0 * MAP_HALF) as usize;
const EXPLORED: f32 = 0.5;
/// Neuberechnung der Sicht (s); dazwischen wird nur weich überblendet
const INTERVAL: f32 = 0.15;

#[derive(Resource)]
pub struct FogGrid {
    explored: Vec<bool>,
    visible: Vec<bool>,
    /// angezeigte Helligkeit je Zelle, gleitet zum Ziel
    shown: Vec<f32>,
    timer: f32,
    fresh: bool,
}

impl Default for FogGrid {
    fn default() -> Self {
        Self { explored: vec![false; N * N], visible: vec![false; N * N], shown: vec![0.0; N * N], timer: 0.0, fresh: true }
    }
}

fn cell(p: Vec3) -> Option<usize> {
    let x = (p.x + MAP_HALF).floor();
    let z = (p.z + MAP_HALF).floor();
    if x < 0.0 || z < 0.0 || x >= N as f32 || z >= N as f32 {
        return None;
    }
    Some(z as usize * N + x as usize)
}

impl FogGrid {
    /// Sieht der Spieler diesen Punkt gerade?
    pub fn visible(&self, p: Vec3) -> bool {
        cell(p).is_none_or(|i| self.visible[i])
    }

    /// Angezeigte Helligkeit (0 unerkundet .. 1 in Sicht), z. B. für die Minikarte
    pub fn shade(&self, p: Vec3) -> f32 {
        cell(p).map_or(EXPLORED, |i| self.shown[i])
    }

    pub fn explored(&self, p: Vec3) -> bool {
        cell(p).is_none_or(|i| self.explored[i])
    }

    /// Kreis aufdecken
    pub fn reveal(&mut self, p: Vec3, r: f32) {
        let (cx, cz) = (p.x + MAP_HALF, p.z + MAP_HALF);
        if cx + r < 0.0 || cz + r < 0.0 {
            return;
        }
        let x0 = (cx - r).floor().max(0.0) as usize;
        let x1 = ((cx + r).ceil() as usize).min(N - 1);
        let z0 = (cz - r).floor().max(0.0) as usize;
        let z1 = ((cz + r).ceil() as usize).min(N - 1);
        let r2 = r * r;
        for z in z0..=z1 {
            let dz = z as f32 + 0.5 - cz;
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                if dx * dx + dz * dz <= r2 {
                    let i = z * N + x;
                    self.visible[i] = true;
                    self.explored[i] = true;
                }
            }
        }
    }
}

/// Sichtweite einer Einheit. Artillerie sieht weniger weit, als sie schießt, und braucht Aufklärung.
pub fn unit_sight(kind: UnitKind) -> f32 {
    match kind {
        UnitKind::Sammler => 13.0,
        UnitKind::Panzer => 17.0,
        UnitKind::Werfer => 18.0,
        UnitKind::Spaeher => 28.0,
        UnitKind::Bollwerk => 16.0,
        UnitKind::Haubitze => 15.0,
    }
}

/// Sichtweite eines Gebäudes: Türme sehen etwas über ihre Reichweite hinaus
fn building_sight(b: &Building, weapon: Option<&Weapon>, wall: bool, site: bool) -> f32 {
    if wall {
        return 6.0;
    }
    // Baustellen sehen nur ihre Umgebung
    let base = if site { 8.0 } else { 12.0 + b.radius * 3.0 };
    weapon.map_or(base, |w| base.max(w.range + 2.0))
}

pub fn setup_fog(mut images: ResMut<Assets<Image>>) {
    let mut img = Image::new_fill(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    let _ = images.insert(&FOG_TEXTURE, img);
}

/// Sicht neu berechnen, Anzeige weich nachführen, Textur schreiben
pub fn update_fog(
    time: Res<Time>,
    mut fog: ResMut<FogGrid>,
    mut images: ResMut<Assets<Image>>,
    units: Query<(&Transform, &Team, &Unit)>,
    buildings: Query<(&Transform, &Team, &Building, Option<&Weapon>, Has<Wall>, Has<Construction>)>,
    eclipse: Option<Res<crate::halkyon::Eclipse>>,
) {
    // In der Halkyon-Finsternis sieht der Spieler weniger weit
    let dark = eclipse.map_or(0.0, |e| e.shade);
    let unit_k = 1.0 - crate::halkyon::UNIT_SIGHT_LOSS * dark;
    let building_k = 1.0 - crate::halkyon::BUILDING_SIGHT_LOSS * dark;
    let dt = time.delta_secs();
    fog.timer -= dt;
    if fog.timer <= 0.0 || fog.fresh {
        fog.timer = INTERVAL;
        fog.visible.iter_mut().for_each(|v| *v = false);
        for (t, team, u) in &units {
            if *team == PLAYER {
                fog.reveal(t.translation, unit_sight(u.kind) * unit_k);
            }
        }
        for (t, team, b, w, wall, site) in &buildings {
            if *team == PLAYER {
                fog.reveal(t.translation, building_sight(b, w, wall, site) * building_k);
            }
        }
    }
    // Aufdecken schnell, Zuziehen langsamer
    let (up, down) = ((dt * 6.0).min(1.0), (dt * 2.5).min(1.0));
    let fresh = std::mem::take(&mut fog.fresh);
    let FogGrid { explored, visible, shown, .. } = &mut *fog;
    let mut changed = fresh;
    for i in 0..N * N {
        let target = if visible[i] {
            1.0
        } else if explored[i] {
            EXPLORED
        } else {
            0.0
        };
        let s = shown[i];
        let next = if fresh {
            target
        } else if target > s {
            s + (target - s) * up
        } else {
            s + (target - s) * down
        };
        if (next * 255.0) as u8 != (s * 255.0) as u8 || fresh {
            changed = true;
        }
        shown[i] = next;
    }
    if !changed {
        return;
    }
    let Some(mut img) = images.get_mut(&FOG_TEXTURE) else { return };
    let Some(data) = img.data.as_mut() else { return };
    for (i, s) in shown.iter().enumerate() {
        data[i * 4] = (s * 255.0) as u8;
    }
}

/// Gegner außerhalb der Sicht ausblenden (Gebäude erst, solange ihr Platz unerkundet ist)
pub fn hide_enemies(
    mut commands: Commands,
    fog: Res<FogGrid>,
    mut q: Query<(Entity, &Transform, &Team, &mut Visibility, Has<Building>, Has<Fogged>), (Or<(With<Unit>, With<Building>)>, Without<Projectile>)>,
    mut shots: Query<(&Transform, &Projectile, &mut Visibility), (Without<Unit>, Without<Building>)>,
) {
    for (e, t, team, mut vis, building, fogged) in &mut q {
        if *team == PLAYER {
            continue;
        }
        let seen = if building { fog.explored(t.translation) } else { fog.visible(t.translation) };
        let want = if seen { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if seen && fogged {
            commands.entity(e).remove::<Fogged>();
        } else if !seen && !fogged {
            commands.entity(e).insert(Fogged);
        }
    }
    for (t, p, mut vis) in &mut shots {
        if p.team == PLAYER {
            continue;
        }
        let want = if fog.visible(t.translation) { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_marks_visible_and_explored() {
        let mut f = FogGrid::default();
        let p = Vec3::new(10.0, 0.0, -20.0);
        assert!(!f.visible(p) && !f.explored(p));
        f.reveal(p, 5.0);
        assert!(f.visible(p) && f.visible(p + Vec3::X * 4.0));
        assert!(!f.visible(p + Vec3::X * 6.0));
        // Sicht verfällt, erkundet bleibt
        f.visible.iter_mut().for_each(|v| *v = false);
        assert!(!f.visible(p) && f.explored(p));
        // Rand der Karte: kein Absturz, außerhalb gilt als sichtbar
        f.reveal(Vec3::new(69.0, 0.0, 69.0), 10.0);
        f.reveal(Vec3::new(-90.0, 0.0, 0.0), 10.0);
        assert!(f.visible(Vec3::new(200.0, 0.0, 0.0)));
    }
}
