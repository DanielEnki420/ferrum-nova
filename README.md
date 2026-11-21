# Ferrum Nova

Ein eigenes Echtzeitstrategie-Spiel in **Rust + Bevy 0.20**. Es läuft als WebAssembly im Browser und ist in eine **Astro**-Website eingebettet. Aus demselben Code lässt sich auch eine Desktop-Version bauen.

```
game/   Spiel (Rust, Bevy). Läuft nativ oder mit Trunk als WebAssembly
web/    Website (Astro). Landingpage + /spielen mit dem eingebetteten Spiel
```

## Voraussetzungen

- Rust über Homebrew-`rustup` mit dem Ziel `wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev) (`brew install trunk`)
- Node 22+

`rustup` ist bei Homebrew „keg-only“. Damit `cargo` im Terminal gefunden wird, einmal ausführen:

```bash
echo 'export PATH="/opt/homebrew/opt/rustup/bin:$PATH"' >> ~/.zshrc
```

## Entwickeln

Spiel mit automatischem Neuladen im Browser (http://localhost:8080):

```bash
cd game && trunk serve --port 8080
```

Als Desktop-Fenster starten:

```bash
cd game && cargo run
```

## Website bauen

Baut das Spiel im Release-Modus nach `web/public/play/` und danach die Astro-Seite nach `web/dist/`:

```bash
cd web && npm run build
```

Lokal ansehen (http://localhost:4321):

```bash
cd web && npx astro preview
```

`web/dist/` ist eine rein statische Seite. Du kannst sie auf GitHub Pages, Vercel, Netlify oder deinen eigenen Server hochladen.

## Spielstand (Prototyp 0.1)

- Zwei Rohstoffe: Ferrit (Kristalle) und Plasma (Quellen). Die Sammler fahren selbst zum knapperen Rohstoff.
- Drei Einheiten: Sammler, Panzer, Raketenwerfer. Gebaut werden sie im HQ über die Tasten 1–3 oder die Knöpfe.
- Gegner-KI (Vaar-Schwarm): baut eine Wirtschaft auf und greift ab etwa 2,5 Minuten in immer größeren Wellen an.
- Sieg: feindliches HQ zerstört. Niederlage: eigenes HQ verloren.
- Grafik (alles im Code erzeugt, ohne Bilddateien):
  - **Gelände:** Relief mit Bergen am Kartenrand, kachelbare Detailtextur und Normal-Map mit Mipmaps.
  - **Modelle:** Fahrzeuge mit Ketten, Laufrollen und drehbaren Türmen, ein detailliertes HQ, Kristalle und Plasmaquellen.
  - **Licht:** HDR, Bloom, Dunst und Schatten, die sich an den Zoom anpassen.
  - **Effekte:** Mündungsfeuer, Raketen-Rauchspuren, Explosionen mit Lichtblitz, Brandspuren und Qualm bei beschädigten Fahrzeugen.
- Steuerung: Linksklick oder Ziehen wählt aus, F wählt alle Kampfeinheiten. Rechtsklick bewegt, greift an oder baut ab. Strg+Rechtsklick ist ein Angriffsmarsch. WASD/Pfeile/Bildschirmrand bewegen die Kamera, Q/E drehen sie, das Mausrad zoomt.

## Mögliche nächste Schritte

- Baukasten aus Chassis und Waffe, Forschungsbaum
- Pionierfahrzeug für weitere Gebäude (Geschütztürme, Fabriken)
- Nebel des Krieges und Minikarte
- Diplomatie mit mehreren KI-Fraktionen
- Mehrspieler über einen Rust-Server (WebSockets)
- Kleinere WebAssembly-Datei durch weniger Bevy-Features (aktuell 36 MB, komprimiert etwa 10 MB)
- Eigene Schriftart mit Umlauten für das HUD (die eingebaute Bevy-Schrift kann keine Umlaute)
