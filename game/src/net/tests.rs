//! Tests für das Duell: Befehle über den Lockstep, Determinismus zweier Simulationen.

use super::*;
use crate::ai::AiState;
use crate::fx::FxEvent;
use crate::models::ModelKit;
use crate::orders::{Alerts, MoveKind};
use crate::sim::{self, SimClock, SimHash, SimRules};
use crate::terrain::{self, FIELDS};
use crate::units::Snapshot;
use bevy::ecs::system::RunSystemOnce;
use std::collections::VecDeque;

/// Kopflose Partie im Duell-Modus mit Startaufstellung wie im Spiel
fn duel_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(SimRules { duel: true })
        .insert_resource(Rng(0x2545_F491_4F6C_DD1D))
        .insert_resource(Stock([[1500, 600], [1500, 600]]))
        .init_resource::<Outcome>()
        .init_resource::<Snapshot>()
        .init_resource::<AiState>()
        .init_resource::<crate::nav::NavGrid>()
        .init_resource::<crate::nav::PathQueue>()
        .init_resource::<Alerts>()
        .init_resource::<crate::halkyon::Eclipse>()
        .init_resource::<crate::frame::Stats>()
        .add_message::<crate::units::BuildRequest>()
        .add_message::<crate::construction::PlaceRequest>()
        .add_message::<FxEvent>()
        .add_message::<crate::audio::PlaySfx>();
    let mut meshes = Assets::<Mesh>::default();
    let mut mats = Assets::<StandardMaterial>::default();
    let kit = ModelKit::new(&mut meshes, &mut mats);
    sim::add_sim(&mut app);
    {
        let world = app.world_mut();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut c = Commands::new(&mut queue, world);
        for (pos, kind, amount) in FIELDS {
            c.spawn((Transform::from_translation(terrain::on_ground(pos)), Visibility::default(), ResourceNode { kind, amount, max: amount }));
        }
        crate::world::spawn_start_forces(&mut c, &kit);
        queue.apply(world);
    }
    app.insert_resource(kit);
    app
}

/// Befehle, die ein „Spieler“ abhängig vom Spielstand gibt (auf beiden Rechnern gleich berechnet)
fn script(world: &mut World, team: Team, tick: u32) -> Vec<Cmd> {
    let mut cmds = Vec::new();
    let mut q = world.query::<(&NetId, &Team, &Unit, Has<Weapon>)>();
    let mut own: Vec<(NetId, bool)> = q.iter(world).filter(|r| *r.1 == team).map(|r| (*r.0, r.3)).collect();
    own.sort();
    let armed: Vec<NetId> = own.iter().filter(|r| r.1).map(|r| r.0).collect();
    let enemy_hq = if team == PLAYER { ENEMY_HQ_POS } else { PLAYER_HQ_POS };
    let home = if team == PLAYER { PLAYER_HQ_POS } else { ENEMY_HQ_POS };
    let dir = if team == PLAYER { Vec3::new(1.0, 0.0, -1.0) } else { Vec3::new(-1.0, 0.0, 1.0) };
    if tick.is_multiple_of(25) {
        cmds.push(Cmd::Produce { kind: if tick.is_multiple_of(75) { UnitKind::Spaeher } else { UnitKind::Panzer } });
    }
    if tick == 30 {
        cmds.push(Cmd::Place { kind: BuildingKind::Geschuetzturm, pos: home + dir * 12.0 + Vec3::new(3.0, 0.0, 3.0), rot: None });
    }
    if tick == 45 {
        cmds.push(Cmd::Place { kind: BuildingKind::Raffinerie, pos: home + Vec3::new(dir.x * -2.0, 0.0, dir.z * 14.0), rot: None });
    }
    if tick == 60 && !armed.is_empty() {
        // Frontlinie quer zur Karte
        let mid = home + dir * 25.0;
        let side = Vec3::new(dir.z, 0.0, -dir.x) * 6.0;
        cmds.push(Cmd::Move { units: armed.clone(), target: mid, line: Some((mid - side, mid + side)), kind: MoveKind::Move, queue: false });
    }
    if tick >= 300 && tick.is_multiple_of(100) && !armed.is_empty() {
        cmds.push(Cmd::Move { units: armed.clone(), target: enemy_hq, line: None, kind: MoveKind::AttackMove, queue: false });
    }
    if tick == 500 && armed.len() > 2 {
        cmds.push(Cmd::Hold { units: armed[..1].to_vec() });
        cmds.push(Cmd::Stop { units: armed[1..2].to_vec() });
    }
    cmds
}

#[derive(Component)]
struct Noise;

/// Was auf einem echten Rechner nebenbei passiert und die Simulation nicht verändern darf:
/// Optik-Entities kommen und gehen (andere Entity-IDs), Auswahl und Nebel wechseln, Kinder werden
/// angehängt und fremde Komponenten wandern auf Sim-Entities (andere Tabellenreihenfolge).
fn noise(world: &mut World, step: u32) {
    for i in 0..(step % 7) {
        world.spawn((Transform::from_xyz(i as f32, 0.0, 0.0), Visibility::default()));
    }
    let mut junk = world.query_filtered::<Entity, (Without<NetId>, Without<Health>, Without<Projectile>, Without<ResourceNode>, Without<ChildOf>, Without<Children>, With<Visibility>)>();
    let doomed: Vec<Entity> = junk.iter(world).step_by(3).collect();
    for e in doomed {
        world.despawn(e);
    }
    let mut units = world.query_filtered::<(Entity, Has<Selected>, Has<Noise>), With<Unit>>();
    let list: Vec<_> = units.iter(world).collect();
    for (i, (e, sel, noisy)) in list.into_iter().enumerate() {
        if (i as u32 + step).is_multiple_of(3) {
            if sel {
                world.entity_mut(e).remove::<Selected>();
            } else {
                world.entity_mut(e).insert(Selected);
            }
        }
        if (i as u32 + step).is_multiple_of(5) {
            if noisy {
                world.entity_mut(e).remove::<Noise>();
            } else {
                world.entity_mut(e).insert((Noise, Fogged));
            }
        }
    }
    let mut buildings = world.query_filtered::<Entity, (With<Building>, Without<Noise>)>();
    if let Some(b) = buildings.iter(world).nth((step % 3) as usize) {
        world.entity_mut(b).insert(Noise).with_child(Transform::default());
    }
}

struct Peer {
    app: App,
    team: Team,
    ls: Lockstep,
    hashes: Vec<(u32, u64)>,
}

impl Peer {
    fn new(team: Team) -> Self {
        Peer { app: duel_app(), team, ls: Lockstep::new(), hashes: Vec::new() }
    }
    fn tick(&self) -> u32 {
        self.app.world().resource::<SimClock>().tick
    }
    /// Einen Tick versuchen; Nachrichten an den Mitspieler landen in `out`
    fn try_tick(&mut self, out: &mut VecDeque<Vec<u8>>) -> bool {
        let t = self.tick();
        if !self.ls.scheduled(self.team, t + DELAY) {
            let cmds = script(self.app.world_mut(), self.team, t);
            out.extend(self.ls.schedule(self.team, t + DELAY, cmds));
        }
        let Some(cmds) = self.ls.take(t) else { return false };
        self.app.world_mut().resource_mut::<TickCmds>().0 = cmds;
        sim::run_tick(self.app.world_mut());
        if let Some((ht, h)) = self.app.world().resource::<SimHash>().latest
            && ht == t
        {
            self.hashes.push((ht, h));
            out.push_back(self.ls.own_hash(self.team, ht, h));
        }
        true
    }
}

/// Zwei Rechner, unterschiedlich schnell, einer mit Optik-Rauschen und verzögerter Leitung:
/// Am Ende stimmen alle Prüfsummen überein, und es wurde wirklich gekämpft.
#[test]
fn two_simulations_stay_in_sync_over_the_lockstep() {
    let mut a = Peer::new(PLAYER);
    let mut b = Peer::new(ENEMY);
    // Leitungen mit Verzögerung (Nachrichten bleiben in Reihenfolge, wie beim WebSocket)
    let (mut a_to_b, mut b_to_a): (VecDeque<(u32, Vec<u8>)>, VecDeque<(u32, Vec<u8>)>) = (VecDeque::new(), VecDeque::new());
    let end = 3000;
    let mut step = 0u32;
    let mut stalls = 0;
    while a.tick() < end || b.tick() < end {
        step += 1;
        // A: ein Tick je Schritt, B: unregelmäßig (wie wechselnde Bildraten)
        let mut out = VecDeque::new();
        if a.tick() < end && !a.try_tick(&mut out) {
            stalls += 1;
        }
        a_to_b.extend(out.drain(..).map(|m| (step + 3, m)));
        let runs = match step % 4 {
            0 => 0,
            1 => 2,
            _ => 1,
        };
        for _ in 0..runs {
            if b.tick() < end && !b.try_tick(&mut out) {
                stalls += 1;
                break;
            }
        }
        b_to_a.extend(out.drain(..).map(|m| (step + 1, m)));
        noise(b.app.world_mut(), step);
        // ankommende Nachrichten
        while a_to_b.front().is_some_and(|(at, _)| *at <= step) {
            let (_, m) = a_to_b.pop_front().unwrap();
            assert_ne!(b.ls.receive(PLAYER, &m), Event::Bad);
        }
        while b_to_a.front().is_some_and(|(at, _)| *at <= step) {
            let (_, m) = b_to_a.pop_front().unwrap();
            assert_ne!(a.ls.receive(ENEMY, &m), Event::Bad);
        }
        assert!(step < end * 4, "Lockstep hängt fest");
    }
    assert!(stalls > 0, "die Verzögerung sollte Wartezeiten erzeugen");
    assert_eq!(a.ls.desync, None, "A meldet Desync");
    assert_eq!(b.ls.desync, None, "B meldet Desync");
    assert_eq!(a.hashes.len(), (end / sim::HASH_EVERY) as usize);
    assert_eq!(a.hashes, b.hashes, "Prüfsummen weichen ab");
    let (ha, hb) = (sim::state_hash(a.app.world_mut()), sim::state_hash(b.app.world_mut()));
    assert_eq!(ha, hb);
    // Es ist auch etwas passiert: Einheiten gebaut, Gebäude errichtet, Verluste
    let stats = a.app.world().resource::<crate::frame::Stats>();
    eprintln!("Statistik: {stats:?}");
    assert!(stats.units_built.iter().all(|n| *n > 3), "gebaut: {:?}", stats.units_built);
    assert!(stats.units_lost.iter().sum::<u32>() > 0, "keine Verluste: {:?}", stats.units_lost);
    assert!(stats.structures_built.iter().all(|n| *n >= 1), "Gebäude: {:?}", stats.structures_built);
}

/// Ohne Rauschen, aber mit Rauschen auf beiden Seiten verschieden: jede Störung allein ändert nichts
#[test]
fn noise_alone_does_not_change_the_outcome() {
    let run = |noisy: bool| -> u64 {
        let mut app = duel_app();
        for t in 0..1200u32 {
            let mut cmds: Vec<(Team, Cmd)> = Vec::new();
            for team in [PLAYER, ENEMY] {
                cmds.extend(script(app.world_mut(), team, t).into_iter().map(|c| (team, c)));
            }
            app.world_mut().resource_mut::<TickCmds>().0 = cmds;
            sim::run_tick(app.world_mut());
            if noisy {
                noise(app.world_mut(), t);
            }
        }
        sim::state_hash(app.world_mut())
    };
    assert_eq!(run(false), run(true));
}

/// Ein Tick läuft erst, wenn die Befehle beider Seiten da sind
#[test]
fn tick_waits_for_both_sides() {
    let mut a = Lockstep::new();
    let mut b = Lockstep::new();
    // die ersten DELAY Ticks sind vorbelegt
    for t in 0..DELAY {
        assert!(a.take(t).is_some());
    }
    let m = a.schedule(PLAYER, DELAY, vec![Cmd::Produce { kind: UnitKind::Panzer }]).unwrap();
    assert!(a.schedule(PLAYER, DELAY, vec![]).is_none(), "zweimal derselbe Tick");
    assert!(a.take(DELAY).is_none(), "Befehle des Mitspielers fehlen");
    let reply = b.schedule(ENEMY, DELAY, vec![Cmd::Surrender]).unwrap();
    assert_eq!(a.receive(ENEMY, &reply), Event::Nothing);
    assert_eq!(b.receive(PLAYER, &m), Event::Nothing);
    let got_a = a.take(DELAY).expect("vollständig");
    assert_eq!(got_a, vec![(PLAYER, Cmd::Produce { kind: UnitKind::Panzer }), (ENEMY, Cmd::Surrender)]);
    for t in 0..DELAY {
        b.take(t);
    }
    assert_eq!(b.take(DELAY).unwrap(), got_a, "beide sehen dieselbe Reihenfolge");
    assert_eq!(a.peer_tick, Some(DELAY));
    // Prüfsummen: gleich = kein Desync, verschieden = Desync
    let h = b.own_hash(ENEMY, 20, 7);
    a.receive(ENEMY, &h);
    a.own_hash(PLAYER, 20, 7);
    assert_eq!(a.desync, None);
    let h = b.own_hash(ENEMY, 40, 8);
    a.own_hash(PLAYER, 40, 9);
    a.receive(ENEMY, &h);
    assert_eq!(a.desync, Some(40));
    // Unsinn wird erkannt
    assert_eq!(a.receive(ENEMY, &[2, 1]), Event::Bad);
    assert_eq!(a.receive(ENEMY, &Lockstep::hello()), Event::Hello(BUILD_ID));
}

/// Handschlag über die Speicher-Leitung: Sitzplätze, Hallo, Start
#[test]
fn handshake_over_memory_link() {
    use link::{Link, MemLink};
    let (mut la, mut lb) = MemLink::pair();
    let mut na = Net::duel("raum".into(), "k".into());
    let mut nb = Net::duel("raum".into(), "k".into());
    for _ in 0..3 {
        for (net, l) in [(&mut na, &mut la), (&mut nb, &mut lb)] {
            let mut out = Vec::new();
            for inc in l.poll() {
                net.handle(inc, &mut out);
            }
            for m in out {
                l.send(m);
            }
        }
    }
    assert_eq!(na.status, Status::Running);
    assert_eq!(nb.status, Status::Running);
    // Mitspieler geht
    let mut out = Vec::new();
    na.handle(link::Incoming::Text("{\"t\":\"peer\",\"here\":false}".into()), &mut out);
    assert_eq!(na.status, Status::PeerLeft);
    nb.handle(link::Incoming::Text("{\"t\": \"full\"}".into()), &mut out);
    assert!(matches!(nb.status, Status::Failed(_)));
    set_me(PLAYER);
}

/// Die weiche Darstellung zwischen zwei Ticks stellt die Sim-Lage danach exakt wieder her
#[test]
fn interpolation_restores_sim_positions_exactly() {
    let mut app = duel_app();
    for t in 0..80 {
        let cmds = script(app.world_mut(), PLAYER, t).into_iter().map(|c| (PLAYER, c)).collect();
        app.world_mut().resource_mut::<TickCmds>().0 = cmds;
        sim::run_tick(app.world_mut());
    }
    let before = sim::state_hash(app.world_mut());
    app.world_mut().resource_mut::<SimClock>().alpha = 0.37;
    app.world_mut().run_system_once(sim::interp_apply).unwrap();
    let mut q = app.world_mut().query_filtered::<&Transform, With<Unit>>();
    let moved = q.iter(app.world()).count();
    assert!(moved > 0);
    app.world_mut().run_system_once(sim::interp_restore).unwrap();
    assert_eq!(sim::state_hash(app.world_mut()), before);
}
