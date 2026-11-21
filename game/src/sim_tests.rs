//! Simulationstests ohne Grafik: Spiellogik über simulierte Zeit laufen lassen
//! und prüfen, ob das Ergebnis stimmt.

use crate::ai::{enemy_ai, AiState};
use crate::components::*;
use crate::construction::{can_place, construct, place_buildings, PlaceRequest, Site};
use crate::fx::FxEvent;
use crate::models::ModelKit;
use crate::terrain::{self, FIELDS};
use crate::units::{enqueue_builds, harvest, movement, separation, snapshot, BuildRequest, Snapshot};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

/// Minimal-App mit fester Schrittweite (0,1 s pro Update)
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
        .insert_resource(Rng(0x2545_F491_4F6C_DD1D))
        .init_resource::<Outcome>()
        .init_resource::<Snapshot>()
        .init_resource::<AiState>()
        .add_message::<BuildRequest>()
        .add_message::<PlaceRequest>()
        .add_message::<FxEvent>();
    let mut meshes = Assets::<Mesh>::default();
    let mut mats = Assets::<StandardMaterial>::default();
    app.insert_resource(ModelKit::new(&mut meshes, &mut mats));
    app
}

fn spawn_hq(app: &mut App, team: Team, pos: Vec3) {
    app.world_mut().spawn((
        Transform::from_translation(terrain::on_ground(pos)),
        team,
        Health { hp: 3000.0, max: 3000.0 },
        Building { radius: 4.8 },
        Hq { queue: Vec::new(), progress: 0.0 },
        Depot,
    ));
}

fn spawn_fields(app: &mut App) {
    for (pos, kind, amount) in FIELDS {
        app.world_mut().spawn((Transform::from_translation(terrain::on_ground(pos)), ResourceNode { kind, amount, max: amount }));
    }
}

/// Ein Sammler mit voller Ladung fährt zur nahen Raffinerie statt zum weit entfernten HQ und lädt dort ab
#[test]
fn sammler_unloads_at_refinery() {
    let mut app = app();
    app.insert_resource(Stock([[0, 0], [0, 0]]));
    app.add_systems(Update, (snapshot, movement, harvest).chain());
    spawn_hq(&mut app, PLAYER, PLAYER_HQ_POS);
    spawn_fields(&mut app);

    // Raffinerie am Ferritfeld (-15, 15), gut 40 m vom HQ entfernt
    let refinery_pos = Vec3::new(-21.0, 0.0, 21.0);
    app.world_mut().spawn((
        Transform::from_translation(terrain::on_ground(refinery_pos)),
        PLAYER,
        Health { hp: 900.0, max: 900.0 },
        Building { radius: BuildingKind::Raffinerie.radius() },
        Structure(BuildingKind::Raffinerie),
        Depot,
    ));
    let start = Vec3::new(-17.0, 0.0, 17.0);
    let sammler = app
        .world_mut()
        .spawn((
            Transform::from_translation(terrain::on_ground(start)),
            PLAYER,
            Health { hp: 160.0, max: 160.0 },
            Unit { kind: UnitKind::Sammler, speed: 7.0, radius: 1.3 },
            Heading(0.0),
            Harvester { state: HarvestState::Returning, node: None, cargo: HARVEST_LOAD, cargo_kind: ResKind::Ferrit },
        ))
        .id();

    app.update();
    let target = app.world().get::<MoveTo>(sammler).expect("Sammler fährt los").0;
    assert!(
        flat(target).distance(flat(refinery_pos)) < 0.5,
        "Ziel sollte die Raffinerie sein, ist aber {target}"
    );

    // Höchstens 10 s Spielzeit bis zum Abladen
    for _ in 0..100 {
        app.update();
        if app.world().resource::<Stock>().0[0][0] > 0 {
            break;
        }
    }
    assert_eq!(app.world().resource::<Stock>().0[0][0], HARVEST_LOAD, "Ladung kam nicht an");
    let pos = app.world().get::<Transform>(sammler).unwrap().translation;
    assert!(flat(pos).distance(flat(refinery_pos)) < 8.0, "Sammler hat nicht an der Raffinerie abgeladen ({pos})");
}

/// Eine Raffinerie im Bau nimmt noch nichts an: dann geht es zum HQ
#[test]
fn unfinished_refinery_is_ignored() {
    let mut app = app();
    app.insert_resource(Stock([[0, 0], [0, 0]]));
    app.add_systems(Update, harvest);
    spawn_hq(&mut app, PLAYER, PLAYER_HQ_POS);
    app.world_mut().spawn((
        Transform::from_translation(Vec3::new(-21.0, 0.0, 21.0)),
        PLAYER,
        Building { radius: 3.3 },
        Structure(BuildingKind::Raffinerie),
        Depot,
        Construction { progress: 0.5, dust: 0.0 },
    ));
    let sammler = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::new(-17.0, 0.0, 17.0)),
            PLAYER,
            Harvester { state: HarvestState::Returning, node: None, cargo: HARVEST_LOAD, cargo_kind: ResKind::Ferrit },
        ))
        .id();
    app.update();
    let target = app.world().get::<MoveTo>(sammler).expect("Sammler fährt los").0;
    assert!(flat(target).distance(flat(PLAYER_HQ_POS)) < 0.5, "Ziel sollte das HQ sein, ist aber {target}");
}

/// Die Gegner-KI errichtet im Lauf von gut fünf Minuten Raffinerie, Geschütztürme, Fabrik und
/// einen gesicherten Außenposten an einem weiteren Feld, alles an gültigen Plätzen und fertig
#[test]
fn ai_builds_its_base() {
    let mut app = app();
    app.insert_resource(Stock([[0, 0], [5000, 5000]]));
    app.add_systems(Update, (enemy_ai, enqueue_builds, place_buildings, construct).chain());
    spawn_hq(&mut app, ENEMY, ENEMY_HQ_POS);
    spawn_hq(&mut app, PLAYER, PLAYER_HQ_POS);
    spawn_fields(&mut app);

    for _ in 0..3200 {
        app.update();
    }

    let world = app.world_mut();
    let mut q = world.query::<(&Team, &Structure, &Transform, Option<&Construction>)>();
    let built: Vec<(BuildingKind, Vec3, bool)> = q
        .iter(world)
        .filter(|(t, ..)| **t == ENEMY)
        .map(|(_, s, t, c)| (s.0, t.translation, c.is_none()))
        .collect();
    let count = |k: BuildingKind| built.iter().filter(|b| b.0 == k).count();
    assert_eq!(count(BuildingKind::Raffinerie), 2, "Raffinerien: {built:?}");
    assert!(count(BuildingKind::Geschuetzturm) >= 2, "Geschütztürme: {built:?}");
    assert_eq!(count(BuildingKind::Fabrik), 1, "Fabrik: {built:?}");
    assert!(built.iter().all(|b| b.2), "nicht alles fertig: {built:?}");

    // Jedes Gebäude stünde auch ohne sich selbst gültig (Abstand, Reichweite, Boden)
    let mut sq = world.query::<(&Transform, &Building, &Team)>();
    let sites: Vec<(Vec3, Site)> = sq
        .iter(world)
        .map(|(t, b, team)| (t.translation, Site { pos: t.translation, radius: b.radius, team: *team, wall: false, gate: false }))
        .collect();
    let nodes: Vec<Vec3> = FIELDS.iter().map(|f| f.0).collect();
    for (kind, pos, _) in &built {
        let others: Vec<Site> = sites.iter().filter(|(p, _)| p.distance(*pos) > 0.01).map(|(_, s)| *s).collect();
        assert!(can_place(*kind, *pos, ENEMY, &others, &nodes), "{kind:?} bei {pos} steht ungültig");
    }
    // Die Raffinerien liegen näher an einem Feld als das HQ
    let nearest = |p: Vec3| nodes.iter().map(|n| flat(*n).distance(flat(p))).fold(f32::MAX, f32::min);
    for r in built.iter().filter(|b| b.0 == BuildingKind::Raffinerie) {
        assert!(nearest(r.1) < nearest(ENEMY_HQ_POS), "Raffinerie bei {} liegt nicht näher am Feld", r.1);
    }
    // Der Außenposten weit vor dem HQ hat einen Geschützturm in der Nähe
    let outpost = built.iter().find(|b| b.0 == BuildingKind::Raffinerie && b.1.distance(ENEMY_HQ_POS) > 30.0).expect("kein Außenposten").1;
    assert!(
        built.iter().any(|b| b.0 == BuildingKind::Geschuetzturm && b.1.distance(outpost) < 14.0),
        "Außenposten ohne Geschützturm: {built:?}"
    );
}

/// Ein zerstörtes HQ zählt sofort als verloren, das Modell stürzt danach ein
/// und hinterlässt einen Trümmerhaufen
#[test]
fn destroyed_building_collapses_into_rubble() {
    use crate::fx::{update_collapse, Collapse, FxKit, Wreck};
    use crate::units::{check_outcome, deaths};
    let mut app = app();
    let mut meshes = Assets::<Mesh>::default();
    let mut mats = Assets::<StandardMaterial>::default();
    let mut images = Assets::<Image>::default();
    app.insert_resource(FxKit::new(&mut meshes, &mut mats, &mut images));
    app.add_systems(Update, (deaths, check_outcome, update_collapse).chain());
    spawn_hq(&mut app, PLAYER, PLAYER_HQ_POS);
    spawn_hq(&mut app, ENEMY, ENEMY_HQ_POS);
    let mut q = app.world_mut().query::<(&Team, &mut Health)>();
    for (team, mut h) in q.iter_mut(app.world_mut()) {
        if *team == PLAYER {
            h.hp = 0.0;
        }
    }

    app.update();
    assert_eq!(*app.world().resource::<Outcome>(), Outcome::Lost, "HQ-Verlust nicht erkannt");
    let world = app.world_mut();
    assert_eq!(world.query::<&Collapse>().iter(world).count(), 1, "kein Einsturz gestartet");

    // Einsturz dauert knapp drei Sekunden; die Animation rechnet mit höchstens 0,05 s je Schritt
    for _ in 0..80 {
        app.update();
    }
    let world = app.world_mut();
    assert_eq!(world.query::<&Collapse>().iter(world).count(), 0, "Einsturz endet nicht");
    assert_eq!(world.query::<&Wreck>().iter(world).count(), 1, "kein Trümmerhaufen");
}

// ---------------------------------------------------------------- Gegner-KI

use crate::ai::{ai_tactics, choose_unit, AiRetreat, AiRole, Seen, WavePhase};
use crate::units::repair;

/// Einheit ohne Modell, nur mit den Spielkomponenten
fn unit(app: &mut App, team: Team, kind: UnitKind, pos: Vec3, hp_frac: f32) -> Entity {
    let (hp, speed, radius, range) = match kind {
        UnitKind::Sammler => (160.0, 7.0, 1.3, 0.0),
        UnitKind::Panzer => (220.0, 6.0, 1.4, 11.0),
        UnitKind::Werfer => (140.0, 4.8, 1.3, 19.0),
        UnitKind::Spaeher => (110.0, 10.0, 1.3, 12.0),
        UnitKind::Bollwerk => (520.0, 4.0, 1.9, 13.0),
        UnitKind::Haubitze => (150.0, 4.2, 1.5, 27.0),
    };
    let mut e = app.world_mut().spawn((
        Transform::from_translation(terrain::on_ground(pos)),
        team,
        Health { hp: hp * hp_frac, max: hp },
        Unit { kind, speed, radius },
        Heading(0.0),
    ));
    if kind == UnitKind::Sammler {
        e.insert(Harvester { state: HarvestState::Idle, node: None, cargo: 0, cargo_kind: ResKind::Ferrit });
    } else {
        e.insert(Weapon { range, damage: 10.0, cooldown: 1.0, timer: 0.0, projectile_speed: 30.0, rocket: false, splash: 0.0, arc: 0.0 });
    }
    e.id()
}

fn pos_of(app: &App, e: Entity) -> Vec3 {
    app.world().get::<Transform>(e).unwrap().translation
}

/// Konter: gegen Geschütztürme Artillerie, gegen viel Panzerung Raketen und schwere Panzer
#[test]
fn ai_counters_what_it_sees() {
    let none = [0.0f32; 6];
    let mut army = [0usize; 6];
    army[UnitKind::Panzer.idx()] = 3;
    army[UnitKind::Werfer.idx()] = 2;
    army[UnitKind::Spaeher.idx()] = 1;
    assert_eq!(choose_unit(&army, &none, 3, true), UnitKind::Haubitze, "Türme mit Fabrik");
    assert_eq!(choose_unit(&army, &none, 3, false), UnitKind::Werfer, "Türme ohne Fabrik");
    let mut armor = [0.0f32; 6];
    armor[UnitKind::Bollwerk.idx()] = 4.0;
    armor[UnitKind::Panzer.idx()] = 4.0;
    let pick = choose_unit(&army, &armor, 0, true);
    assert!(matches!(pick, UnitKind::Bollwerk | UnitKind::Werfer), "gegen Panzerung: {pick:?}");
    let mut arty = [0.0f32; 6];
    arty[UnitKind::Haubitze.idx()] = 5.0;
    let mut no_scouts = army;
    no_scouts[UnitKind::Spaeher.idx()] = 0;
    assert_eq!(choose_unit(&no_scouts, &arty, 0, false), UnitKind::Spaeher, "gegen Artillerie");
}

/// Sammler fliehen vor bewaffneten Gegnern zur Abladestelle
#[test]
fn harvester_flees_from_attackers() {
    let mut app = app();
    app.insert_resource(Stock([[0, 0], [0, 0]]));
    app.add_systems(Update, (snapshot, enemy_ai, movement).chain());
    spawn_hq(&mut app, ENEMY, ENEMY_HQ_POS);
    let field = Vec3::new(28.0, 0.0, -50.0);
    let sammler = unit(&mut app, ENEMY, UnitKind::Sammler, field + Vec3::new(2.0, 0.0, 0.0), 1.0);
    let raider = unit(&mut app, PLAYER, UnitKind::Panzer, field + Vec3::new(-6.0, 0.0, 0.0), 1.0);
    app.update();
    let target = app.world().get::<MoveTo>(sammler).expect("Sammler flieht nicht").0;
    assert!(flat(target).distance(flat(ENEMY_HQ_POS)) < 1.0, "flieht nicht zum HQ, sondern nach {target}");
    let d0 = flat(pos_of(&app, sammler)).distance(flat(pos_of(&app, raider)));
    for _ in 0..20 {
        app.update();
    }
    let d1 = flat(pos_of(&app, sammler)).distance(flat(pos_of(&app, raider)));
    assert!(d1 > d0 + 5.0, "Abstand wächst nicht: {d0} -> {d1}");
}

/// Greift der Spieler die Basis an, rückt die Reserve dorthin aus
#[test]
fn reserve_defends_base() {
    let mut app = app();
    app.insert_resource(Stock([[0, 0], [0, 0]]));
    app.add_systems(Update, (snapshot, enemy_ai).chain());
    spawn_hq(&mut app, ENEMY, ENEMY_HQ_POS);
    let rally = crate::ai::rally();
    let guards: Vec<Entity> = (0..3)
        .map(|i| {
            let e = unit(&mut app, ENEMY, UnitKind::Panzer, rally + Vec3::X * i as f32 * 3.0, 1.0);
            app.world_mut().entity_mut(e).insert(AiRole::Reserve);
            e
        })
        .collect();
    let raid = ENEMY_HQ_POS + Vec3::new(-6.0, 0.0, 18.0);
    unit(&mut app, PLAYER, UnitKind::Panzer, raid, 1.0);
    app.update();
    assert!(app.world().resource::<AiState>().threat.is_some(), "Bedrohung nicht erkannt");
    for g in guards {
        assert_eq!(app.world().get::<AiRole>(g), Some(&AiRole::Defend));
        let to = app.world().get::<MoveTo>(g).expect("Verteidiger ohne Marschbefehl").0;
        assert!(flat(to).distance(flat(raid)) < 6.0, "Verteidiger fährt nach {to} statt zum Angreifer");
        assert!(app.world().get::<AttackMove>(g).is_some(), "Verteidiger fährt ohne Angriffsmarsch");
    }
}

/// Eine Welle sammelt sich, rückt geschlossen vor und zieht sich zurück, wenn sie zu schwach wird
#[test]
fn wave_gathers_advances_and_retreats() {
    let mut app = app();
    app.insert_resource(Stock([[0, 0], [0, 0]]));
    app.add_systems(Update, (snapshot, enemy_ai, movement, separation).chain());
    spawn_hq(&mut app, ENEMY, ENEMY_HQ_POS);
    spawn_hq(&mut app, PLAYER, PLAYER_HQ_POS);
    app.world_mut().resource_mut::<AiState>().next_attack = 0.0;
    let rally = crate::ai::rally();
    let kinds = [UnitKind::Panzer, UnitKind::Panzer, UnitKind::Werfer, UnitKind::Bollwerk, UnitKind::Spaeher];
    let army: Vec<Entity> = kinds
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let e = unit(&mut app, ENEMY, *k, rally + Vec3::new(i as f32 * 2.5 - 5.0, 0.0, 0.0), 1.0);
            // der Späher ist schon Reserve, sonst würde er Kundschafter
            app.world_mut().entity_mut(e).insert(AiRole::Reserve);
            e
        })
        .collect();
    app.update();
    {
        let ai = app.world().resource::<AiState>();
        let w = ai.wave.as_ref().expect("keine Welle gebildet");
        assert_eq!(w.members.len(), 5);
        assert_eq!(w.phase, WavePhase::Gather);
    }
    for _ in 0..300 {
        app.update();
    }
    let ai = app.world().resource::<AiState>();
    assert_eq!(ai.wave.as_ref().map(|w| w.phase), Some(WavePhase::Advance), "Welle rückt nicht vor");
    let pts: Vec<Vec3> = army.iter().map(|e| flat(pos_of(&app, *e))).collect();
    let centroid = pts.iter().sum::<Vec3>() / pts.len() as f32;
    assert!(centroid.distance(flat(ENEMY_HQ_POS)) > flat(rally).distance(flat(ENEMY_HQ_POS)) + 15.0, "Welle kommt nicht voran: {centroid}");
    let spread = pts.iter().map(|p| p.distance(centroid)).fold(0.0, f32::max);
    assert!(spread < 14.0, "Welle zerfasert: größter Abstand {spread}");

    // schwere Verluste: Rückzug
    for e in &army[..4] {
        app.world_mut().get_mut::<Health>(*e).unwrap().hp *= 0.1;
    }
    for _ in 0..10 {
        app.update();
    }
    assert!(app.world().resource::<AiState>().wave.is_none(), "Welle löst sich nicht auf");
    for e in &army {
        assert_eq!(app.world().get::<AiRole>(*e), Some(&AiRole::Reserve));
        let to = app.world().get::<MoveTo>(*e).expect("kein Rückzugsbefehl").0;
        assert!(flat(to).distance(flat(rally)) < 8.0, "Rückzug nach {to} statt zum Sammelpunkt");
    }
}

/// Schwer beschädigte Einheiten fahren zum HQ und werden dort repariert
#[test]
fn damaged_unit_retreats_and_repairs() {
    let mut app = app();
    app.add_systems(Update, (snapshot, ai_tactics, movement, repair).chain());
    spawn_hq(&mut app, ENEMY, ENEMY_HQ_POS);
    let tank = unit(&mut app, ENEMY, UnitKind::Panzer, ENEMY_HQ_POS + Vec3::new(-25.0, 0.0, 25.0), 0.2);
    app.update();
    assert!(app.world().get::<AiRetreat>(tank).is_some(), "kein Rückzug");
    for _ in 0..500 {
        app.update();
    }
    let h = app.world().get::<Health>(tank).unwrap();
    assert!(h.hp / h.max > 0.85, "nicht repariert: {}/{}", h.hp, h.max);
    assert!(app.world().get::<AiRetreat>(tank).is_none(), "bleibt im Rückzug");
    assert!(flat(pos_of(&app, tank)).distance(flat(ENEMY_HQ_POS)) < crate::units::REPAIR_RANGE);
}

/// Haubitzen weichen bewaffneten Gegnern aus, die ihnen zu nahe kommen
#[test]
fn howitzer_keeps_distance() {
    let mut app = app();
    app.add_systems(Update, (snapshot, ai_tactics, movement).chain());
    let gun = unit(&mut app, ENEMY, UnitKind::Haubitze, Vec3::new(20.0, 0.0, -20.0), 1.0);
    let raider = unit(&mut app, PLAYER, UnitKind::Spaeher, Vec3::new(14.0, 0.0, -16.0), 1.0);
    let d0 = flat(pos_of(&app, gun)).distance(flat(pos_of(&app, raider)));
    for _ in 0..25 {
        app.update();
    }
    let d1 = flat(pos_of(&app, gun)).distance(flat(pos_of(&app, raider)));
    assert!(d1 > d0 + 3.0 && d1 > 9.0, "Haubitze weicht nicht aus: {d0} -> {d1}");
}

/// Der Späher erkundet die Karte, und die KI merkt sich, was er sieht
#[test]
fn scout_explores_and_remembers() {
    let mut app = app();
    app.insert_resource(Stock([[0, 0], [0, 0]]));
    app.add_systems(Update, (snapshot, enemy_ai, movement).chain());
    spawn_hq(&mut app, ENEMY, ENEMY_HQ_POS);
    spawn_hq(&mut app, PLAYER, PLAYER_HQ_POS);
    let scout = unit(&mut app, ENEMY, UnitKind::Spaeher, ENEMY_HQ_POS + Vec3::new(-6.0, 0.0, 6.0), 1.0);
    // ein Spielerturm am mittleren Ferritfeld, weit außer Sicht der Basis
    let turret = app
        .world_mut()
        .spawn((
            Transform::from_translation(terrain::on_ground(Vec3::new(8.0, 0.0, -8.0))),
            PLAYER,
            Health { hp: 500.0, max: 500.0 },
            Building { radius: 1.9 },
            Structure(BuildingKind::Geschuetzturm),
        ))
        .id();
    app.update();
    assert_eq!(app.world().get::<AiRole>(scout), Some(&AiRole::Scout));
    assert!(!app.world().resource::<AiState>().intel.contains_key(&turret), "Turm schon vor dem Erkunden bekannt");
    for _ in 0..900 {
        app.update();
    }
    let ai = app.world().resource::<AiState>();
    let s = ai.intel.get(&turret).expect("Späher hat den Turm nicht entdeckt");
    assert_eq!(s.what, Seen::Structure(BuildingKind::Geschuetzturm));
    assert!(ai.scout_leg >= 3, "Späher kommt nicht herum: {} Wegpunkte", ai.scout_leg);
}

/// Ganze Partie im Zeitraffer (bis 15 Minuten) gegen einen Spieler, der sich nur mit vier
/// Geschütztürmen verteidigt: Die KI baut ihre Wirtschaft aus, erkundet,
/// greift in Wellen an und setzt der Spielerbasis zu
#[test]
fn full_match_ai_builds_up_and_attacks() {
    use crate::fx::{update_collapse, FxKit};
    use crate::units::{auto_target, check_outcome, combat, deaths, production, projectiles};
    use crate::world::spawn_unit;
    let mut app = app();
    let mut meshes = Assets::<Mesh>::default();
    let mut mats = Assets::<StandardMaterial>::default();
    let mut images = Assets::<Image>::default();
    app.insert_resource(FxKit::new(&mut meshes, &mut mats, &mut images));
    app.insert_resource(Stock([[400, 150], [400, 150]]));
    app.add_systems(
        Update,
        (
            (enemy_ai, ai_tactics, enqueue_builds, place_buildings, construct).chain(),
            (snapshot, auto_target, combat, movement, separation, harvest, projectiles, production, repair, deaths, check_outcome, update_collapse).chain(),
        )
            .chain(),
    );
    spawn_hq(&mut app, PLAYER, PLAYER_HQ_POS);
    spawn_hq(&mut app, ENEMY, ENEMY_HQ_POS);
    spawn_fields(&mut app);
    app.world_mut().resource_scope(|world, kit: Mut<ModelKit>| {
        let mut c = world.commands();
        for i in 0..2 {
            let o = i as f32 * 2.8;
            spawn_unit(&mut c, &kit, PLAYER, UnitKind::Sammler, PLAYER_HQ_POS + Vec3::new(7.0 + o, 0.0, -7.0));
            spawn_unit(&mut c, &kit, ENEMY, UnitKind::Sammler, ENEMY_HQ_POS + Vec3::new(-7.0 - o, 0.0, 7.0));
        }
        for i in 0..3 {
            let o = i as f32 * 2.8;
            spawn_unit(&mut c, &kit, PLAYER, UnitKind::Panzer, PLAYER_HQ_POS + Vec3::new(9.0 + o, 0.0, -1.0));
            spawn_unit(&mut c, &kit, ENEMY, UnitKind::Panzer, ENEMY_HQ_POS + Vec3::new(-9.0 - o, 0.0, 1.0));
        }
    });
    app.world_mut().flush();
    // Verteidigungsring aus fertigen Geschütztürmen vor dem Spieler-HQ
    app.world_mut().resource_scope(|world, kit: Mut<ModelKit>| {
        let mut c = world.commands();
        for (x, z) in [(12.0, -2.0), (2.0, -12.0), (14.0, -14.0), (-4.0, -8.0)] {
            let e = crate::construction::spawn_structure(&mut c, &kit, PLAYER, BuildingKind::Geschuetzturm, PLAYER_HQ_POS + Vec3::new(x, 0.0, z), None);
            let hp = BuildingKind::Geschuetzturm.hp();
            c.entity(e).remove::<Construction>().insert(Health { hp, max: hp });
        }
    });
    app.world_mut().flush();

    let (mut waves, mut max_harv, mut was_wave) = (0, 0, false);
    let mut kinds_built = std::collections::HashSet::new();
    for step in 0..9000 {
        app.update();
        if step % 10 != 0 {
            continue;
        }
        let world = app.world_mut();
        let has_wave = world.resource::<AiState>().wave.is_some();
        if has_wave && !was_wave {
            waves += 1;
        }
        was_wave = has_wave;
        let mut q = world.query::<(&Team, &Unit)>();
        let harv = q.iter(world).filter(|(t, u)| **t == ENEMY && u.kind == UnitKind::Sammler).count();
        max_harv = max_harv.max(harv);
        for (t, u) in q.iter(world) {
            if *t == ENEMY {
                kinds_built.insert(u.kind);
            }
        }
        if *world.resource::<Outcome>() != Outcome::Running {
            break;
        }
    }
    let world = app.world_mut();
    let outcome = *world.resource::<Outcome>();
    let mut hq = world.query::<(&Team, &Health, &Hq)>();
    let player_hq = hq.iter(world).find(|(t, ..)| **t == PLAYER).map(|(_, h, _)| h.hp / h.max);
    let minutes = world.resource::<AiState>().elapsed / 60.0;
    println!("Nach {minutes:.1} min: Wellen {waves}, Sammler max {max_harv}, Typen {kinds_built:?}, Ausgang {outcome:?}, Spieler-HQ {player_hq:?}");
    assert!(max_harv >= 5, "Wirtschaft wächst nicht: höchstens {max_harv} Sammler");
    assert!(waves >= 1, "keine Angriffswelle");
    assert!(kinds_built.contains(&UnitKind::Spaeher), "kein Späher gebaut");
    assert!(
        outcome == Outcome::Lost || player_hq.is_some_and(|f| f < 0.8),
        "Angriffe zeigen keine Wirkung: Ausgang {outcome:?}, Spieler-HQ {player_hq:?}"
    );
}

// ---------------------------------------------------------------------------
// Steuerung: Formation, Befehlsschlange, Patrouille, Stellung halten, Sammelpunkt
// ---------------------------------------------------------------------------

use crate::orders::*;

/// Befehle wie aus der Eingabe absetzen
fn order(app: &mut App, f: impl FnOnce(&mut Commands)) {
    let world = app.world_mut();
    let mut c = world.commands();
    f(&mut c);
    world.flush();
}

fn member(app: &App, e: Entity) -> Member {
    let u = app.world().get::<Unit>(e).unwrap();
    Member {
        e,
        pos: pos_of(app, e),
        speed: u.speed,
        radius: u.radius,
        armed: app.world().get::<Weapon>(e).is_some(),
        harvester: app.world().get::<Harvester>(e).is_some(),
    }
}

/// Formation: genug Abstand, mittig um das Ziel; Frontlinie über die ganze Breite; Wege kreuzen sich nicht
#[test]
fn formation_slots_line_up_without_crossing() {
    let slots = formation_slots(6, Vec3::ZERO, Vec3::Z, None, 3.0, false);
    assert_eq!(slots.len(), 6);
    for i in 0..6 {
        for j in i + 1..6 {
            assert!(slots[i].distance(slots[j]) >= 2.99, "Plätze zu eng: {:?}", slots);
        }
    }
    let mean = slots.iter().sum::<Vec3>() / 6.0;
    assert!(mean.length() < 0.01, "Formation nicht mittig: {mean}");

    // 4 Einheiten auf einer 12-m-Linie: eine Reihe von Ende zu Ende
    let line = formation_slots(4, Vec3::ZERO, Vec3::Z, Some(12.0), 3.0, true);
    assert!(line.iter().all(|s| s.z.abs() < 0.01), "nicht auf der Linie: {line:?}");
    let max_x = line.iter().map(|s| s.x.abs()).fold(0.0, f32::max);
    assert!((max_x - 6.0).abs() < 0.01, "Linie nicht voll genutzt: {line:?}");

    // Einheiten nebeneinander: Reihenfolge quer zur Fahrtrichtung bleibt erhalten
    let units: Vec<Vec3> = [5.0, -3.0, 1.0, -7.0].iter().map(|x| Vec3::new(*x, 0.0, -20.0)).collect();
    let a = assign_slots(&units, &line, Vec3::Z);
    let mut used = a.clone();
    used.sort();
    assert_eq!(used, vec![0, 1, 2, 3], "keine Eins-zu-eins-Zuordnung");
    for i in 0..4 {
        for j in 0..4 {
            if units[i].x < units[j].x {
                assert!(line[a[i]].x < line[a[j]].x, "Wege kreuzen sich");
            }
        }
    }
}

/// Shift+Rechtsklick: Ziele werden nacheinander angefahren
#[test]
fn shift_queue_follows_waypoints() {
    let mut app = app();
    app.add_systems(Update, (snapshot, follow_orders, movement).chain());
    let tank = unit(&mut app, PLAYER, UnitKind::Panzer, Vec3::ZERO, 1.0);
    let a = Vec3::new(12.0, 0.0, 0.0);
    let b = Vec3::new(12.0, 0.0, 12.0);
    let m = member(&app, tank);
    order(&mut app, |c| {
        issue_group_move(c, &[m], a, None, MoveKind::Move, false);
        issue_group_move(c, &[m], b, None, MoveKind::Move, true);
    });
    assert_eq!(app.world().get::<Waypoints>(tank).map(|w| w.0.len()), Some(1));
    let mut near_a = f32::MAX;
    for _ in 0..120 {
        app.update();
        near_a = near_a.min(flat(pos_of(&app, tank)).distance(a));
    }
    assert!(near_a < 1.0, "erstes Ziel nicht erreicht: {near_a}");
    assert!(flat(pos_of(&app, tank)).distance(b) < 1.0, "zweites Ziel nicht erreicht");
    assert!(app.world().get::<Waypoints>(tank).is_none(), "Befehlsschlange nicht abgearbeitet");
}

/// Patrouille: pendelt zwischen Start und Ziel
#[test]
fn patrol_goes_back_and_forth() {
    let mut app = app();
    app.add_systems(Update, (snapshot, follow_orders, movement).chain());
    let tank = unit(&mut app, PLAYER, UnitKind::Panzer, Vec3::ZERO, 1.0);
    let b = Vec3::new(15.0, 0.0, 0.0);
    let m = member(&app, tank);
    order(&mut app, |c| issue_group_move(c, &[m], b, None, MoveKind::Patrol, false));
    let (mut at_b, mut back_at_a) = (false, false);
    for _ in 0..200 {
        app.update();
        let p = flat(pos_of(&app, tank));
        if p.distance(b) < 1.5 {
            at_b = true;
        }
        if at_b && p.length() < 1.5 {
            back_at_a = true;
        }
    }
    assert!(at_b && back_at_a, "Patrouille pendelt nicht (Ziel {at_b}, zurück {back_at_a})");
    assert!(app.world().get::<AttackMove>(tank).is_some(), "Patrouille greift unterwegs nicht an");
}

/// Stellung halten: kein Nachfahren auf Gegner außer Reichweite; ohne Halten fährt die Einheit hin
#[test]
fn hold_position_does_not_chase() {
    let mut app = app();
    app.add_systems(Update, (snapshot, crate::units::auto_target, movement).chain());
    let holder = unit(&mut app, PLAYER, UnitKind::Panzer, Vec3::ZERO, 1.0);
    app.world_mut().entity_mut(holder).insert(HoldPosition);
    unit(&mut app, ENEMY, UnitKind::Panzer, Vec3::new(14.0, 0.0, 0.0), 1.0);
    let free = unit(&mut app, PLAYER, UnitKind::Panzer, Vec3::new(0.0, 0.0, 40.0), 1.0);
    unit(&mut app, ENEMY, UnitKind::Panzer, Vec3::new(14.0, 0.0, 40.0), 1.0);
    let (h0, f0) = (pos_of(&app, holder), pos_of(&app, free));
    for _ in 0..30 {
        app.update();
    }
    assert!(flat(pos_of(&app, holder) - h0).length() < 0.1, "haltende Einheit ist losgefahren");
    assert!(flat(pos_of(&app, free) - f0).length() > 1.0, "freie Einheit greift nicht an");
}

/// Gruppen fahren im Tempo der langsamsten Einheit und drehen sich am Ziel in Blickrichtung
#[test]
fn group_moves_at_slowest_speed_and_faces_forward() {
    let mut app = app();
    app.add_systems(Update, (snapshot, follow_orders, movement, face_on_arrival).chain());
    let fast = unit(&mut app, PLAYER, UnitKind::Spaeher, Vec3::new(-2.0, 0.0, 0.0), 1.0);
    let slow = unit(&mut app, PLAYER, UnitKind::Bollwerk, Vec3::new(2.0, 0.0, 0.0), 1.0);
    let goal = Vec3::new(0.0, 0.0, -40.0);
    let ms = [member(&app, fast), member(&app, slow)];
    order(&mut app, |c| issue_group_move(c, &ms, goal, None, MoveKind::Move, false));
    for _ in 0..40 {
        app.update();
    }
    let (df, ds) = (flat(pos_of(&app, fast)).distance(goal), flat(pos_of(&app, slow)).distance(goal));
    assert!((df - ds).abs() < 2.0, "Gruppe zieht sich auseinander: {df} / {ds}");
    for _ in 0..200 {
        app.update();
    }
    for e in [fast, slow] {
        assert!(flat(pos_of(&app, e)).distance(goal) < 4.0, "Ziel nicht erreicht");
        assert!(app.world().get::<GroupSpeed>(e).is_none(), "Gruppentempo bleibt nach Ankunft");
        let h = app.world().get::<Heading>(e).unwrap().0;
        let want = Heading::from_dir(Vec3::NEG_Z).0;
        let diff = (h - want + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        assert!(diff.abs() < 0.1, "nicht in Marschrichtung ausgerichtet: {diff}");
    }
}

/// Stopp hebt alle Befehle auf
#[test]
fn stop_clears_all_orders() {
    let mut app = app();
    let tank = unit(&mut app, PLAYER, UnitKind::Panzer, Vec3::ZERO, 1.0);
    let m = member(&app, tank);
    order(&mut app, |c| {
        issue_group_move(c, &[m], Vec3::new(10.0, 0.0, 0.0), None, MoveKind::Patrol, false);
        issue_group_move(c, &[m], Vec3::new(20.0, 0.0, 0.0), None, MoveKind::Move, true);
    });
    assert!(app.world().get::<Patrol>(tank).is_some() && app.world().get::<Waypoints>(tank).is_some());
    order(&mut app, |c| clear_orders(&mut c.entity(tank)));
    let w = app.world();
    assert!(w.get::<MoveTo>(tank).is_none() && w.get::<Patrol>(tank).is_none() && w.get::<Waypoints>(tank).is_none() && w.get::<AttackMove>(tank).is_none());
}

/// Sammelpunkt: neue Kampfeinheiten fahren hin, neue Sammler bauen am Sammelfeld ab
#[test]
fn rally_point_directs_new_units() {
    let mut app = app();
    app.add_systems(Update, crate::units::production);
    spawn_hq(&mut app, PLAYER, PLAYER_HQ_POS);
    spawn_fields(&mut app);
    let node = app.world_mut().query_filtered::<Entity, With<ResourceNode>>().iter(app.world()).next().unwrap();
    let rally = Vec3::new(-30.0, 0.0, 30.0);
    let hq = app.world_mut().query_filtered::<Entity, With<Hq>>().iter(app.world()).next().unwrap();
    app.world_mut().entity_mut(hq).insert(Rally { pos: rally, node: None });
    {
        let mut h = app.world_mut().get_mut::<Hq>(hq).unwrap();
        h.queue = vec![UnitKind::Panzer];
        h.progress = 1000.0;
    }
    app.update();
    let mut q = app.world_mut().query::<(&Unit, &MoveTo)>();
    let (u, mv) = q.iter(app.world()).next().expect("keine Einheit gebaut");
    assert_eq!(u.kind, UnitKind::Panzer);
    assert!(mv.0.distance(rally) < 0.01, "fährt nicht zum Sammelpunkt");

    app.world_mut().entity_mut(hq).insert(Rally { pos: rally, node: Some(node) });
    {
        let mut h = app.world_mut().get_mut::<Hq>(hq).unwrap();
        h.queue = vec![UnitKind::Sammler];
        h.progress = 1000.0;
    }
    app.update();
    let mut q = app.world_mut().query::<&Harvester>();
    let h = q.iter(app.world()).next().expect("kein Sammler gebaut");
    assert_eq!(h.node, Some(node));
    assert_eq!(h.state, HarvestState::ToNode);
}

/// Angriffsmeldungen: nahe Treffer zählen als ein Angriff, die Leertaste springt nur zu frischen
#[test]
fn alerts_merge_and_expire() {
    let mut a = Alerts::default();
    a.hit(Vec3::new(0.0, 0.0, 0.0));
    a.hit(Vec3::new(3.0, 0.0, 0.0));
    assert_eq!(a.pings.len(), 1);
    a.hit(Vec3::new(40.0, 0.0, 0.0));
    assert_eq!(a.pings.len(), 2);
    assert_eq!(a.recent(), Some(Vec3::new(40.0, 0.0, 0.0)));
    a.tick(ALERT_SHOW + 1.0);
    assert!(a.recent().is_none());
    assert!(a.pings.is_empty());
}

/// Mauerstücke von `a` nach `b` direkt in die Welt setzen (fertig gebaut)
fn spawn_wall_line(app: &mut App, team: Team, a: Vec3, b: Vec3) -> Vec<Entity> {
    let (segs, _) = crate::construction::wall_segments(a, b);
    segs.into_iter()
        .map(|p| {
            app.world_mut()
                .spawn((
                    Transform::from_translation(terrain::on_ground(p)),
                    team,
                    Health { hp: 450.0, max: 450.0 },
                    Building { radius: BuildingKind::Mauer.radius() },
                    Structure(BuildingKind::Mauer),
                    Wall,
                ))
                .id()
        })
        .collect()
}

fn nav_app() -> App {
    let mut app = app();
    app.init_resource::<crate::nav::NavGrid>()
        .init_resource::<crate::nav::PathQueue>()
        .add_systems(Update, (snapshot, crate::nav::update_grid, crate::nav::plan_paths, movement, separation).chain());
    app
}

/// Ein Panzer fährt um eine Mauer herum statt hindurch und kommt am Ziel an
#[test]
fn unit_drives_around_a_wall() {
    let mut app = nav_app();
    spawn_wall_line(&mut app, ENEMY, Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 0.0, 10.0));
    let tank = app
        .world_mut()
        .spawn((
            Transform::from_translation(terrain::on_ground(Vec3::new(-8.0, 0.0, 0.0))),
            PLAYER,
            Health { hp: 220.0, max: 220.0 },
            Unit { kind: UnitKind::Panzer, speed: 6.0, radius: 1.4 },
            Heading(0.0),
            MoveTo(Vec3::new(8.0, 0.0, 0.0)),
        ))
        .id();
    let mut closest_to_wall = f32::MAX;
    for _ in 0..300 {
        app.update();
        let p = app.world().get::<Transform>(tank).expect("Panzer").translation;
        if p.z.abs() < 10.0 {
            closest_to_wall = closest_to_wall.min(p.x.abs());
        }
    }
    let p = app.world().get::<Transform>(tank).expect("Panzer").translation;
    assert!(flat(p).distance(Vec3::new(8.0, 0.0, 0.0)) < 1.5, "nicht angekommen: {p}");
    assert!(closest_to_wall > 1.2, "durch die Mauer gefahren (Abstand {closest_to_wall})");
}

/// Ein Ziel hinter einer geschlossenen Mauer: die Einheit im Angriffsmarsch nimmt ein Mauerstück ins Visier
#[test]
fn attack_move_breaches_closed_wall() {
    let mut app = nav_app();
    let center = Vec3::new(10.0, 0.0, 0.0);
    let mut ring = Vec::new();
    for k in 0..4 {
        let corners = [Vec3::new(-7.0, 0.0, -7.0), Vec3::new(7.0, 0.0, -7.0), Vec3::new(7.0, 0.0, 7.0), Vec3::new(-7.0, 0.0, 7.0)];
        ring.extend(spawn_wall_line(&mut app, ENEMY, center + corners[k], center + corners[(k + 1) % 4]));
    }
    let tank = app
        .world_mut()
        .spawn((
            Transform::from_translation(terrain::on_ground(Vec3::new(-14.0, 0.0, 0.0))),
            PLAYER,
            Health { hp: 220.0, max: 220.0 },
            Unit { kind: UnitKind::Panzer, speed: 6.0, radius: 1.4 },
            Heading(0.0),
            Weapon { range: 11.0, damage: 18.0, cooldown: 1.1, timer: 0.0, projectile_speed: 38.0, rocket: false, splash: 0.0, arc: 0.0 },
            MoveTo(center),
            AttackMove,
        ))
        .id();
    for _ in 0..20 {
        app.update();
    }
    let target = app.world().get::<AttackTarget>(tank).map(|a| a.0);
    assert!(target.is_some_and(|t| ring.contains(&t)), "kein Mauerstück als Ziel: {target:?}");
}

/// Ein fertiger Raketenturm sucht sich selbst ein Ziel auf 22 m (außer Reichweite eines Geschützturms) und trifft
#[test]
fn rocket_tower_hits_at_long_range() {
    use crate::units::{auto_target, combat, projectiles};
    let mut app = app();
    app.add_systems(Update, (snapshot, auto_target, combat, projectiles).chain());
    let pos = Vec3::new(0.0, 0.0, 0.0);
    app.world_mut().resource_scope(|world, kit: Mut<ModelKit>| {
        let mut c = world.commands();
        let e = crate::construction::spawn_structure(&mut c, &kit, PLAYER, BuildingKind::Raketenturm, pos, None);
        let hp = BuildingKind::Raketenturm.hp();
        c.entity(e).remove::<Construction>().insert(Health { hp, max: hp });
    });
    app.world_mut().flush();
    let target = app
        .world_mut()
        .spawn((
            Transform::from_translation(terrain::on_ground(pos + Vec3::new(22.0, 0.0, 0.0))),
            ENEMY,
            Health { hp: 220.0, max: 220.0 },
            Unit { kind: UnitKind::Panzer, speed: 6.0, radius: 1.4 },
            Heading(0.0),
        ))
        .id();
    assert!(22.0 - 1.4 > 17.0, "Ziel muss außerhalb der Geschützturm-Reichweite liegen");
    for _ in 0..80 {
        app.update();
    }
    let hp = app.world().get::<Health>(target).expect("Ziel").hp;
    assert!(hp < 220.0 - 50.0, "Raketenturm hat kaum getroffen: {hp}");
}
