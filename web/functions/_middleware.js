// Zugangsschutz für die Testversion auf Cloudflare Pages (Pages Function, läuft vor jeder Anfrage).
// Der Browser fragt nach Benutzer und Passwort; der Benutzername ist egal, das Passwort steht als
// geheime Variable SPIEL_PASSWORT im Pages-Projekt (wrangler pages secret put SPIEL_PASSWORT).
// Ohne gesetztes Passwort bleibt die Seite gesperrt. Suchmaschinen sollen nichts aufnehmen.

const REALM = 'Ferrum Nova Testversion';

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

export async function onRequest(context) {
  const secret = context.env.SPIEL_PASSWORT;
  if (!secret) {
    return denied();
  }
  const header = context.request.headers.get('Authorization') || '';
  const [scheme, encoded] = header.split(' ');
  if (scheme !== 'Basic' || !encoded) {
    return denied();
  }
  let decoded = '';
  try {
    decoded = new TextDecoder().decode(Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0)));
  } catch {
    return denied();
  }
  const password = decoded.slice(decoded.indexOf(':') + 1);
  if (!same(password, secret)) {
    return denied();
  }
  const response = await context.next();
  const out = new Response(response.body, response);
  out.headers.set('X-Robots-Tag', 'noindex, nofollow');
  return out;
}
