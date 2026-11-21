//! Simulationstests ohne Grafik: Spiellogik über simulierte Zeit laufen lassen
//! und prüfen, ob das Ergebnis stimmt.

use crate::ai::{enemy_ai, AiState};
use crate::components::*;
use crate::construction::{can_place, construct, place_buildings, PlaceRequest, Site};
use crate::fx::FxEvent;
use crate::models::ModelKit;
use crate::terrain::{self, FIELDS};
use crate::units::{enqueue_builds, harvest, movement, snapshot, BuildRequest, Snapshot};
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

/// Die Gegner-KI errichtet im Lauf von gut fünf Minuten Raffinerie, Geschütztürme und Fabrik
/// an gültigen Plätzen, und alle werden fertig
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
    assert_eq!(count(BuildingKind::Raffinerie), 1, "Raffinerie: {built:?}");
    assert!(count(BuildingKind::Geschuetzturm) >= 2, "Geschütztürme: {built:?}");
    assert_eq!(count(BuildingKind::Fabrik), 1, "Fabrik: {built:?}");
    assert!(built.iter().all(|b| b.2), "nicht alles fertig: {built:?}");

    // Jedes Gebäude stünde auch ohne sich selbst gültig (Abstand, Reichweite, Boden)
    let mut sq = world.query::<(&Transform, &Building, &Team)>();
    let sites: Vec<(Vec3, Site)> = sq
        .iter(world)
        .map(|(t, b, team)| (t.translation, Site { pos: t.translation, radius: b.radius, team: *team }))
        .collect();
    let nodes: Vec<Vec3> = FIELDS.iter().map(|f| f.0).collect();
    for (kind, pos, _) in &built {
        let others: Vec<Site> = sites.iter().filter(|(p, _)| p.distance(*pos) > 0.01).map(|(_, s)| *s).collect();
        assert!(can_place(*kind, *pos, ENEMY, &others, &nodes), "{kind:?} bei {pos} steht ungültig");
    }
    // Die Raffinerie liegt näher am nächsten Feld als das HQ
    let refinery = built.iter().find(|b| b.0 == BuildingKind::Raffinerie).unwrap().1;
    let nearest = |p: Vec3| nodes.iter().map(|n| flat(*n).distance(flat(p))).fold(f32::MAX, f32::min);
    assert!(nearest(refinery) < nearest(ENEMY_HQ_POS), "Raffinerie liegt nicht näher am Feld");
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
