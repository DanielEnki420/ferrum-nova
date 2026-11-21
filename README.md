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
- Grafik:
  - **Gelände:** Relief mit Bergen am Kartenrand. Ein eigener Splat-Shader (`game/assets/shaders/terrain.wesl`) mischt vier echte Bodentexturen (Erde, Geröll, Fels, Gras; CC0, siehe [CREDITS.md](CREDITS.md)) höhenbasiert. Fels liegt dreiplanar auf steilen Flanken, die Kachelung wird durch eine zweite, gedrehte Abtastung aufgebrochen.
  - **Vegetation:** Zehntausende Grashalme in Büscheln, Büsche und Kiefern, alles prozedural. Der Wind bewegt sie im Vertex-Shader (`wind.wesl`) mit Böen, die über die Ebene wandern.
  - **Fahrzeuge:** Panzer mit keilförmigem Turm, Kommandantenkuppel, Rohr mit Mündungsbremse, Nebelwerfern, Kettenumlauf, Laufrollen, Antriebsrad und Schürzen. Raketenwerfer mit Rohrbündel, Sammler als Sechsrad-Laster mit Erzbunker. Staub und Gebrauchsspuren stecken in den Vertexfarben.
  - **Licht:** HDR, Bloom, Dunst und Schatten, die sich an den Zoom anpassen.
  - **Effekte:** Mündungsfeuer, Raketen-Rauchspuren, Explosionen mit Lichtblitz, Brandspuren und Qualm bei beschädigten Fahrzeugen.
- Leistung: Statisches (Felsen, Vegetation) ist pro Kartenkachel zu einem Mesh zusammengefasst, jedes Fahrzeug besteht aus drei bis vier Meshes. **F3** blendet FPS, Bildzeit und Objektzahl ein und aus.
- Steuerung: Linksklick oder Ziehen wählt aus, F wählt alle Kampfeinheiten. Rechtsklick bewegt, greift an oder baut ab. Strg+Rechtsklick ist ein Angriffsmarsch. WASD/Pfeile/Bildschirmrand bewegen die Kamera, Q/E drehen sie, das Mausrad zoomt.

## Mögliche nächste Schritte

- Baukasten aus Chassis und Waffe, Forschungsbaum
- Pionierfahrzeug für weitere Gebäude (Geschütztürme, Fabriken)
- Nebel des Krieges und Minikarte
- Diplomatie mit mehreren KI-Fraktionen
- Mehrspieler über einen Rust-Server (WebSockets)
- HQ und Rohstofffelder im selben Detailgrad wie die Fahrzeuge
- Texturen als KTX2 (GPU-komprimiert) statt JPG, sobald KTX-Software verfügbar ist
- Eigene Schriftart mit Umlauten für das HUD (die eingebaute Bevy-Schrift kann keine Umlaute)
