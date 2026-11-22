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
    /// Befehle, die noch auf einen freien Tick warten
    pending: Vec<Cmd>,
    /// Tick, für den die Eingaben schon erzeugt wurden
    scripted: Option<u32>,
}

impl Peer {
    fn new(team: Team) -> Self {
        Peer { app: duel_app(), team, ls: Lockstep::new(), hashes: Vec::new(), pending: Vec::new(), scripted: None }
    }
    fn tick(&self) -> u32 {
        self.app.world().resource::<SimClock>().tick
    }
    /// Einen Tick versuchen; Nachrichten an den Mitspieler landen in `out`
    fn try_tick(&mut self, out: &mut VecDeque<Vec<u8>>) -> bool {
        let t = self.tick();
        if self.scripted != Some(t) {
            self.scripted = Some(t);
            let cmds = script(self.app.world_mut(), self.team, t);
            self.pending.extend(cmds);
        }
        out.extend(self.ls.fill(self.team, t, &mut self.pending));
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
    run_pair(false);
}

/// Wie oben, aber beide Seiten ändern ihre Eingabeverzögerung ständig und unabhängig
/// voneinander (2 bis 8 Ticks, wie bei schwankendem Ping): Die Spielstände bleiben gleich.
#[test]
fn changing_delays_keep_the_simulations_in_sync() {
    run_pair(true);
}

fn run_pair(vary: bool) {
    let mut a = Peer::new(PLAYER);
    let mut b = Peer::new(ENEMY);
    // Leitungen mit Verzögerung (Nachrichten bleiben in Reihenfolge, wie beim WebSocket)
    let (mut a_to_b, mut b_to_a): (VecDeque<(u32, Vec<u8>)>, VecDeque<(u32, Vec<u8>)>) = (VecDeque::new(), VecDeque::new());
    let end = 3000;
    let mut step = 0u32;
    let mut stalls = 0;
    while a.tick() < end || b.tick() < end {
        step += 1;
        if vary {
            a.ls.delay = MIN_DELAY + (step / 97) % (MAX_DELAY - MIN_DELAY + 1);
            b.ls.delay = MAX_DELAY - (step / 61) % (MAX_DELAY - MIN_DELAY + 1);
        }
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

/// Verzögerung aus der Antwortzeit: kurzer Ping → wenig Puffer, langer Ping → mehr,
/// runter erst nach ruhigen 3 Sekunden, Ping-Nachrichten kommen zurück
#[test]
fn delay_follows_the_measured_ping() {
    let mut n = Net::duel("r".into(), "k".into());
    assert_eq!(n.wanted_delay(), DELAY, "ohne Messung bleibt es beim Start");
    for _ in 0..30 {
        n.note_rtt(0.030);
    }
    assert_eq!(n.wanted_delay(), MIN_DELAY, "30 ms Ping: 2 Ticks");
    n.status = Status::Running;
    n.now = 10.0;
    n.adapt_delay();
    assert_eq!(n.lockstep.delay, DELAY, "nicht sofort kleiner");
    n.now = 13.5;
    n.adapt_delay();
    assert_eq!(n.lockstep.delay, DELAY - 1, "nach 3 s einen Tick kleiner");
    for _ in 0..30 {
        n.note_rtt(0.400);
    }
    n.adapt_delay();
    assert!(n.lockstep.delay >= 6, "400 ms Ping: sofort mehr Puffer, war {}", n.lockstep.delay);
    // Ping → Pong
    let mut out = Vec::new();
    n.handle(link::Incoming::Binary(Lockstep::ping(1.25)), &mut out);
    assert_eq!(out, vec![Lockstep::pong(1.25)]);
    let mut m = Net::duel("r".into(), "k".into());
    m.now = 1.30;
    m.handle(link::Incoming::Binary(Lockstep::pong(1.25)), &mut out);
    assert!((m.rtt.unwrap() - 0.05).abs() < 1e-9);
}

/// Befehle für erledigte oder weit entfernte Ticks werden verworfen (kein Speicherüberlauf)
#[test]
fn ticks_outside_the_window_are_refused() {
    let mut a = Lockstep::new();
    let mut b = Lockstep::new();
    let far = b.schedule(ENEMY, 5000, vec![]).unwrap();
    assert_eq!(a.receive(ENEMY, &far), Event::Bad);
    let ok = b.schedule(ENEMY, DELAY, vec![]).unwrap();
    assert_eq!(a.receive(ENEMY, &ok), Event::Nothing);
    for t in 0..DELAY {
        assert!(a.take(t).is_some());
    }
    let old = b.schedule(ENEMY, 1, vec![Cmd::Surrender]);
    assert!(old.is_none() || a.receive(ENEMY, &old.unwrap()) == Event::Bad, "erledigter Tick");
    let mut w = Writer::default();
    w.u8(MSG_CMDS);
    w.u32(1);
    assert_eq!(a.receive(ENEMY, &w.0), Event::Bad);
}

/// Ein fertiges Gebäude direkt in die Partie setzen und seine NetId holen
fn build_here(app: &mut App, team: Team, kind: BuildingKind, pos: Vec3) -> (Entity, NetId) {
    let e = app.world_mut().resource_scope(|world, kit: Mut<ModelKit>| {
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut c = Commands::new(&mut queue, world);
        let e = crate::construction::spawn_finished_structure(&mut c, &kit, team, kind, pos, None);
        queue.apply(world);
        e
    });
    run(app, vec![]);
    let id = *app.world().get::<NetId>(e).expect("NetId");
    (e, id)
}

fn run(app: &mut App, cmds: Vec<(Team, Cmd)>) {
    app.world_mut().resource_mut::<TickCmds>().0 = cmds;
    sim::run_tick(app.world_mut());
}

fn structures_of(app: &mut App, kind: BuildingKind) -> Vec<(Vec3, Team, Option<f32>)> {
    let mut q = app.world_mut().query::<(&Transform, &Team, &Structure, Option<&crate::components::Construction>)>();
    q.iter(app.world()).filter(|r| r.2 .0 == kind).map(|r| (r.0.translation, *r.1, r.3.map(|c| c.progress))).collect()
}

/// Abriss: nur eigene Gebäude, halbe Kosten zurück, doppelte Befehle erstatten nicht doppelt,
/// das Hauptquartier lässt sich nicht abreißen
#[test]
fn demolish_refunds_half_once() {
    let mut app = duel_app();
    let pos = PLAYER_HQ_POS + Vec3::new(12.0, 0.0, -2.0);
    let (_, tower) = build_here(&mut app, PLAYER, BuildingKind::Geschuetzturm, pos);
    let before = app.world().resource::<Stock>().0;
    // der Gegner darf fremde Gebäude nicht abreißen
    run(&mut app, vec![(ENEMY, Cmd::Demolish { building: tower })]);
    assert_eq!(structures_of(&mut app, BuildingKind::Geschuetzturm).len(), 1);
    run(&mut app, vec![(PLAYER, Cmd::Demolish { building: tower }), (PLAYER, Cmd::Demolish { building: tower })]);
    assert!(structures_of(&mut app, BuildingKind::Geschuetzturm).is_empty());
    let after = app.world().resource::<Stock>().0;
    let back = crate::construction::refund(BuildingKind::Geschuetzturm);
    assert_eq!(back, [60, 20]);
    assert_eq!(after[0][0], before[0][0] + back[0]);
    assert_eq!(after[0][1], before[0][1] + back[1]);
    assert_eq!(after[1], before[1]);
    // Hauptquartier bleibt stehen
    let mut q = app.world_mut().query_filtered::<(&NetId, &Team), With<Hq>>();
    let hq = *q.iter(app.world()).find(|r| *r.1 == PLAYER).expect("HQ").0;
    run(&mut app, vec![(PLAYER, Cmd::Demolish { building: hq })]);
    let mut q = app.world_mut().query_filtered::<&Team, With<Hq>>();
    assert!(q.iter(app.world()).any(|t| *t == PLAYER));
}

/// Versetzen: kostenlos, das Gebäude baut sich am neuen Platz ab halber Bauzeit auf;
/// auf einen belegten Platz oder von fremder Hand geht es nicht
#[test]
fn relocate_moves_and_rebuilds() {
    let mut app = duel_app();
    let pos = PLAYER_HQ_POS + Vec3::new(12.0, 0.0, -2.0);
    let (_, tower) = build_here(&mut app, PLAYER, BuildingKind::Geschuetzturm, pos);
    let before = app.world().resource::<Stock>().0;
    // auf das Hauptquartier: verweigert
    run(&mut app, vec![(PLAYER, Cmd::Relocate { building: tower, pos: PLAYER_HQ_POS, rot: None })]);
    // vom Gegner: verweigert
    let to = PLAYER_HQ_POS + Vec3::new(2.0, 0.0, -13.0);
    run(&mut app, vec![(ENEMY, Cmd::Relocate { building: tower, pos: to, rot: None })]);
    let towers = structures_of(&mut app, BuildingKind::Geschuetzturm);
    assert_eq!(towers.len(), 1);
    assert!(flat(towers[0].0).distance(flat(pos)) < 0.01);
    run(&mut app, vec![(PLAYER, Cmd::Relocate { building: tower, pos: to, rot: None })]);
    let towers = structures_of(&mut app, BuildingKind::Geschuetzturm);
    assert_eq!(towers.len(), 1, "{towers:?}");
    assert!(flat(towers[0].0).distance(flat(to)) < 0.01);
    assert!(towers[0].2.is_some_and(|p| (0.5..0.6).contains(&p)), "Baufortschritt {:?}", towers[0].2);
    assert_eq!(app.world().resource::<Stock>().0, before);
}

/// Neue Mauerlinien rasten an Mauerenden ein: Ende eines Stücks liegt 1 m von der Mitte entlang der Linie
#[test]
fn wall_ends_lie_on_the_line() {
    let a = Vec3::new(0.0, 0.0, 0.0);
    let b = Vec3::new(0.0, 0.0, 6.0);
    let (segs, end) = crate::construction::wall_segments(a, b);
    let rot = crate::construction::along(b - a);
    let last = crate::construction::wall_ends(*segs.last().expect("Stück"), rot);
    assert!(last.iter().any(|p| p.distance(end) < 1e-4), "{last:?} vs {end}");
    let first = crate::construction::wall_ends(segs[0], rot);
    assert!(first.iter().any(|p| p.distance(a) < 1e-4));
}
