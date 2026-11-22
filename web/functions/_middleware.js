// Zugangsschutz für die Testversion auf Cloudflare Pages (Pages Function, läuft vor jeder Anfrage).
// Der Browser fragt nach Benutzer und Passwort; der Benutzername ist egal, das Passwort steht als
// geheime Variable SPIEL_PASSWORT im Pages-Projekt (wrangler pages secret put SPIEL_PASSWORT).
// Ohne gesetztes Passwort bleibt die Seite gesperrt. Suchmaschinen sollen nichts aufnehmen.
//
// Drei verschiedene falsche Passwörter in 15 Minuten sperren die IP-Adresse für 15 Minuten
// (Zähler: Durable Object Fehlversuche im Worker web/worker-duell, Bindung SPERRE).
//
// Nach der Anmeldung setzt die Seite ein Zugangs-Cookie (signiert mit dem Passwort, 30 Tage gültig).
// Damit klappt auch die Duell-Verbindung (WebSocket), bei der nicht jeder Browser die
// Basic-Auth-Daten mitschickt. Ein neues Passwort macht alle alten Cookies ungültig.

const REALM = 'Ferrum Nova Testversion';
const COOKIE = '__Host-fn_zugang';
const DAYS = 30;

function denied() {
  return new Response('Zugang nur mit Einladung.', {
    status: 401,
    headers: {
      'WWW-Authenticate': `Basic realm="${REALM}", charset="UTF-8"`,
      'Cache-Control': 'no-store',
      'X-Robots-Tag': 'noindex, nofollow',
    },
  });
}

// Vergleich ohne früh abzubrechen, damit die Antwortzeit nichts über das Passwort verrät
function same(a, b) {
  const enc = new TextEncoder();
  const x = enc.encode(a);
  const y = enc.encode(b);
  let diff = x.length ^ y.length;
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    diff |= (x[i] ?? 0) ^ (y[i] ?? 0);
  }
  return diff === 0;
}

// HMAC-Schlüssel je Passwort zwischenspeichern (ein Isolate bedient viele Anfragen)
let cachedKey = null;
async function hmacKey(secret) {
  if (cachedKey?.secret !== secret) {
    const key = await crypto.subtle.importKey(
      'raw',
      new TextEncoder().encode(secret),
      { name: 'HMAC', hash: 'SHA-256' },
      false,
      ['sign'],
    );
    cachedKey = { secret, key };
  }
  return cachedKey.key;
}

async function signature(secret, exp) {
  const mac = await crypto.subtle.sign('HMAC', await hmacKey(secret), new TextEncoder().encode(`v1|${exp}`));
  return [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function readCookie(request) {
  for (const part of (request.headers.get('Cookie') || '').split(';')) {
    const i = part.indexOf('=');
    if (i > 0 && part.slice(0, i).trim() === COOKIE) {
      return part.slice(i + 1).trim();
    }
  }
  return '';
}

// Gültiges Zugangs-Cookie: "<ablauf>.<signatur>", Ablauf in Sekunden seit 1970
async function cookieValid(request, secret) {
  const [exp, sig] = readCookie(request).split('.');
  const t = Number(exp);
  if (!sig || !Number.isInteger(t) || t <= Date.now() / 1000) {
    return false;
  }
  return same(sig, await signature(secret, exp));
}

// Passwort aus dem Basic-Auth-Kopf (null, wenn keiner da oder unlesbar)
function basicPassword(request) {
  const header = request.headers.get('Authorization') || '';
  const [scheme, encoded] = header.split(' ');
  if (scheme !== 'Basic' || !encoded) {
    return null;
  }
  try {
    const decoded = new TextDecoder().decode(Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0)));
    return decoded.slice(decoded.indexOf(':') + 1);
  } catch {
    return null;
  }
}

async function sha256(text) {
  const d = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

// Wer zählt als derselbe Absender: IPv4 die Adresse, IPv6 das /64-Netz
function ipKey(ip) {
  if (!ip.includes(':')) {
    return ip;
  }
  const [h, t] = ip.split('::');
  const head = h ? h.split(':') : [];
  const tail = t ? t.split(':') : [];
  const groups = [...head, ...Array(Math.max(0, 8 - head.length - tail.length)).fill('0'), ...tail];
  return groups.slice(0, 4).map((g) => g.toLowerCase().replace(/^0+(?=.)/, '')).join(':') + '::/64';
}

// Anmeldeversuch beim Zähler melden; true = Adresse gesperrt. Ohne Zähler (oder bei einem
// Fehler des Zählers) gibt es keine Sperre, damit niemand grundlos ausgesperrt wird.
async function locked(context, ok, password) {
  const sperre = context.env.SPERRE;
  if (!sperre) {
    return false;
  }
  try {
    const ip = context.request.headers.get('CF-Connecting-IP') || 'unbekannt';
    const body = JSON.stringify({ ok, hash: ok ? undefined : await sha256(password) });
    const res = await sperre.get(sperre.idFromName(ipKey(ip))).fetch('https://sperre/', { method: 'POST', body });
    return (await res.json()).blocked === true;
  } catch (e) {
    console.error('Sperre nicht erreichbar', e);
    return false;
  }
}

function tooMany() {
  return new Response('Zu viele falsche Passwörter. Bitte in 15 Minuten noch einmal versuchen.', {
    status: 429,
    headers: {
      'Content-Type': 'text/plain; charset=utf-8',
      'Retry-After': '900',
      'Cache-Control': 'no-store',
      'X-Robots-Tag': 'noindex, nofollow',
    },
  });
}

export async function onRequest(context) {
  const secret = context.env.SPIEL_PASSWORT;
  if (!secret) {
    return denied();
  }
  const { request } = context;
  let fresh = false;
  if (!(await cookieValid(request, secret))) {
    const password = basicPassword(request);
    if (password === null) {
      return denied();
    }
    // Jeder Versuch geht an den Zähler: nach 3 falschen Passwörtern ist die Adresse gesperrt,
    // dann hilft auch das richtige nicht mehr (sonst ließe sich weiter raten)
    const ok = same(password, secret);
    if (await locked(context, ok, password)) {
      return tooMany();
    }
    if (!ok) {
      return denied();
    }
    fresh = true;
  }

  const response = await context.next();
  // WebSocket-Upgrade (Duell) unverändert durchreichen: Kopieren verlöre die Verbindung
  if (response.status === 101 || response.webSocket) {
    return response;
  }
  const out = new Response(response.body, response);
  out.headers.set('X-Robots-Tag', 'noindex, nofollow');
  if (fresh) {
    const exp = String(Math.floor(Date.now() / 1000) + DAYS * 86400);
    out.headers.append(
      'Set-Cookie',
      `${COOKIE}=${exp}.${await signature(secret, exp)}; Max-Age=${DAYS * 86400}; Path=/; Secure; HttpOnly; SameSite=Lax`,
    );
    out.headers.set('Cache-Control', 'private, no-store');
  } else if (!out.headers.has('Cache-Control')) {
    out.headers.set('Cache-Control', 'private');
  }
  return out;
}
