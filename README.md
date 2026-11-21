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
- Drei baubare Gebäude (Tasten 4–6 oder Knöpfe). Eine durchscheinende Vorschau zeigt, ob der Platz passt: in Reichweite eigener Gebäude, auf ebenem Boden, mit Abstand zu Gebäuden und Rohstofffeldern. Linksklick baut, mit Shift mehrere, Rechtsklick oder Esc bricht ab. Das Gebäude wächst dann aus dem Boden und arbeitet erst, wenn es fertig ist.
  - **Geschützturm** (120 F / 40 P): Betonsockel mit Brüstung, Drehkopf mit Zwillingsrohren. Verteidigt die Basis selbstständig.
  - **Raffinerie** (150 F / 0 P): Halle, zwei Silos auf Stelzen, Abladebucht mit Förderband. Sammler liefern an der nächsten Abladestelle ab, das verkürzt ihre Wege.
  - **Fabrik** (200 F / 80 P): Halle mit Sheddach, Portalkran und Kühlaggregat. Jede Fabrik beschleunigt die Produktion im HQ um 40 % (bis zu drei zählen).
- Gegner-KI (Vaar-Schwarm): baut eine Wirtschaft auf, errichtet eine Raffinerie, Geschütztürme zur Front und später eine Fabrik und greift ab etwa 2,5 Minuten in immer größeren Wellen an.
- Sieg: feindliches HQ zerstört. Niederlage: eigenes HQ verloren.
- Grafik:
  - **Gelände:** Relief mit Bergen am Kartenrand. Ein eigener Splat-Shader (`game/assets/shaders/terrain.wesl`) mischt vier echte Bodentexturen (Erde, Geröll, Fels, Gras; CC0, siehe [CREDITS.md](CREDITS.md)) höhenbasiert. Fels liegt dreiplanar auf steilen Flanken, die Kachelung wird durch eine zweite, gedrehte Abtastung aufgebrochen.
  - **Vegetation:** Zehntausende Grashalme in Büscheln, Büsche und Kiefern, alles prozedural. Der Wind bewegt sie im Vertex-Shader (`wind.wesl`) mit Böen, die über die Ebene wandern.
  - **Fahrzeuge:** Panzer mit keilförmigem Turm, Kommandantenkuppel, Rohr mit Mündungsbremse, Nebelwerfern, Kettenumlauf, Laufrollen, Antriebsrad und Schürzen. Raketenwerfer mit Rohrbündel, Sammler als Sechsrad-Laster mit Erzbunker. Staub und Gebrauchsspuren stecken in den Vertexfarben.
  - **Licht:** HDR, Bloom, Dunst und Schatten, die sich an den Zoom anpassen.
  - **Effekte:** Partikelsystem aus kamerazugewandten Flächen mit prozedural erzeugten Rauch-, Feuer- und Funkentexturen. Mündungsfeuer mit Staubaufwirbelung, Raketenstart mit Rückstrahl, Einschläge mit Funken, Erdfontäne, Brocken und Brandfleck. Explosionen mit mehrlagigem Feuerball, Druckwelle, Trümmern, die vom Gelände abprallen, Staubring, Rauchsäule und Lichtblitz. Zerstörte Fahrzeuge bleiben als verkohltes Wrack liegen (Turm teils abgesprengt), brennen, qualmen und versinken nach einer Minute.
- Leistung: Statisches (Felsen, Vegetation) ist pro Kartenkachel zu einem Mesh zusammengefasst, jedes Fahrzeug besteht aus drei bis vier Meshes. **F3** blendet FPS, Bildzeit und Objektzahl ein und aus.
- Steuerung: Linksklick oder Ziehen wählt aus, F wählt alle Kampfeinheiten. Rechtsklick bewegt, greift an oder baut ab. Strg+Rechtsklick ist ein Angriffsmarsch. WASD/Pfeile/Bildschirmrand bewegen die Kamera, Q/E drehen sie, das Mausrad zoomt.

## Mögliche nächste Schritte

- Baukasten aus Chassis und Waffe, Forschungsbaum
- Pionierfahrzeug, das Gebäude errichtet, und weitere Gebäudetypen (Forschung, Radar)
- Nebel des Krieges und Minikarte
- Diplomatie mit mehreren KI-Fraktionen
- Mehrspieler über einen Rust-Server (WebSockets)
- Texturen als KTX2 (GPU-komprimiert) statt JPG, sobald KTX-Software verfügbar ist
- Eigene Schriftart mit Umlauten für das HUD (die eingebaute Bevy-Schrift kann keine Umlaute)
