// Duell-Relay für Ferrum Nova: ein Durable Object je Raum verbindet zwei WebSockets und reicht
// die Nachrichten der Spieler unverändert an den anderen weiter. Spiellogik gibt es hier nicht;
// beide Browser rechnen die Simulation selbst (Lockstep, siehe game/src/net).
//
// Steuermeldungen an die Spieler (Text, JSON):
//   {"t":"welcome","seat":0|1}   Sitzplatz = Team (0 = Gastgeber, blau)
//   {"t":"peer","here":true}     der andere ist da (auch an den, der als Zweiter kommt)
//   {"t":"peer","here":false}    der andere ist gegangen
//   {"t":"full"} / {"t":"denied"}  Raum voll bzw. falscher Schlüssel; danach wird geschlossen
// Binäre Nachrichten sind Spielnachrichten und gehen an den anderen Spieler.

const ROOM = /^[a-z0-9]{1,32}$/i;
// Räume vergessen ihren Schlüssel nach einem Tag ohne Verbindung
const FORGET_MS = 24 * 60 * 60 * 1000;
// Schutz vor Missbrauch: Spielnachrichten sind klein (Befehle eines Ticks, Prüfsumme, Ping),
// normal sind rund 25 pro Sekunde. Wer deutlich mehr oder Größeres schickt, fliegt raus.
const MAX_BYTES = 64 * 1024;
const MAX_PER_SECOND = 200;

export class DuellRaum {
  constructor(state) {
    this.state = state;
  }

  async fetch(request) {
    if (request.headers.get('Upgrade') !== 'websocket') {
      return new Response('Nur WebSocket', { status: 426 });
    }
    const key = new URL(request.url).searchParams.get('k') || '';
    const pair = new WebSocketPair();
    const [client, server] = [pair[0], pair[1]];

    // Wer zuerst kommt, legt den Schlüssel des Raums fest
    let stored = await this.state.storage.get('key');
    if (!stored && ROOM.test(key)) {
      stored = key;
      await this.state.storage.put('key', key);
    }
    const present = this.state.getWebSockets();
    const refuse = (t) => {
      server.accept();
      server.send(JSON.stringify({ t }));
      server.close(1008, t);
      return new Response(null, { status: 101, webSocket: client });
    };
    if (stored !== key) return refuse('denied');
    const taken = new Set(present.map((ws) => ws.deserializeAttachment()?.seat));
    const seat = [0, 1].find((s) => !taken.has(s));
    if (seat === undefined) return refuse('full');

    this.state.acceptWebSocket(server);
    server.serializeAttachment({ seat });
    server.send(JSON.stringify({ t: 'welcome', seat }));
    if (present.length > 0) {
      server.send(JSON.stringify({ t: 'peer', here: true }));
      for (const ws of present) ws.send(JSON.stringify({ t: 'peer', here: true }));
    }
    await this.state.storage.setAlarm(Date.now() + FORGET_MS);
    return new Response(null, { status: 101, webSocket: client });
  }

  webSocketMessage(ws, message) {
    // Text von Spielern wird ignoriert; nur Spielnachrichten (binär) gehen weiter
    if (typeof message === 'string') return;
    if (message.byteLength > MAX_BYTES || this.tooFast(ws)) {
      try {
        ws.close(1008, 'zu viel');
      } catch {
        // schon zu
      }
      this.webSocketClose(ws, 1008);
      return;
    }
    for (const other of this.state.getWebSockets()) {
      if (other !== ws) other.send(message);
    }
  }

  // Nachrichten je Verbindung und Sekunde zählen (nur im Speicher; nach einem Ruhezustand
  // des Objekts beginnt die Zählung neu, das reicht als Bremse)
  tooFast(ws) {
    this.rates ??= new Map();
    const now = Date.now();
    let r = this.rates.get(ws);
    if (!r || now - r.start >= 1000) {
      r = { start: now, n: 0 };
      this.rates.set(ws, r);
    }
    r.n += 1;
    return r.n > MAX_PER_SECOND;
  }

  webSocketClose(ws, code) {
    this.rates?.delete(ws);
    for (const other of this.state.getWebSockets()) {
      if (other !== ws) other.send(JSON.stringify({ t: 'peer', here: false }));
    }
    try {
      ws.close(code === 1005 ? 1000 : code, 'bye');
    } catch {
      // schon zu
    }
  }

  webSocketError(ws) {
    this.webSocketClose(ws, 1011);
  }

  async alarm() {
    if (this.state.getWebSockets().length === 0) {
      await this.state.storage.deleteAll();
    } else {
      await this.state.storage.setAlarm(Date.now() + FORGET_MS);
    }
  }
}

// Eigenständig erreichbar unter /ws/<raum>?k=<schlüssel> (z. B. für wrangler dev).
// Auf der Website leitet die Pages Function web/functions/ws/[raum].js dorthin weiter.
export default {
  async fetch(request, env) {
    const m = new URL(request.url).pathname.match(/^\/ws\/([^/]+)$/);
    if (!m || !ROOM.test(m[1])) {
      return new Response('Unbekannter Raum', { status: 404 });
    }
    const id = env.DUELL.idFromName(m[1].toLowerCase());
    return env.DUELL.get(id).fetch(request);
  },
};
