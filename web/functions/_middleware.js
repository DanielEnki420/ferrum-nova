// Zugangsschutz für die Testversion auf Cloudflare Pages (Pages Function, läuft vor jeder Anfrage).
// Der Browser fragt nach Benutzer und Passwort; der Benutzername ist egal, das Passwort steht als
// geheime Variable SPIEL_PASSWORT im Pages-Projekt (wrangler pages secret put SPIEL_PASSWORT).
// Ohne gesetztes Passwort bleibt die Seite gesperrt. Suchmaschinen sollen nichts aufnehmen.
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

function basicValid(request, secret) {
  const header = request.headers.get('Authorization') || '';
  const [scheme, encoded] = header.split(' ');
  if (scheme !== 'Basic' || !encoded) {
    return false;
  }
  let decoded = '';
  try {
    decoded = new TextDecoder().decode(Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0)));
  } catch {
    return false;
  }
  return same(decoded.slice(decoded.indexOf(':') + 1), secret);
}

export async function onRequest(context) {
  const secret = context.env.SPIEL_PASSWORT;
  if (!secret) {
    return denied();
  }
  const { request } = context;
  let fresh = false;
  if (!(await cookieValid(request, secret))) {
    if (!basicValid(request, secret)) {
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
