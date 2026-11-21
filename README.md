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
- Sechs Einheiten, gebaut im HQ über die Tasten 1–6 oder die Knöpfe:
  - **Sammler** (50 F): Sechsrad-Laster, dessen Erzbunker sich sichtbar füllt.
  - **Späher** (60 F / 10 P): schneller 4×4-Spähwagen mit Maschinenkanone.
  - **Panzer** (100 F / 25 P): Kampfpanzer, das Rückgrat der Armee.
  - **Bollwerk** (220 F / 90 P, braucht eine Fabrik): schwerer Panzer mit Reaktivpanzerung, Räumschild und Dachmaschinengewehr.
  - **Raketenwerfer** (80 F / 70 P): große Reichweite, Raketen mit Rauchspur.
  - **Haubitze** (150 F / 110 P, braucht eine Fabrik): Artillerie mit Stützspaten, Granaten im Bogen und Flächenschaden.
- Drei baubare Gebäude (Tasten 7–9 oder Knöpfe). Eine durchscheinende Vorschau zeigt, ob der Platz passt: in Reichweite eigener Gebäude, auf ebenem Boden, mit Abstand zu Gebäuden und Rohstofffeldern. Linksklick baut, mit Shift mehrere, Rechtsklick oder Esc bricht ab. Das Gebäude wächst dann aus dem Boden und arbeitet erst, wenn es fertig ist.
  - **Geschützturm** (120 F / 40 P): Betonsockel mit Brüstung, Drehkopf mit Zwillingsrohren. Verteidigt die Basis selbstständig.
  - **Raffinerie** (150 F / 0 P): Halle, zwei Silos auf Stelzen, Abladebucht mit Förderband. Sammler liefern an der nächsten Abladestelle ab, das verkürzt ihre Wege.
  - **Fabrik** (200 F / 80 P): Halle mit Sheddach, Portalkran und Kühlaggregat. Jede Fabrik beschleunigt die Produktion im HQ um 40 % (bis zu drei zählen).
- Gegner-KI (Vaar-Schwarm) in drei Ebenen:
  - **Strategie:** Die KI sieht nur, was ihre Einheiten und Gebäude erfassen, und merkt es sich. Ein Späher erkundet Felder, Kartenmitte und Spielerbasis. Gebaut wird als Konter auf das Gesehene: Haubitzen gegen Geschütztürme (ohne Fabrik Raketenwerfer), Raketenwerfer und Bollwerke gegen viel Panzerung, Späher gegen Artillerie. Die Wirtschaft wächst mit zwei Sammlern je Raffinerie. Nach etwa vier Minuten entsteht ein Außenposten mit zweiter Raffinerie und Geschützturm an einem weiteren Feld.
  - **Verteidigung:** Gegner nahe Basis oder Sammlern gelten als Bedrohung. Die Reserve rückt aus, bei Übermacht kehrt auch eine laufende Angriffswelle um. Sammler fliehen und meiden umkämpfte Felder, neue Geschütztürme kommen an die bedrohte Seite.
  - **Angriff und Taktik:** Ab etwa 2,5 Minuten sammeln sich Wellen vor der Basis und rücken geschlossen vor, Nachzügler schließen auf. Ziele sind bekannte Gebäude (Raffinerien zuerst, mit Artillerie die Geschütztürme), sonst das HQ. Einheiten wählen ihr Ziel nach Waffe und Zustand: Raketen gegen Panzerung, Späher gegen Artillerie, angeschlagene Gegner zuerst. Haubitzen halten Abstand, schwer beschädigte Einheiten fahren zur Reparatur ans HQ, eine stark geschwächte Welle zieht sich zurück.
- Werkstatt: Fahrzeuge in der Nähe des eigenen HQ werden außerhalb des Gefechts langsam repariert (beide Seiten).
- Sieg: feindliches HQ zerstört. Niederlage: eigenes HQ verloren.
- Grafik:
  - **Gelände:** Relief mit Bergen am Kartenrand. Ein eigener Splat-Shader (`game/assets/shaders/terrain.wesl`) mischt vier echte Bodentexturen (Erde, Geröll, Fels, Gras; CC0, siehe [CREDITS.md](CREDITS.md)) höhenbasiert. Fels liegt dreiplanar auf steilen Flanken, die Kachelung wird durch eine zweite, gedrehte Abtastung aufgebrochen.
  - **Vegetation:** Zehntausende Grashalme in Büscheln, Büsche und Kiefern, alles prozedural. Der Wind bewegt sie im Vertex-Shader (`wind.wesl`) mit Böen, die über die Ebene wandern.
  - **Fahrzeuge:** Panzer mit keilförmigem Turm, Kommandantenkuppel, Rohr mit Mündungsbremse, Nebelwerfern, Kettenumlauf, Laufrollen, Antriebsrad und Schürzen. Raketenwerfer mit Rohrbündel, Sammler als Sechsrad-Laster mit Erzbunker. Staub und Gebrauchsspuren stecken in den Vertexfarben.
  - **Fahrzeug-Animation:** Räder, Antriebs- und Leiträder drehen sich passend zur gefahrenen Strecke. Ketten hinterlassen Spuren im Sand, die langsam verblassen, schnelle Fahrt wirbelt Staub auf. Beim Schuss springen Rohr und Wanne zurück, beim Anfahren und Bremsen nickt das Fahrzeug. Stark beschädigte Fahrzeuge werden rußgeschwärzt, qualmen und brennen schließlich.
  - **Licht:** HDR, Bloom, Dunst und Schatten, die sich an den Zoom anpassen.
  - **Effekte:** Partikelsystem aus kamerazugewandten Flächen mit prozedural erzeugten Rauch-, Feuer- und Funkentexturen. Mündungsfeuer mit Staubaufwirbelung, Raketenstart mit Rückstrahl, Einschläge mit Funken, Erdfontäne, Brocken und Brandfleck. Explosionen mit mehrlagigem Feuerball, Druckwelle, Trümmern, die vom Gelände abprallen, Staubring, Rauchsäule und Lichtblitz. Zerstörte Fahrzeuge bleiben als verkohltes Wrack liegen (Turm teils abgesprengt), brennen, qualmen und versinken nach einer Minute.
  - **Waffen:** Jede Waffenklasse hat ihr eigenes Mündungsfeuer und Geschoss. Die Maschinenkanone des Spähers züngelt kurz und wirft Hülsen aus. Das Bollwerk feuert mit langem Feuerstrahl und seitlichen Rauchfahnen aus der Mündungsbremse. Die Haubitze stößt einen Rauchring aus und schlägt eine Druckwelle in den Boden. Messinghülsen springen über den Boden.
  - **Einschläge:** Treffer auf Panzerung sprühen Funken und Querschläger, Treffer auf Gebäude grauen Betonstaub und Splitter. Im Boden werfen Granaten eine Erdfontäne auf, die Haubitze eine hohe Erdsäule mit Druckwelle und großem Krater.
  - **Gebäudeeinsturz:** Zerstörte Gebäude sacken unter mehreren Explosionen ab und kippen. Eine Staubwolke quillt heraus, zurück bleibt ein brennender Trümmerhaufen mit Wandresten und verbogenen Trägern.
  - **Umgebung:** Die Raffinerie dampft, die Fabrikschornsteine rauchen, auf Baustellen sprühen Schweißfunken mit bläulichem Lichtbogen, über den Plasmaquellen steigen violett leuchtende Teilchen auf.
- Leistung: Statisches (Felsen, Vegetation) ist pro Kartenkachel zu einem Mesh zusammengefasst, jedes Fahrzeug besteht aus drei bis vier Meshes. **F3** blendet FPS, Bildzeit und Objektzahl ein und aus.
- Tests: `cargo test` im Ordner `game` lässt die Spiellogik ohne Grafik über simulierte Zeit laufen. Geprüft wird unter anderem, dass Sammler an der nächsten fertigen Raffinerie abladen, dass die KI ihre Gebäude an gültigen Plätzen errichtet dass ein zerstörtes Gebäude sofort aus dem Spiel ausscheidet, einstürzt und Trümmer hinterlässt, und wie sich die KI verhält: Konter, Flucht der Sammler, Verteidigung, Wellen mit Rückzug, Reparatur, Ausweichen der Haubitzen, Erkundung. Eine ganze Partie läuft dabei im Zeitraffer durch.
- Steuerung: Linksklick oder Ziehen wählt aus, F wählt alle Kampfeinheiten. Rechtsklick bewegt, greift an oder baut ab. Strg+Rechtsklick ist ein Angriffsmarsch. WASD/Pfeile/Bildschirmrand bewegen die Kamera, Q/E drehen sie, das Mausrad zoomt.

## Mögliche nächste Schritte

- Baukasten aus Chassis und Waffe, Forschungsbaum
- Pionierfahrzeug, das Gebäude errichtet, und weitere Gebäudetypen (Forschung, Radar)
- Nebel des Krieges und Minikarte
- Diplomatie mit mehreren KI-Fraktionen
- Mehrspieler über einen Rust-Server (WebSockets)
- Texturen als KTX2 (GPU-komprimiert) statt JPG, sobald KTX-Software verfügbar ist
- Eigene Schriftart mit Umlauten für das HUD (die eingebaute Bevy-Schrift kann keine Umlaute)
