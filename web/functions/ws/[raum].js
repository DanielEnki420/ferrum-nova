// Duell: /ws/<raum>?k=<schlüssel> an den Raum im Durable Object weiterreichen (web/worker-duell).
// Läuft hinter functions/_middleware.js, also mit demselben Zugangsschutz wie das Spiel.

export async function onRequest(context) {
  const raum = String(context.params.raum || '');
  if (!/^[a-z0-9]{1,32}$/i.test(raum)) {
    return new Response('Unbekannter Raum', { status: 404 });
  }
  if (!context.env.DUELL) {
    return new Response('Duell-Server nicht eingerichtet (Durable-Object-Bindung DUELL fehlt).', { status: 503 });
  }
  const id = context.env.DUELL.idFromName(raum.toLowerCase());
  return context.env.DUELL.get(id).fetch(context.request);
}
