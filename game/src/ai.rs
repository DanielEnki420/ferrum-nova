//! Gegner-KI (Vaar-Schwarm) in drei Ebenen.
//!
//! - **Strategie** (`enemy_ai`, einmal pro Sekunde): sieht nur, was ihre Einheiten und Gebäude
//!   gerade erfassen, und merkt es sich. Ein Späher erkundet die Karte. Die Produktion reagiert auf
//!   das Gesehene (Konter). Die Wirtschaft wächst mit Sammlern je Raffinerie und einer zweiten
//!   Raffinerie an einem weiteren Feld, notfalls über einen Geschützturm als Brückenkopf.
//! - **Verteidigung**: Gegner nahe Basis oder Sammlern sind eine Bedrohung. Die Reserve rückt aus,
//!   eine laufende Angriffswelle wird bei Übermacht zurückgerufen, Sammler fliehen,
//!   neue Geschütztürme kommen an die bedrohte Seite.
//! - **Taktik** (`ai_tactics`, viermal pro Sekunde): Wellen sammeln sich und rücken geschlossen vor,
//!   Einheiten wählen ihre Ziele nach Waffe und Zustand, Haubitzen halten Abstand,
//!   schwer beschädigte Einheiten ziehen sich zur Reparatur ans HQ zurück.

use crate::components::*;
use crate::construction::{find_spot, PlaceRequest, Site, BUILD_RANGE};
use crate::terrain::FIELDS;
use crate::units::{BuildRequest, Snapshot};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// Sichtweite von Einheiten und Gebäuden (Gebäude ab ihrem Rand)
const SIGHT_UNIT: f32 = 22.0;
const SIGHT_BUILDING: f32 = 20.0;
/// So nah an Basis oder Sammlern gilt ein Gegner als Bedrohung
const THREAT_BASE: f32 = 28.0;
const THREAT_HARVESTER: f32 = 16.0;
/// Sammler fliehen vor bewaffneten Gegnern in diesem Umkreis
const FLEE: f32 = 13.0;
/// Haubitzen weichen Gegnern in diesem Umkreis aus
const KITE: f32 = 10.0;

/// Was die KI von einem Spielerobjekt weiß
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seen {
    Unit(UnitKind),
    Structure(BuildingKind),
    Hq,
}

#[derive(Clone, Copy, Debug)]
pub struct Sighting {
    pub what: Seen,
    pub pos: Vec3,
    /// Spielzeit der letzten Sichtung
    pub time: f32,
}

/// Aufgabe einer Kampfeinheit der KI
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AiRole {
    /// wartet am Sammelpunkt
    Reserve,
    /// erkundet die Karte
    Scout,
    /// gehört zur laufenden Angriffswelle
    Wave,
    /// verteidigt die Basis
    Defend,
}

/// Zieht sich zur Reparatur ans HQ zurück
#[derive(Component)]
pub struct AiRetreat;

/// Weicht bis zu diesem Zeitpunkt aus (Haubitze)
#[derive(Component)]
pub struct AiKite(pub f32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WavePhase {
    /// sammeln am Bereitstellungsraum
    Gather,
    /// geschlossen vorrücken
    Advance,
}

#[derive(Debug)]
pub struct Wave {
    pub members: Vec<Entity>,
    pub phase: WavePhase,
    pub target: Vec3,
    /// Kampfwert beim Aufbruch (Kosten mal Zustand)
    pub start_value: f32,
    pub timer: f32,
}

#[derive(Resource)]
pub struct AiState {
    pub think: f32,
    pub tactic: f32,
    pub elapsed: f32,
    pub next_attack: f32,
    pub wave_size: usize,
    /// Wartezeit bis zum nächsten Gebäude
    pub build_cd: f32,
    /// Gedächtnis: zuletzt gesehene Spielerobjekte
    pub intel: HashMap<Entity, Sighting>,
    pub wave: Option<Wave>,
    /// Bedrohung der Basis: Schwerpunkt und Stärke
    pub threat: Option<(Vec3, f32)>,
    pub last_threat: f32,
    pub threat_dir: Option<Vec3>,
    pub scout_leg: usize,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            think: 0.0,
            tactic: 0.0,
            elapsed: 0.0,
            next_attack: 150.0,
            wave_size: 5,
            build_cd: 30.0,
            intel: HashMap::default(),
            wave: None,
            threat: None,
            last_threat: -999.0,
            threat_dir: None,
            scout_leg: 0,
        }
    }
}

fn to_player() -> Vec3 {
    (PLAYER_HQ_POS - ENEMY_HQ_POS).normalize()
}

/// Sammelpunkt der Reserve vor der Basis
pub fn rally() -> Vec3 {
    ENEMY_HQ_POS + to_player() * 16.0
}

/// Kampfwert einer Einheit: ihre Kosten
pub fn value(k: UnitKind) -> f32 {
    let c = k.cost();
    (c[0] + c[1]) as f32
}

/// Kleiner fester Versatz je Einheit, damit sich Gruppen nicht auf einem Punkt stapeln
fn jitter(e: Entity) -> Vec3 {
    let i = e.index().index() as f32;
    Vec3::new((i * 2.399).cos(), 0.0, (i * 2.399).sin()) * (2.0 + (i * 0.618).fract() * 4.0)
}

fn clamp_map(p: Vec3) -> Vec3 {
    let lim = MAP_HALF - 3.0;
    Vec3::new(p.x.clamp(-lim, lim), p.y, p.z.clamp(-lim, lim))
}

/// Route des Spähers: Felder, die Kartenmitte und die Umgebung der Spielerbasis
fn scout_route() -> Vec<Vec3> {
    let mut r: Vec<Vec3> = FIELDS.iter().map(|f| f.0).collect();
    r.sort_by(|a, b| a.distance(ENEMY_HQ_POS).total_cmp(&b.distance(ENEMY_HQ_POS)));
    r.push(PLAYER_HQ_POS + Vec3::new(14.0, 0.0, -4.0));
    r.push(PLAYER_HQ_POS + Vec3::new(4.0, 0.0, -14.0));
    r
}

/// Welcher Kampftyp als Nächstes? Gewichte aus Grundmischung und Kontern gegen das Gesehene,
/// gebaut wird der Typ, der in der eigenen Armee am meisten fehlt.
pub fn choose_unit(own: &[usize; 6], seen: &[f32; 6], turrets_seen: usize, factory: bool) -> UnitKind {
    use UnitKind::*;
    let mut w = [0.0f32; 6];
    w[Panzer.idx()] = 3.0;
    w[Werfer.idx()] = 2.0;
    w[Spaeher.idx()] = 1.0;
    if factory {
        w[Bollwerk.idx()] = 1.0;
        w[Haubitze.idx()] = 1.0;
    }
    let t = turrets_seen as f32;
    // Geschütztürme: Artillerie mit größerer Reichweite, ohne Fabrik Raketenwerfer
    if factory {
        w[Haubitze.idx()] += 2.0 * t;
    } else {
        w[Werfer.idx()] += 1.5 * t;
    }
    // viel Panzerung: Raketen und schwere Panzer
    let heavy = seen[Panzer.idx()] + 2.0 * seen[Bollwerk.idx()];
    w[Werfer.idx()] += 0.5 * heavy;
    if factory {
        w[Bollwerk.idx()] += 0.6 * heavy;
    } else {
        w[Panzer.idx()] += 0.4 * heavy;
    }
    // schnelle Späher: Panzer halten sie auf
    w[Panzer.idx()] += 0.6 * seen[Spaeher.idx()];
    // Artillerie: schnelle Späher fahren sie an
    w[Spaeher.idx()] += 0.8 * (seen[Werfer.idx()] + seen[Haubitze.idx()]);

    let total_w: f32 = w.iter().sum();
    let total_own: usize = own.iter().enumerate().filter(|(i, _)| *i != Sammler.idx()).map(|(_, n)| n).sum();
    let mut best = (Panzer, f32::MIN);
    for k in UnitKind::ALL {
        if k == Sammler || w[k.idx()] <= 0.0 {
            continue;
        }
        let want = w[k.idx()] / total_w;
        let have = if total_own == 0 { 0.0 } else { own[k.idx()] as f32 / total_own as f32 };
        let gap = want - have;
        if gap > best.1 {
            best = (k, gap);
        }
    }
    best.0
}

/// Lagebild eines Objekts in diesem Takt
struct Obj {
    e: Entity,
    pos: Vec3,
    team: Team,
    hp: f32,
    unit: Option<UnitKind>,
    building: Option<(f32, Option<BuildingKind>, bool)>,
    constructing: bool,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn enemy_ai(
    mut commands: Commands,
    time: Res<Time>,
    mut ai: ResMut<AiState>,
    mut stock: ResMut<Stock>,
    outcome: Res<Outcome>,
    mut builds: MessageWriter<BuildRequest>,
    mut places: MessageWriter<PlaceRequest>,
    all: Query<(
        Entity,
        &Transform,
        &Team,
        &Health,
        Option<&Unit>,
        Option<&Building>,
        Option<&Structure>,
        Has<Hq>,
        Has<Construction>,
    )>,
    roles: Query<(Option<&AiRole>, Has<MoveTo>, Has<AttackTarget>, Has<AiRetreat>)>,
    mut harvesters: Query<(Entity, &Transform, &Team, &mut Harvester)>,
    hqs: Query<(&Team, &Hq)>,
    nodes: Query<(Entity, &Transform, &ResourceNode)>,
    memory: Option<Res<crate::vaar::VaarMemory>>,
) {
    if *outcome != Outcome::Running {
        return;
    }
    let dt = time.delta_secs();
    ai.elapsed += dt;
    ai.think -= dt;
    if ai.think > 0.0 {
        return;
    }
    ai.think = 1.0;
    let now = ai.elapsed;

    // Kleiner Bonus, damit der Gegner mithalten kann
    stock.0[ENEMY.0 as usize][0] += 1;
    stock.0[ENEMY.0 as usize][1] += 1;

    // ------------------------------------------------------------ Lage erfassen
    let objs: Vec<Obj> = all
        .iter()
        .map(|(e, t, team, h, u, b, s, hq, cons)| Obj {
            e,
            pos: t.translation,
            team: *team,
            hp: h.hp / h.max,
            unit: u.map(|u| u.kind),
            building: b.map(|b| (b.radius, s.map(|s| s.0), hq)),
            constructing: cons,
        })
        .collect();
    let eyes: Vec<(Vec3, f32)> = objs
        .iter()
        .filter(|o| o.team == ENEMY)
        .map(|o| (o.pos, o.building.map_or(SIGHT_UNIT, |b| SIGHT_BUILDING + b.0)))
        .collect();
    for o in objs.iter().filter(|o| o.team == PLAYER) {
        let what = match (o.unit, o.building) {
            (Some(k), _) => Seen::Unit(k),
            (None, Some((_, Some(k), _))) => Seen::Structure(k),
            (None, Some((_, None, true))) => Seen::Hq,
            _ => continue,
        };
        // Das gegnerische HQ ist von Anfang an bekannt
        let visible = what == Seen::Hq || eyes.iter().any(|(p, r)| flat(*p).distance(flat(o.pos)) < *r);
        if visible {
            ai.intel.insert(o.e, Sighting { what, pos: o.pos, time: now });
        }
    }
    // Zerstörtes vergessen, Einheiten nach einer Weile auch (sie bewegen sich)
    ai.intel.retain(|e, s| all.contains(*e) && (!matches!(s.what, Seen::Unit(_)) || now - s.time < 45.0));

    let mine: Vec<&Obj> = objs.iter().filter(|o| o.team == ENEMY).collect();
    let my_buildings: Vec<&Obj> = mine.iter().copied().filter(|o| o.building.is_some()).collect();
    let harvester_pos: Vec<Vec3> = mine.iter().filter(|o| o.unit == Some(UnitKind::Sammler)).map(|o| o.pos).collect();
    // gerade sichtbare, bewaffnete Gegner
    let hostiles: Vec<(Vec3, UnitKind)> = ai
        .intel
        .values()
        .filter_map(|s| match s.what {
            Seen::Unit(k) if k != UnitKind::Sammler && now - s.time < 0.5 => Some((s.pos, k)),
            _ => None,
        })
        .collect();

    // ------------------------------------------------------------ Bedrohung
    let (mut tv, mut tc) = (0.0, Vec3::ZERO);
    for (p, k) in &hostiles {
        let near_base = my_buildings.iter().any(|b| flat(b.pos).distance(flat(*p)) < THREAT_BASE + b.building.map_or(0.0, |b| b.0));
        let near_harv = harvester_pos.iter().any(|h| flat(*h).distance(flat(*p)) < THREAT_HARVESTER);
        if near_base || near_harv {
            tv += value(*k);
            tc += *p * value(*k);
        }
    }
    ai.threat = (tv > 0.0).then(|| (tc / tv, tv));
    if let Some((p, _)) = ai.threat {
        ai.last_threat = now;
        ai.threat_dir = Some(flat(p - ENEMY_HQ_POS).normalize_or(to_player()));
    }

    // ------------------------------------------------------------ Bestand
    let mut own = [0usize; 6];
    for o in &mine {
        if let Some(k) = o.unit {
            own[k.idx()] += 1;
        }
    }
    let mut seen = [0usize; 6];
    let mut turrets_seen = 0;
    for s in ai.intel.values() {
        match s.what {
            Seen::Unit(k) => seen[k.idx()] += 1,
            Seen::Structure(BuildingKind::Geschuetzturm | BuildingKind::Raketenturm) => turrets_seen += 1,
            _ => {}
        }
    }
    // Gedächtnis aus früheren Partien (vaar.rs): Vorwissen, das mit eigener Sicht verblasst
    let mem = memory.as_deref();
    let seen_total: usize = seen.iter().enumerate().filter(|(i, _)| *i != UnitKind::Sammler.idx()).map(|(_, n)| n).sum();
    let prior = mem.map_or([0.0; 6], |m| m.prior(seen_total));
    let seenf: [f32; 6] = std::array::from_fn(|i| seen[i] as f32 + prior[i]);
    let expects_rush = mem.is_some_and(|m| m.expects_rush());
    let expects_turtle = mem.is_some_and(|m| m.expects_turtle());
    let count = |k: BuildingKind| my_buildings.iter().filter(|o| o.building.is_some_and(|b| b.1 == Some(k))).count();
    let factory_ready = my_buildings.iter().any(|o| o.building.is_some_and(|b| b.1 == Some(BuildingKind::Fabrik)) && !o.constructing);
    let refineries = count(BuildingKind::Raffinerie);
    let depots: Vec<Vec3> = my_buildings
        .iter()
        .filter(|o| !o.constructing && (o.building.is_some_and(|b| b.2 || b.1 == Some(BuildingKind::Raffinerie))))
        .map(|o| o.pos)
        .collect();
    let node_list: Vec<(Entity, Vec3, ResKind, u32)> = nodes.iter().map(|(e, t, n)| (e, t.translation, n.kind, n.amount)).collect();
    let node_pos: Vec<Vec3> = node_list.iter().map(|n| n.1).collect();

    // ------------------------------------------------------------ Gebäude
    ai.build_cd -= 1.0;
    let wants_heavy = expects_turtle || turrets_seen >= 2 || seen[UnitKind::Panzer.idx()] + 2 * seen[UnitKind::Bollwerk.idx()] >= 4;
    let recent_threat = now - ai.last_threat < 60.0;
    // gegen frühe Angreifer steht der erste Turm eher
    let first_turret = if expects_rush { 45.0 } else { 80.0 };
    let turrets_wanted = if now < first_turret { 0 } else { (1 + ((now - first_turret) / 70.0) as usize + recent_threat as usize).min(5) };
    // Raketentürme erst, wenn die Wirtschaft läuft: einer ab Minute 4,5, dann alle 2,5 Minuten einer mehr
    let rockets_wanted = if now < 270.0 { 0 } else { (1 + ((now - 270.0) / 150.0) as usize).min(3) };
    let site_list: Vec<Site> = objs
        .iter()
        .filter_map(|o| o.building.map(|b| Site { pos: o.pos, radius: b.0, team: o.team, wall: b.1.is_some_and(|k| k.is_wall()), gate: b.1 == Some(BuildingKind::Tor) }))
        .collect();
    let threatened_node = |p: Vec3| hostiles.iter().any(|(h, _)| flat(*h).distance(flat(p)) < 18.0);
    // Ausbaufeld: auf der eigenen Kartenhälfte, weit weg von jeder Abladestelle, sicher
    let expansion = node_list
        .iter()
        .filter(|n| n.3 > 300)
        .filter(|n| n.1.distance(ENEMY_HQ_POS) < n.1.distance(PLAYER_HQ_POS))
        .filter(|n| depots.iter().all(|d| flat(*d).distance(flat(n.1)) > 22.0))
        .filter(|n| !threatened_node(n.1))
        .max_by_key(|n| n.3)
        .map(|n| n.1);
    let expansion_refinery = my_buildings
        .iter()
        .filter(|o| o.building.is_some_and(|b| b.1 == Some(BuildingKind::Raffinerie)))
        .find(|o| o.pos.distance(ENEMY_HQ_POS) > 30.0)
        .map(|o| o.pos);
    let turret_near = |p: Vec3| my_buildings.iter().any(|o| o.building.is_some_and(|b| b.1 == Some(BuildingKind::Geschuetzturm)) && o.pos.distance(p) < 14.0);

    let front = ai.threat_dir.filter(|_| recent_threat).or(mem.and_then(|m| m.expected_front())).unwrap_or(to_player());
    let goal: Option<(BuildingKind, Vec3)> = if refineries == 0 && now > 35.0 {
        // zwischen HQ und dem nächsten Feld
        let field = node_pos.iter().min_by(|a, b| a.distance(ENEMY_HQ_POS).total_cmp(&b.distance(ENEMY_HQ_POS))).copied().unwrap_or(ENEMY_HQ_POS);
        Some((BuildingKind::Raffinerie, ENEMY_HQ_POS.lerp(field, 0.55)))
    } else if count(BuildingKind::Geschuetzturm) < turrets_wanted {
        // zur Front oder zur zuletzt bedrohten Seite
        let side = Vec3::new(-front.z, 0.0, front.x);
        let n = count(BuildingKind::Geschuetzturm) as f32;
        let sign = if (n as usize).is_multiple_of(2) { 1.0 } else { -1.0 };
        Some((BuildingKind::Geschuetzturm, ENEMY_HQ_POS + front * 13.0 + side * sign * (4.0 + n * 3.0)))
    } else if count(BuildingKind::Fabrik) == 0 && (now > 200.0 || (now > 120.0 && wants_heavy)) {
        Some((BuildingKind::Fabrik, ENEMY_HQ_POS - to_player() * 2.0 + Vec3::new(to_player().z, 0.0, -to_player().x) * 11.0))
    } else if count(BuildingKind::Raketenturm) < rockets_wanted {
        // hinter den Geschütztürmen, abwechselnd links und rechts
        let side = Vec3::new(-front.z, 0.0, front.x);
        let n = count(BuildingKind::Raketenturm) as f32;
        let sign = if (n as usize).is_multiple_of(2) { -1.0 } else { 1.0 };
        Some((BuildingKind::Raketenturm, ENEMY_HQ_POS + front * 7.0 + side * sign * (7.0 + n * 2.0)))
    } else if refineries < 2 && now > 240.0 && !recent_threat {
        expansion.map(|field| {
            let target = field + flat(ENEMY_HQ_POS - field).normalize_or_zero() * 8.0;
            if find_spot(BuildingKind::Raffinerie, target, ENEMY, &site_list, &node_pos).is_some() {
                (BuildingKind::Raffinerie, target)
            } else {
                // außer Baureichweite: Geschützturm als Brückenkopf in Richtung Feld
                let from = site_list
                    .iter()
                    .filter(|s| s.team == ENEMY)
                    .min_by(|a, b| a.pos.distance(field).total_cmp(&b.pos.distance(field)))
                    .map_or((ENEMY_HQ_POS, 4.8), |s| (s.pos, s.radius));
                let dir = flat(field - from.0).normalize_or_zero();
                (BuildingKind::Geschuetzturm, from.0 + dir * (BUILD_RANGE + from.1 - 3.0))
            }
        })
    } else {
        // Außenposten absichern
        expansion_refinery
            .filter(|r| !turret_near(*r))
            .map(|r| (BuildingKind::Geschuetzturm, r + flat(PLAYER_HQ_POS - r).normalize_or_zero() * 7.0))
    };
    let mut saving = false;
    if let Some((kind, target)) = goal
        && ai.build_cd <= 0.0
    {
        if stock.can_afford(ENEMY, kind.cost()) {
            if let Some(pos) = find_spot(kind, target, ENEMY, &site_list, &node_pos) {
                places.write(PlaceRequest { team: ENEMY, kind, pos, rot: None });
                ai.build_cd = 12.0;
            } else {
                ai.build_cd = 20.0;
            }
        } else {
            saving = true;
        }
    }

    // ------------------------------------------------------------ Produktion
    let queue_len = hqs.iter().find(|(t, _)| **t == ENEMY).map(|(_, hq)| hq.queue.len()).unwrap_or(99);
    let harv_count = own[UnitKind::Sammler.idx()];
    let harv_wanted = (4 + 2 * refineries).min(9);
    let under_attack = ai.threat.is_some_and(|(_, v)| v > 300.0);
    if queue_len < 2 && (!saving || harv_count < 4 || ai.threat.is_some()) {
        let scout_alive = mine.iter().any(|o| roles.get(o.e).is_ok_and(|r| r.0 == Some(&AiRole::Scout)));
        let kind = if harv_count < 3 || (harv_count < harv_wanted && !under_attack) {
            UnitKind::Sammler
        } else if !scout_alive && own[UnitKind::Spaeher.idx()] == 0 {
            UnitKind::Spaeher
        } else {
            choose_unit(&own, &seenf, turrets_seen, factory_ready)
        };
        builds.write(BuildRequest { team: ENEMY, kind });
    }

    // ------------------------------------------------------------ Sammler
    let assigned: Vec<Option<Entity>> = harvesters_nodes(&harvesters);
    let s = stock.0[ENEMY.0 as usize];
    let want_kind = if s[1] * 2 < s[0] { ResKind::Plasma } else { ResKind::Ferrit };
    for (e, t, team, mut h) in &mut harvesters {
        if *team != ENEMY {
            continue;
        }
        let me = t.translation;
        let home = depots.iter().min_by(|a, b| a.distance(me).total_cmp(&b.distance(me))).copied().unwrap_or(ENEMY_HQ_POS);
        // Flucht vor bewaffneten Gegnern
        if hostiles.iter().any(|(p, _)| flat(*p).distance(flat(me)) < FLEE) {
            h.node = None;
            h.state = if h.cargo > 0 { HarvestState::Returning } else { HarvestState::Idle };
            commands.entity(e).insert(MoveTo(home));
            continue;
        }
        let moving = roles.get(e).is_ok_and(|r| r.1);
        if h.state != HarvestState::Idle || h.cargo > 0 || h.node.is_some() || moving {
            continue;
        }
        // Feld zuteilen: nah an einer Abladestelle, wenig belegt, ungefährlich
        let best = node_list
            .iter()
            .filter(|n| n.3 > 0)
            .map(|n| {
                let to_depot = depots.iter().map(|d| flat(*d).distance(flat(n.1))).fold(f32::MAX, f32::min);
                let crowd = assigned.iter().filter(|a| **a == Some(n.0)).count() as f32;
                let danger = if threatened_node(n.1) { 200.0 } else { 0.0 };
                let pref = if n.2 == want_kind { 0.0 } else { 35.0 };
                (n.0, to_depot + crowd * 10.0 + danger + pref)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((ne, _)) = best {
            h.node = Some(ne);
            h.state = HarvestState::ToNode;
        }
    }

    // ------------------------------------------------------------ Armee
    let combat: Vec<&Obj> = mine.iter().copied().filter(|o| o.unit.is_some_and(|k| k != UnitKind::Sammler)).collect();
    let role_of = |e: Entity| roles.get(e).ok().and_then(|r| r.0.copied());
    let route = scout_route();
    let mut scout_taken = combat.iter().any(|o| role_of(o.e) == Some(AiRole::Scout));
    for o in &combat {
        let (role, moving, attacking, retreating) = match roles.get(o.e) {
            Ok((r, m, a, rt)) => (r.copied(), m, a, rt),
            Err(_) => continue,
        };
        match role {
            None => {
                let r = if !scout_taken && o.unit == Some(UnitKind::Spaeher) {
                    scout_taken = true;
                    AiRole::Scout
                } else {
                    AiRole::Reserve
                };
                commands.entity(o.e).insert(r);
            }
            Some(AiRole::Scout) => {
                if o.hp < 0.5 {
                    // angeschlagen: heim, ein anderer übernimmt
                    commands.entity(o.e).insert((AiRole::Reserve, MoveTo(rally())));
                } else if !moving {
                    let wp = route[ai.scout_leg % route.len()];
                    ai.scout_leg += 1;
                    commands.entity(o.e).insert(MoveTo(wp)).remove::<AttackMove>();
                }
            }
            Some(AiRole::Reserve) => {
                let spot = rally() + jitter(o.e);
                if !moving && !attacking && !retreating && flat(o.pos).distance(flat(spot)) > 4.0 {
                    commands.entity(o.e).insert((MoveTo(spot), AttackMove));
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ Verteidigung
    let home_value: f32 = combat
        .iter()
        .filter(|o| matches!(role_of(o.e), Some(AiRole::Reserve | AiRole::Defend)))
        .map(|o| o.unit.map_or(0.0, value) * o.hp)
        .sum::<f32>()
        + count(BuildingKind::Geschuetzturm) as f32 * 150.0
        + count(BuildingKind::Raketenturm) as f32 * 130.0;
    if let Some((tp, tv)) = ai.threat {
        // Übermacht an der Basis: die Welle kehrt um
        let recall = tv > home_value * 0.8;
        if recall && let Some(w) = ai.wave.take() {
            for e in w.members {
                commands.entity(e).insert(AiRole::Defend);
            }
            ai.next_attack = now + 30.0;
        }
        for o in &combat {
            let role = role_of(o.e);
            let busy = roles.get(o.e).is_ok_and(|r| r.2 || r.3);
            if matches!(role, Some(AiRole::Reserve | AiRole::Defend)) || (recall && role == Some(AiRole::Wave)) {
                commands.entity(o.e).insert(AiRole::Defend);
                if !busy {
                    commands.entity(o.e).insert((MoveTo(tp + jitter(o.e) * 0.6), AttackMove));
                }
            }
        }
    } else if now - ai.last_threat > 6.0 {
        for o in &combat {
            if role_of(o.e) == Some(AiRole::Defend) {
                commands.entity(o.e).insert(AiRole::Reserve);
            }
        }
    }

    // ------------------------------------------------------------ Angriffswellen
    let alive = |e: &Entity| objs.iter().any(|o| o.e == *e);
    if let Some(w) = &mut ai.wave {
        w.members.retain(alive);
    }
    if ai.wave.as_ref().is_some_and(|w| w.members.is_empty()) {
        ai.wave = None;
        ai.next_attack = now + 40.0;
    }
    if ai.wave.is_none() && now >= ai.next_attack && ai.threat.is_none() {
        let ready: Vec<&Obj> = combat
            .iter()
            .copied()
            .filter(|o| role_of(o.e) == Some(AiRole::Reserve) && o.hp > 0.6 && flat(o.pos).distance(flat(rally())) < 20.0)
            .filter(|o| !roles.get(o.e).is_ok_and(|r| r.3))
            .collect();
        if ready.len() >= ai.wave_size {
            // nicht alles schicken: der Rest bleibt als Reserve zur Verteidigung
            let mut ready = ready;
            ready.truncate(ai.wave_size + 4);
            let artillery = ready.iter().any(|o| o.unit == Some(UnitKind::Haubitze));
            let target = pick_target(&ai.intel, artillery);
            let start_value = ready.iter().map(|o| o.unit.map_or(0.0, value) * o.hp).sum();
            for o in &ready {
                commands.entity(o.e).insert(AiRole::Wave);
            }
            ai.wave = Some(Wave { members: ready.iter().map(|o| o.e).collect(), phase: WavePhase::Gather, target, start_value, timer: 0.0 });
            ai.wave_size = (ai.wave_size + 2).min(16);
        }
    }
    let intel = ai.intel.clone();
    let mut disband = false;
    if let Some(w) = &mut ai.wave {
        w.timer += 1.0;
        let members: Vec<&Obj> = w.members.iter().filter_map(|e| objs.iter().find(|o| o.e == *e)).collect();
        let centroid = members.iter().map(|o| flat(o.pos)).sum::<Vec3>() / members.len().max(1) as f32;
        match w.phase {
            WavePhase::Gather => {
                // Bereitstellung vor dem Sammelpunkt, dann geschlossen los
                let stage = rally() + to_player() * 8.0;
                let together = members.iter().all(|o| flat(o.pos).distance(centroid) < 10.0) && centroid.distance(flat(stage)) < 8.0;
                if together || w.timer > 25.0 {
                    w.phase = WavePhase::Advance;
                } else {
                    for o in &members {
                        if !roles.get(o.e).is_ok_and(|r| r.2) {
                            commands.entity(o.e).insert((MoveTo(stage + jitter(o.e)), AttackMove));
                        }
                    }
                }
            }
            WavePhase::Advance => {
                let strength: f32 = members.iter().map(|o| o.unit.map_or(0.0, value) * o.hp).sum();
                if strength < w.start_value * 0.35 {
                    // zu schwach geworden: Rückzug zum Sammelpunkt
                    for o in &members {
                        commands.entity(o.e).insert((AiRole::Reserve, MoveTo(rally() + jitter(o.e)))).remove::<(AttackMove, AttackTarget)>();
                    }
                    disband = true;
                } else {
                    // Ziel erreicht und nichts mehr da: nächstes Ziel
                    let target_gone = !intel.values().any(|s| !matches!(s.what, Seen::Unit(_)) && flat(s.pos).distance(flat(w.target)) < 6.0);
                    if target_gone && centroid.distance(flat(w.target)) < 14.0 {
                        let artillery = members.iter().any(|o| o.unit == Some(UnitKind::Haubitze));
                        w.target = pick_target(&intel, artillery);
                    }
                    let step = centroid + (flat(w.target) - centroid).clamp_length_max(10.0);
                    for o in &members {
                        if roles.get(o.e).is_ok_and(|r| r.2 || r.3) {
                            continue;
                        }
                        // Nachzügler schließen auf, die anderen rücken gemeinsam vor
                        let goal = if flat(o.pos).distance(centroid) > 12.0 { centroid } else { step + jitter(o.e) * 0.6 };
                        commands.entity(o.e).insert((MoveTo(goal), AttackMove));
                    }
                }
            }
        }
    }
    if disband {
        ai.wave = None;
        ai.next_attack = now + 45.0;
    }
}

fn harvesters_nodes(q: &Query<(Entity, &Transform, &Team, &mut Harvester)>) -> Vec<Option<Entity>> {
    q.iter().filter(|(_, _, t, _)| **t == ENEMY).map(|(_, _, _, h)| h.node).collect()
}

/// Angriffsziel aus dem Gedächtnis: nahe Gebäude zuerst, Raffinerien (Wirtschaft) bevorzugt,
/// mit Artillerie die Geschütztürme. Ohne Wissen das HQ.
pub fn pick_target(intel: &HashMap<Entity, Sighting>, artillery: bool) -> Vec3 {
    intel
        .values()
        .filter_map(|s| {
            let bonus = match s.what {
                Seen::Structure(BuildingKind::Geschuetzturm | BuildingKind::Raketenturm) if artillery => -15.0,
                Seen::Structure(BuildingKind::Raffinerie) => -10.0,
                Seen::Structure(_) | Seen::Hq => 0.0,
                Seen::Unit(_) => return None,
            };
            Some((s.pos, s.pos.distance(ENEMY_HQ_POS) + bonus))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(PLAYER_HQ_POS, |t| t.0)
}

/// Taktik im Gefecht: Rückzug, Ausweichen, Zielwahl
#[allow(clippy::type_complexity)]
pub fn ai_tactics(
    mut commands: Commands,
    time: Res<Time>,
    mut ai: ResMut<AiState>,
    snap: Res<Snapshot>,
    units: Query<(Entity, &Transform, &Team, &Unit, &Health, &Weapon, Option<&AttackTarget>, Has<AiRetreat>, Option<&AiKite>, Option<&AiRole>)>,
    info: Query<(&Health, Option<&Unit>, Option<&Structure>, Has<Weapon>)>,
) {
    ai.tactic -= time.delta_secs();
    if ai.tactic > 0.0 {
        return;
    }
    ai.tactic = 0.25;
    let now = ai.elapsed;
    for (e, t, team, unit, h, w, atk, retreating, kite, role) in &units {
        if *team != ENEMY || role == Some(&AiRole::Scout) {
            continue;
        }
        let me = flat(t.translation);
        let frac = h.hp / h.max;
        // schwer beschädigt: zur Reparatur ans HQ
        if !retreating && frac < 0.3 {
            let home = ENEMY_HQ_POS + to_player() * 7.0 + jitter(e);
            commands.entity(e).remove::<(AttackTarget, AttackMove)>().insert((AiRetreat, MoveTo(home), AiRole::Reserve));
            if let Some(wv) = &mut ai.wave {
                wv.members.retain(|m| *m != e);
            }
            continue;
        }
        if retreating {
            if frac > 0.85 {
                commands.entity(e).remove::<AiRetreat>();
            }
            continue;
        }
        if let Some(k) = kite {
            if now < k.0 {
                continue;
            }
            commands.entity(e).remove::<AiKite>();
        }
        // Haubitze: Abstand zu bewaffneten Gegnern halten
        if unit.kind == UnitKind::Haubitze {
            let close = snap
                .iter()
                .filter(|(oe, o)| o.team != ENEMY && !o.is_building && info.get(**oe).is_ok_and(|i| i.3))
                .map(|(_, o)| flat(o.pos))
                .filter(|p| p.distance(me) < KITE)
                .min_by(|a, b| a.distance(me).total_cmp(&b.distance(me)));
            if let Some(p) = close {
                let away = (me - p).normalize_or(-to_player());
                commands.entity(e).remove::<(AttackTarget, AttackMove)>().insert((MoveTo(clamp_map(me + away * 9.0)), AiKite(now + 1.6)));
                continue;
            }
        }
        // Zielwahl: Abstand, Zustand des Ziels und was die eigene Waffe am besten kann
        let mut best: Option<(Entity, f32)> = None;
        let mut current: Option<f32> = None;
        for (oe, o) in snap.iter() {
            if o.team == ENEMY {
                continue;
            }
            let d = flat(o.pos).distance(me) - o.radius;
            if d > w.range + 2.0 {
                continue;
            }
            let Ok((oh, ou, os, oarmed)) = info.get(*oe) else { continue };
            let mut score = d / w.range + (oh.hp / oh.max) * 0.8;
            let target_kind = ou.map(|u| u.kind);
            score += match (unit.kind, target_kind, os.map(|s| s.0)) {
                (UnitKind::Haubitze, None, Some(BuildingKind::Geschuetzturm | BuildingKind::Raketenturm)) => -0.9,
                (UnitKind::Haubitze, None, _) => -0.3,
                (_, None, Some(BuildingKind::Geschuetzturm | BuildingKind::Raketenturm)) => 0.2,
                (_, None, _) => 0.7,
                (UnitKind::Werfer, Some(UnitKind::Panzer | UnitKind::Bollwerk | UnitKind::Haubitze), _) => -0.3,
                (UnitKind::Spaeher, Some(UnitKind::Werfer | UnitKind::Haubitze | UnitKind::Sammler), _) => -0.4,
                (UnitKind::Panzer | UnitKind::Bollwerk, Some(UnitKind::Sammler), _) => 0.2,
                _ => 0.0,
            };
            // gefährliche Artillerie zuerst ausschalten
            if matches!(target_kind, Some(UnitKind::Werfer | UnitKind::Haubitze)) && oarmed {
                score -= 0.15;
            }
            if atk.is_some_and(|a| a.0 == *oe) {
                current = Some(score);
            }
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((*oe, score));
            }
        }
        if let Some((target, score)) = best {
            // nur wechseln, wenn es sich deutlich lohnt (kein Hin und Her)
            if current.is_none_or(|c| score < c - 0.25) && atk.is_none_or(|a| a.0 != target) {
                commands.entity(e).insert(AttackTarget(target));
            }
        }
    }
}
