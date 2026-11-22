//! Duell über das Netz: deterministischer Lockstep.
//!
//! Beide Browser rechnen dieselbe Simulation (sim.rs). Übertragen werden nur die Befehle: Was ein
//! Spieler in Tick `T` eingibt, gilt auf beiden Rechnern ab Tick `T + DELAY`. Ein Tick läuft erst,
//! wenn die Befehle beider Seiten für ihn da sind. Alle `HASH_EVERY` Ticks tauschen die beiden
//! eine Prüfsumme des Spielstands aus; weichen sie ab, meldet das Spiel einen Desync.
//!
//! Die Verbindung läuft über einen kleinen Cloudflare-Worker (`web/worker-duell`), der zwei
//! WebSockets pro Raum verbindet und Nachrichten nur weiterreicht.

pub mod cmd;
pub mod link;

use crate::components::*;
use crate::sim::{SimClock, SimHash};
use bevy::prelude::*;
use cmd::{Cmd, Outbox, Reader, TickCmds, Writer};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};

/// Eingabeverzögerung zu Beginn in Ticks (4 × 50 ms). Danach passt sie sich der gemessenen
/// Antwortzeit an (`MIN_DELAY` bis `MAX_DELAY`, siehe `Net::adapt_delay`).
pub const DELAY: u32 = 4;
pub const MIN_DELAY: u32 = 2;
pub const MAX_DELAY: u32 = 8;
/// So weit voraus darf der Mitspieler Befehle schicken (Ticks, 60 s)
const TICK_WINDOW: u32 = 1200;

/// Version der Spieldateien: beide müssen exakt dasselbe Spiel laufen lassen (build.rs)
pub const BUILD_ID: u64 = {
    let s = env!("FERRUM_BUILD_ID").as_bytes();
    let mut v: u64 = 0;
    let mut i = 0;
    while i < s.len() {
        let d = s[i];
        let n = if d >= b'a' { d - b'a' + 10 } else { d - b'0' };
        v = (v << 4) | n as u64;
        i += 1;
    }
    v
};

static ME: AtomicU8 = AtomicU8::new(0);

/// Das Team, das dieser Rechner steuert (Einzelspieler und Gastgeber: Team 0)
pub fn me() -> Team {
    Team(ME.load(Ordering::Relaxed))
}

pub fn set_me(team: Team) {
    ME.store(team.0, Ordering::Relaxed);
}

/// Der Gegner aus Sicht dieses Rechners
pub fn them() -> Team {
    Team(1 - me().0)
}

// ------------------------------------------------------------------ Lockstep-Kern (ohne Bevy)

const MSG_HELLO: u8 = 1;
const MSG_CMDS: u8 = 2;
const MSG_HASH: u8 = 3;
const MSG_PING: u8 = 4;
const MSG_PONG: u8 = 5;

/// Befehlsschlangen beider Seiten und die Prüfsummen, unabhängig vom Transport
#[derive(Default, Debug)]
pub struct Lockstep {
    /// Tick → Befehle je Team (None = noch nicht da)
    slots: BTreeMap<u32, [Option<Vec<Cmd>>; 2]>,
    hashes: BTreeMap<u32, [Option<u64>; 2]>,
    /// Höchster Tick, für den Befehle des Mitspielers da sind
    pub peer_tick: Option<u32>,
    pub desync: Option<u32>,
    /// Nächster Tick, für den dieser Rechner seine Befehle festlegen muss
    next_mine: u32,
    /// Nächster Tick, der gerechnet wird (ältere sind erledigt)
    next_take: u32,
    /// Aktuelle eigene Eingabeverzögerung in Ticks
    pub delay: u32,
}

/// Was aus einer Nachricht des Mitspielers folgt
#[derive(Debug, PartialEq)]
pub enum Event {
    Hello(u64),
    /// Messung des Mitspielers (Zeitstempel zurückschicken)
    Ping(f64),
    /// Antwort auf die eigene Messung
    Pong(f64),
    Nothing,
    Bad,
}

impl Lockstep {
    pub fn new() -> Self {
        let mut l = Lockstep::default();
        // die ersten Ticks laufen ohne Befehle, bis die ersten Eingaben ankommen
        for t in 0..DELAY {
            l.slots.insert(t, [Some(Vec::new()), Some(Vec::new())]);
        }
        l.next_mine = DELAY;
        l.delay = DELAY;
        l
    }

    /// Eigene Befehle lückenlos einplanen: alle Ticks bis `tick + delay`, die noch fehlen.
    /// Die Befehle kommen in den ersten davon. Ist nichts einzuplanen (die Verzögerung ist
    /// gerade gesunken), bleiben sie in `pending` für den nächsten Tick.
    pub fn fill(&mut self, me: Team, tick: u32, pending: &mut Vec<Cmd>) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while self.next_mine <= tick + self.delay {
            let t = self.next_mine;
            self.next_mine += 1;
            if self.scheduled(me, t) {
                continue;
            }
            out.extend(self.schedule(me, t, std::mem::take(pending)));
        }
        out
    }

    pub fn ping(at: f64) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(MSG_PING);
        w.u64(at.to_bits());
        w.0
    }

    pub fn pong(at: f64) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(MSG_PONG);
        w.u64(at.to_bits());
        w.0
    }

    pub fn hello() -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(MSG_HELLO);
        w.u64(BUILD_ID);
        w.0
    }

    /// Eigene Befehle für `tick` festlegen; Ergebnis: Nachricht an den Mitspieler
    /// (None, wenn für diesen Tick schon festgelegt)
    pub fn schedule(&mut self, me: Team, tick: u32, cmds: Vec<Cmd>) -> Option<Vec<u8>> {
        let slot = self.slots.entry(tick).or_default();
        let mine = &mut slot[me.0 as usize & 1];
        if mine.is_some() {
            return None;
        }
        let mut w = Writer::default();
        w.u8(MSG_CMDS);
        w.u32(tick);
        for c in &cmds {
            w.cmd(c);
        }
        *mine = Some(cmds);
        Some(w.0)
    }

    pub fn scheduled(&self, me: Team, tick: u32) -> bool {
        self.slots.get(&tick).is_some_and(|s| s[me.0 as usize & 1].is_some())
    }

    /// Nachricht des Mitspielers (Team `peer`) verarbeiten
    pub fn receive(&mut self, peer: Team, data: &[u8]) -> Event {
        let mut r = Reader::new(data);
        match r.u8() {
            Some(MSG_HELLO) => r.u64().map_or(Event::Bad, Event::Hello),
            Some(MSG_CMDS) => {
                let Some(tick) = r.u32() else { return Event::Bad };
                // Nur Ticks im erwartbaren Fenster: Erledigtes oder weit Zukünftiges würde
                // sonst ewig im Speicher liegen (ein manipulierter Mitspieler könnte ihn füllen)
                if tick < self.next_take || tick > self.next_take + TICK_WINDOW {
                    return Event::Bad;
                }
                let mut cmds = Vec::new();
                while !r.done() {
                    match r.cmd() {
                        Some(c) => cmds.push(c),
                        None => return Event::Bad,
                    }
                }
                self.slots.entry(tick).or_default()[peer.0 as usize & 1] = Some(cmds);
                self.peer_tick = Some(self.peer_tick.map_or(tick, |p| p.max(tick)));
                Event::Nothing
            }
            Some(MSG_PING) => r.u64().map_or(Event::Bad, |b| Event::Ping(f64::from_bits(b))),
            Some(MSG_PONG) => r.u64().map_or(Event::Bad, |b| Event::Pong(f64::from_bits(b))),
            Some(MSG_HASH) => {
                let (Some(tick), Some(h)) = (r.u32(), r.u64()) else { return Event::Bad };
                self.note_hash(peer, tick, h);
                Event::Nothing
            }
            _ => Event::Bad,
        }
    }

    /// Befehle beider Seiten für `tick`, Team 0 zuerst (None: noch nicht vollständig)
    pub fn take(&mut self, tick: u32) -> Option<Vec<(Team, Cmd)>> {
        let ready = self.slots.get(&tick).is_some_and(|s| s[0].is_some() && s[1].is_some());
        if !ready {
            return None;
        }
        let [a, b] = self.slots.remove(&tick)?;
        self.next_take = self.next_take.max(tick + 1);
        let mut out: Vec<(Team, Cmd)> = a.unwrap_or_default().into_iter().map(|c| (PLAYER, c)).collect();
        out.extend(b.unwrap_or_default().into_iter().map(|c| (ENEMY, c)));
        Some(out)
    }

    /// Eigene Prüfsumme festhalten; Ergebnis: Nachricht an den Mitspieler
    pub fn own_hash(&mut self, me: Team, tick: u32, h: u64) -> Vec<u8> {
        self.note_hash(me, tick, h);
        let mut w = Writer::default();
        w.u8(MSG_HASH);
        w.u32(tick);
        w.u64(h);
        w.0
    }

    fn note_hash(&mut self, team: Team, tick: u32, h: u64) {
        let e = self.hashes.entry(tick).or_default();
        e[team.0 as usize & 1] = Some(h);
        if let [Some(a), Some(b)] = *e {
            if a != b && self.desync.is_none() {
                self.desync = Some(tick);
            }
            self.hashes.remove(&tick);
        }
        // alte, unvollständige Einträge nicht ewig aufheben
        while self.hashes.len() > 64 {
            let first = *self.hashes.keys().next().unwrap();
            self.hashes.remove(&first);
        }
    }
}

// ------------------------------------------------------------------ Verbindung im Spiel

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Einzelspieler
    Solo,
    /// WebSocket wird geöffnet
    Connecting,
    /// verbunden, der Mitspieler fehlt noch
    WaitingForPeer,
    /// beide da, das Spiel läuft
    Running,
    /// ein Tick wartet auf Befehle des Mitspielers
    Stalled,
    PeerLeft,
    /// Raum voll, falscher Schlüssel, andere Spielversion, Verbindung verloren
    Failed(&'static str),
}

#[derive(Resource)]
pub struct Net {
    pub status: Status,
    pub lockstep: Lockstep,
    /// Raum und Schlüssel (Duell)
    pub room: Option<(String, String)>,
    peer_here: bool,
    hello_sent: bool,
    /// So lange wartet der laufende Tick schon (s)
    pub stalled_for: f32,
    /// Echtzeit (s), für die Laufzeitmessung
    pub now: f64,
    last_ping: f64,
    /// Geglättete Antwortzeit über das Relay und ihre Schwankung (s)
    pub rtt: Option<f64>,
    rtt_var: f64,
    /// Seit wann die Verzögerung kleiner sein dürfte (s)
    lower_since: Option<f64>,
}

impl Net {
    pub fn solo() -> Self {
        Net {
            status: Status::Solo,
            lockstep: Lockstep::new(),
            room: None,
            peer_here: false,
            hello_sent: false,
            stalled_for: 0.0,
            now: 0.0,
            last_ping: f64::NEG_INFINITY,
            rtt: None,
            rtt_var: 0.0,
            lower_since: None,
        }
    }
    pub fn duel(room: String, key: String) -> Self {
        Net { status: Status::Connecting, room: Some((room, key)), ..Net::solo() }
    }
    pub fn is_duel(&self) -> bool {
        self.room.is_some()
    }
    /// Steht eine Partie zum Spielen bereit?
    pub fn started(&self) -> bool {
        matches!(self.status, Status::Solo | Status::Running | Status::Stalled)
    }

    /// Ereignis vom Transport verarbeiten; Nachrichten zum Senden landen in `out`
    pub fn handle(&mut self, inc: link::Incoming, out: &mut Vec<Vec<u8>>) {
        use link::Incoming;
        match inc {
            Incoming::Open => {
                if self.status == Status::Connecting {
                    self.status = Status::WaitingForPeer;
                }
            }
            Incoming::Text(t) => self.control(&t, out),
            Incoming::Binary(b) => match self.lockstep.receive(them(), &b) {
                Event::Hello(build) => {
                    if build != BUILD_ID {
                        self.status = Status::Failed("Ihr spielt unterschiedliche Versionen. Beide bitte neu laden.");
                    } else if matches!(self.status, Status::WaitingForPeer | Status::Connecting) {
                        self.status = Status::Running;
                    }
                }
                Event::Ping(at) => out.push(Lockstep::pong(at)),
                Event::Pong(at) => self.note_rtt((self.now - at).max(0.0)),
                Event::Nothing => {}
                Event::Bad => warn!("Duell: unlesbare Nachricht ({} Bytes)", b.len()),
            },
            Incoming::Closed => {
                if !matches!(self.status, Status::Failed(_) | Status::PeerLeft) {
                    self.status = Status::Failed("Verbindung zum Duell-Server verloren.");
                }
            }
        }
    }

    /// Neue Messung der Antwortzeit einrechnen (wie bei TCP: Mittel und Schwankung)
    pub fn note_rtt(&mut self, sample: f64) {
        match self.rtt {
            None => {
                self.rtt = Some(sample);
                self.rtt_var = sample / 2.0;
            }
            Some(r) => {
                self.rtt_var = 0.75 * self.rtt_var + 0.25 * (r - sample).abs();
                self.rtt = Some(0.875 * r + 0.125 * sample);
            }
        }
    }

    /// Passende Verzögerung: Befehle müssen beim Mitspieler sein, bevor ihr Tick dran ist.
    /// Einweg-Laufzeit plus Schwankung plus ein Bild und ein halber Tick Spielraum.
    pub fn wanted_delay(&self) -> u32 {
        let Some(rtt) = self.rtt else { return DELAY };
        let need = rtt / 2.0 + 2.0 * self.rtt_var + 0.016 + 0.025;
        ((need / crate::sim::DT as f64).ceil() as u32).clamp(MIN_DELAY, MAX_DELAY)
    }

    /// Verzögerung anpassen: sofort größer, kleiner erst nach 3 s ruhiger Leitung
    /// und ohne Wartezeiten. Jede Seite wählt ihre eigene, `Lockstep::fill` hält sie lückenlos.
    pub fn adapt_delay(&mut self) {
        let want = self.wanted_delay();
        let ls = &mut self.lockstep;
        if self.status == Status::Stalled {
            self.lower_since = None;
        }
        if want > ls.delay {
            ls.delay = want;
            self.lower_since = None;
        } else if want < ls.delay {
            let since = *self.lower_since.get_or_insert(self.now);
            if self.now - since >= 3.0 {
                ls.delay -= 1;
                self.lower_since = None;
            }
        } else {
            self.lower_since = None;
        }
    }

    /// Steuermeldungen des Relays (JSON, siehe web/worker-duell)
    fn control(&mut self, t: &str, out: &mut Vec<Vec<u8>>) {
        let field = |name: &str| -> Option<String> {
            let key = format!("\"{name}\":");
            let rest = &t[t.find(&key)? + key.len()..];
            let rest = rest.trim_start().trim_start_matches('"');
            Some(rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect())
        };
        match field("t").as_deref() {
            Some("welcome") => {
                let seat: u8 = field("seat").and_then(|s| s.parse().ok()).unwrap_or(0);
                set_me(Team(seat.min(1)));
                info!("Duell: Sitz {seat} ({})", if seat == 0 { "Gastgeber, blau" } else { "Gast, rot" });
                if self.status == Status::Connecting {
                    self.status = Status::WaitingForPeer;
                }
            }
            Some("peer") => {
                let here = field("here").as_deref() == Some("true");
                if here {
                    self.peer_here = true;
                    if !self.hello_sent {
                        self.hello_sent = true;
                        out.push(Lockstep::hello());
                    }
                } else if self.peer_here {
                    self.peer_here = false;
                    if !matches!(self.status, Status::Failed(_)) {
                        self.status = Status::PeerLeft;
                    }
                }
            }
            Some("full") => self.status = Status::Failed("Dieses Duell ist schon voll (zwei Spieler)."),
            Some("denied") => self.status = Status::Failed("Der Einladungslink ist ungültig."),
            _ => warn!("Duell: unbekannte Meldung {t}"),
        }
    }
}

/// Der Transport (WebSocket im Browser, Speicher in Tests)
pub struct LinkBox(pub Box<dyn link::Link>);

/// Im Duell die Verbindung zum Relay öffnen (`/ws/<raum>?k=<schlüssel>` auf derselben Adresse)
pub fn connect(app: &mut App) {
    let Some((room, key)) = app.world().get_resource::<Net>().and_then(|n| n.room.clone()) else { return };
    #[cfg(target_arch = "wasm32")]
    {
        let url = relay_url(&room, &key);
        match link::WsLink::open(&url) {
            Ok(ws) => {
                app.insert_non_send(LinkBox(Box::new(ws)));
            }
            Err(e) => {
                error!("Duell: WebSocket {url} geht nicht auf: {e}");
                app.world_mut().resource_mut::<Net>().status = Status::Failed("Keine Verbindung zum Duell-Server.");
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (room, key);
        app.world_mut().resource_mut::<Net>().status = Status::Failed("Duelle gibt es nur im Browser.");
    }
}

#[cfg(target_arch = "wasm32")]
fn relay_url(room: &str, key: &str) -> String {
    let loc = web_sys::window().map(|w| w.location());
    let mut host = loc.as_ref().and_then(|l| l.host().ok()).unwrap_or_default();
    // lokal unter `trunk serve` (Port 8080) direkt zu `wrangler dev` (Port 8787):
    // Trunks WebSocket-Proxy reicht die Verbindung nicht sauber durch
    if matches!(host.as_str(), "localhost:8080" | "127.0.0.1:8080") {
        host = "127.0.0.1:8787".into();
    }
    let secure = loc.as_ref().and_then(|l| l.protocol().ok()).is_some_and(|p| p == "https:");
    format!("{}://{host}/ws/{room}?k={key}", if secure { "wss" } else { "ws" })
}

/// Zufällige Kennung aus Kleinbuchstaben und Ziffern (Raum, Schlüssel)
pub fn random_code(len: usize) -> String {
    const ALPHA: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut bytes = vec![0u8; len];
    #[cfg(target_arch = "wasm32")]
    if let Some(c) = web_sys::window().and_then(|w| w.crypto().ok()) {
        let _ = c.get_random_values_with_u8_array(&mut bytes);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) | 1;
        for b in &mut bytes {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }
    bytes.iter().map(|b| ALPHA[*b as usize % ALPHA.len()] as char).collect()
}

/// Eingehende Nachrichten abholen und verarbeiten, Antworten senden
pub fn poll(world: &mut World) {
    let Some(mut lb) = world.remove_non_send::<LinkBox>() else { return };
    let incoming = lb.0.poll();
    let mut out = Vec::new();
    if let Some(mut net) = world.get_resource_mut::<Net>() {
        for inc in incoming {
            net.handle(inc, &mut out);
        }
    }
    for m in out {
        lb.0.send(m);
    }
    world.insert_non_send(lb);
}

/// Vor einem Tick: eigene Befehle einplanen, Befehle beider Seiten bereitstellen.
/// `false`: dieser Tick kann noch nicht laufen.
pub fn prepare_tick(world: &mut World) -> bool {
    let duel = world.get_resource::<Net>().is_some_and(|n| n.is_duel());
    if !duel {
        let cmds = std::mem::take(&mut world.resource_mut::<Outbox>().0);
        let me = me();
        world.resource_mut::<TickCmds>().0 = cmds.into_iter().map(|c| (me, c)).collect();
        return true;
    }
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    world.resource_mut::<Net>().now = now;
    poll(world);
    let tick = world.resource::<SimClock>().tick;
    let me = me();
    let mut msgs = Vec::new();
    let ready = {
        let mut pending = std::mem::take(&mut world.resource_mut::<Outbox>().0);
        let mut net = world.resource_mut::<Net>();
        if !net.started() {
            // Eingaben vor dem Start verwerfen
            false
        } else {
            // Antwortzeit messen (zweimal pro Sekunde) und die Verzögerung danach richten
            if now - net.last_ping >= 0.5 {
                net.last_ping = now;
                msgs.push(Lockstep::ping(now));
            }
            net.adapt_delay();
            msgs.extend(net.lockstep.fill(me, tick, &mut pending));
            if !pending.is_empty() {
                // noch kein Tick frei (der Tick wartete oder die Verzögerung sank): aufheben
                world.resource_mut::<Outbox>().0 = pending;
                net = world.resource_mut::<Net>();
            }
            match net.lockstep.take(tick) {
                Some(cmds) => {
                    net.status = Status::Running;
                    net.stalled_for = 0.0;
                    world.resource_mut::<TickCmds>().0 = cmds;
                    true
                }
                None => {
                    net.status = Status::Stalled;
                    false
                }
            }
        }
    };
    for m in msgs {
        send(world, m);
    }
    ready
}

fn send(world: &mut World, m: Vec<u8>) {
    if let Some(mut lb) = world.get_non_send_mut::<LinkBox>() {
        lb.0.send(m);
    }
}

/// Nach einem Tick: Prüfsumme austauschen
pub fn after_tick(world: &mut World) {
    let tick = world.resource::<SimClock>().tick;
    let Some((t, h)) = world.resource::<SimHash>().latest else { return };
    if t + 1 != tick {
        return;
    }
    let Some(mut net) = world.get_resource_mut::<Net>() else { return };
    if !net.is_duel() {
        return;
    }
    let m = net.lockstep.own_hash(me(), t, h);
    if t % 200 == 0 {
        let ping = net.rtt.map_or(-1.0, |r| r * 1000.0);
        info!("Duell: Tick {t}, Prüfsumme {h:016x}, Ping {ping:.0} ms, Verzögerung {} Ticks", net.lockstep.delay);
    }
    if let Some(d) = net.lockstep.desync
        && !matches!(net.status, Status::Failed(_))
    {
        // weiterzuspielen hätte keinen Sinn: beide sähen ein anderes Spiel
        error!("Duell: Spielstände weichen ab (Desync) ab Tick {d}");
        net.status = Status::Failed("Die Spielstände sind auseinandergelaufen (Desync). Das Duell wurde beendet.");
    }
    send(world, m);
}

/// Wie viele Ticks dieser Rechner hinter dem Mitspieler liegt
pub fn ticks_behind(world: &World) -> u32 {
    let Some(net) = world.get_resource::<Net>() else { return 0 };
    let Some(peer) = net.lockstep.peer_tick else { return 0 };
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    // Der Mitspieler plant mit etwa derselben Verzögerung wie dieser Rechner
    peer.saturating_sub(net.lockstep.delay).saturating_sub(tick)
}

/// Wartezeit mitzählen (für die Anzeige „Warte auf Mitspieler“)
pub fn track_stall(time: Res<Time<Real>>, net: Option<ResMut<Net>>) {
    let Some(mut net) = net else { return };
    if net.status == Status::Stalled {
        net.stalled_for += time.delta_secs();
    } else if net.stalled_for != 0.0 {
        net.stalled_for = 0.0;
    }
}

#[cfg(test)]
mod tests;
