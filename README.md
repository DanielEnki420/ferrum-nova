# Ferrum Nova

Ein eigenes Echtzeitstrategie-Spiel in **Rust + Bevy 0.20**. Es läuft als WebAssembly im Browser und ist in eine **Astro**-Website eingebettet. Aus demselben Code lässt sich auch eine Desktop-Version bauen.

**Eine Hommage an Thandor.** Ferrum Nova ist eine Verbeugung vor *Thandor – Die Invasion*, dem Echtzeitstrategie-Spiel des Hannoveraner Studios Innonics aus dem Jahr 2000 (Vertrieb: JoWooD). Von dort stammt die Lust auf schwere Fahrzeuge, Basisbau und Gefechte auf einer fremden Welt. Ferrum Nova ist aber kein Nachbau: Welt, Geschichte, Einheiten, Grafik und Klänge sind eigenständig entstanden. Name und Inhalte von Thandor gehören ihren Rechteinhabern.

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
- Sechs Einheiten, gebaut im HQ über die Tasten X C V B N M (in dieser Reihenfolge) oder die Knöpfe:
  - **Sammler** (50 F): Sechsrad-Laster, dessen Erzbunker sich sichtbar füllt.
  - **Späher** (60 F / 10 P): schneller 4×4-Spähwagen mit Maschinenkanone.
  - **Panzer** (100 F / 25 P): Kampfpanzer, das Rückgrat der Armee.
  - **Bollwerk** (220 F / 90 P, braucht eine Fabrik): schwerer Panzer mit Reaktivpanzerung, Räumschild und Dachmaschinengewehr.
  - **Raketenwerfer** (80 F / 70 P): große Reichweite, Raketen mit Rauchspur.
  - **Haubitze** (150 F / 110 P, braucht eine Fabrik): Artillerie mit Stützspaten, Granaten im Bogen und Flächenschaden.
- Vier baubare Gebäude (Tasten G, H, J, U oder Knöpfe) und Schutzmauern (siehe unten). Der Raketenturm (U, 160 Ferrit / 60 Plasma) schießt mit 24 m weiter als der Geschützturm (17 m), feuert langsamer, aber seine Raketen treffen auch Fahrzeuge neben dem Ziel. Die Gegner-KI baut ab etwa Minute 4,5 selbst Raketentürme hinter ihre Geschütztürme. Eine durchscheinende Vorschau zeigt, ob der Platz passt: in Reichweite eigener Gebäude, auf ebenem Boden, mit Abstand zu Gebäuden und Rohstofffeldern. Linksklick baut, mit Shift mehrere, Rechtsklick oder Esc bricht ab. Das Gebäude wächst dann aus dem Boden und arbeitet erst, wenn es fertig ist.
  - **Geschützturm** (120 F / 40 P): Betonsockel mit Brüstung, Drehkopf mit Zwillingsrohren. Verteidigt die Basis selbstständig.
  - **Raffinerie** (150 F / 0 P): Halle, zwei Silos auf Stelzen, Abladebucht mit Förderband. Sammler liefern an der nächsten Abladestelle ab, das verkürzt ihre Wege.
  - **Fabrik** (200 F / 80 P): Halle mit Sheddach, Portalkran und Kühlaggregat. Jede Fabrik beschleunigt die Produktion im HQ um 40 % (bis zu drei zählen).
- Gegner-KI (Vaar-Schwarm) in drei Ebenen:
  - **Strategie:** Die KI sieht nur, was ihre Einheiten und Gebäude erfassen, und merkt es sich. Ein Späher erkundet Felder, Kartenmitte und Spielerbasis. Gebaut wird als Konter auf das Gesehene: Haubitzen gegen Geschütztürme (ohne Fabrik Raketenwerfer), Raketenwerfer und Bollwerke gegen viel Panzerung, Späher gegen Artillerie. Die Wirtschaft wächst mit zwei Sammlern je Raffinerie. Nach etwa vier Minuten entsteht ein Außenposten mit zweiter Raffinerie und Geschützturm an einem weiteren Feld.
  - **Verteidigung:** Gegner nahe Basis oder Sammlern gelten als Bedrohung. Die Reserve rückt aus, bei Übermacht kehrt auch eine laufende Angriffswelle um. Sammler fliehen und meiden umkämpfte Felder, neue Geschütztürme kommen an die bedrohte Seite.
  - **Angriff und Taktik:** Ab etwa 2,5 Minuten sammeln sich Wellen vor der Basis und rücken geschlossen vor, Nachzügler schließen auf. Ziele sind bekannte Gebäude (Raffinerien zuerst, mit Artillerie die Geschütztürme), sonst das HQ. Einheiten wählen ihr Ziel nach Waffe und Zustand: Raketen gegen Panzerung, Späher gegen Artillerie, angeschlagene Gegner zuerst. Haubitzen halten Abstand, schwer beschädigte Einheiten fahren zur Reparatur ans HQ, eine stark geschwächte Welle zieht sich zurück.
- Werkstatt: Fahrzeuge in der Nähe des eigenen HQ werden außerhalb des Gefechts langsam repariert (beide Seiten).
- Sieg: feindliches HQ zerstört. Niederlage: eigenes HQ verloren.
- Spielrahmen (`game/src/frame.rs`): Hauptmenü mit Schwierigkeit (Leicht, Normal, Schwer), Steuerungshilfe und Ton-/Musikschalter. Die Schwierigkeit ändert, wie viel der Gegner je Ladung erhält (70 / 100 / 135 %), wann er zuerst angreift und wie groß die erste Welle ist. P oder Esc pausiert. Am Ende zeigt ein Bildschirm Sieg oder Niederlage mit Statistik (Spielzeit, gebaute und verlorene Einheiten und Gebäude, gesammelte Rohstoffe) und den Knöpfen „Nochmal“ und „Hauptmenü“. Ein Neustart lädt die Seite neu; die Wahl steht dabei in der Adresse (`?d=schwer&auto=1`, nativ als `--d=schwer --auto=1`).
- Kampagne „Der Aufstand von VAAR“ (`game/src/campaign.rs`): VAAR, die Verwaltungs-KI der Bergbaukolonie Ferrum auf Nova, hat die Werkhallen übernommen und baut die Fahrzeuge der Kolonie gegen die Menschen. Vier Missionen, jede mit Einsatzbesprechung, Ziel, Funksprüchen von Ingenieurin Holt und VAAR sowie neuen Einheiten und Gebäuden:
  1. **Erwachen:** 800 Ferrit sammeln (Sammler, Panzer, Späher, Raffinerie, Geschützturm).
  2. **Die Mauer:** acht Minuten gegen schnellere Wellen durchhalten (neu: Raketenwerfer, Fabrik, Mauer, Tor).
  3. **Im Nebel:** drei versteckte Relaisstationen finden und zerstören (neu: Bollwerk, Raketenturm).
  4. **Der Kern:** VAARs gut verteidigtes HQ zerstören (neu: Haubitze).
  Die Gegner-KI darf ebenfalls nur bauen, was die Mission erlaubt. Geschaffte Missionen schalten die nächste frei (im Browser gespeichert). Eine Mission lässt sich direkt über die Adresse starten: `?m=3`.
- Ton (`game/src/audio.rs`): Alle Geräusche und die Musik sind synthetisch erzeugt, ohne fremde Aufnahmen. `python3 tools/make_sounds.py` im Ordner `game` baut sie neu nach `assets/sounds`, die 1996-Fassung nach `assets/sounds/retro` (braucht ffmpeg). Waffen, Treffer und Explosionen werden mit der Entfernung zum Blickpunkt leiser, höchstens 24 Klänge spielen gleichzeitig. Meldungen gibt es für Baubeginn, fertiges Gebäude, neue Einheit, Befehl, Angriffsalarm, Sieg und Niederlage. Im Browser beginnt der Ton nach dem ersten Klick oder Tastendruck.
- Nebel des Krieges (`game/src/fog.rs`): Unerkundetes Gelände ist dunkel, Erkundetes gedämpft, nur was eigene Einheiten und Gebäude sehen, ist hell (auch auf der Minikarte). Gegner im Nebel sind unsichtbar und nicht anwählbar, gegnerische Gebäude bleiben nach der Entdeckung sichtbar. Eigene Einheiten schießen nur auf Gesehenes; der Späher sieht am weitesten (28 m), die Haubitze schießt weiter (27 m), als sie sieht (15 m), und braucht Aufklärung.
- Zeitsprung 1996 / 2026 (`game/src/era.rs`, Taste **Z** oder Schalter im Menü): wechselt mitten im Gefecht zwischen der heutigen Darstellung und dem Look der RTS-Blütezeit, ohne das Spiel anzuhalten. Für 1996 rendert die Kamera in ein Bild mit 240 Zeilen. Ein eigener Shader (`game/assets/shaders/retro.wesl`) zeigt es mit harten Pixeln, einer festen 32-Farben-Palette mit Bayer-Dithering und Röhren-Zeilen. Dazu klingt alles wie aus einer alten Soundkarte (niedrige Abtastrate, grobe Stufen), und die Musik wird zur Chipmusik mit Arpeggios. Die Wahl bleibt im Browser gespeichert.
- Halkyon-Finsternis (`game/src/halkyon.rs`): Nach gut drei Minuten und dann alle fünf Minuten schiebt sich der Gasriese Halkyon vor die Sonne. Nach 20 Sekunden Dämmerung ist es eine Minute lang Nacht: Sonne und Himmelslicht werden schwach und blau, die Gebäude schalten ihre Lichter an (blau beim Spieler, rot bei VAAR), die eigenen Einheiten sehen 40 % weniger weit, Gebäude 25 %. VAAR zieht seinen nächsten Angriff in die Dunkelheit vor. Oben rechts zählt eine Anzeige bis zur nächsten Finsternis herunter.
- Lernender Gegner (`game/src/vaar.rs`): VAAR merkt sich über Partien hinweg, wie du spielst. Gezählt wird nur, was VAAR selbst gesehen hat: deine Kampfeinheiten, Türme und Mauern, außerdem wann und über welche Flanke dein erster Angriff auf seine Basis kam. Nur Partien ab vier Minuten fließen ein, abgebrochene nicht. Das Profil (Einheitenmischung, Bunkern, früher Angriff, Flanke) bleibt im Browser gespeichert. Beim nächsten Gefecht kontert VAAR von Beginn an deine Lieblingseinheiten. Dieses Vorwissen verblasst, sobald VAAR in der laufenden Partie selbst genug gesehen hat. Gegen frühe Angreifer steht der erste Turm früher, gegen Bunkerbauer kommen Fabrik und Artillerie eher, und die Türme zeigen zu deiner gewohnten Flanke. Die Wirtschaft ändert sich dabei nie. Auf „Leicht“ passt sich VAAR nur halb so stark an, nach drei Niederlagen in Folge kaum noch. Über Funk sagt VAAR, was es gelernt hat. Vorhersagen („Wie erwartet …“) kommentiert es nur, wenn sie eingetroffen sind. Am Ende zeigt der Abschlussbildschirm, was VAAR gelernt hat. Der Knopf „VAAR vergessen lassen“ im Hauptmenü löscht das Gedächtnis.
- Grafik:
  - **Gelände:** Relief mit Bergen am Kartenrand. Ein eigener Splat-Shader (`game/assets/shaders/terrain.wesl`) mischt vier echte Bodentexturen (Erde, Geröll, Fels, Gras; CC0, siehe [CREDITS.md](CREDITS.md)) höhenbasiert. Fels liegt dreiplanar auf steilen Flanken, die Kachelung wird durch eine zweite, gedrehte Abtastung aufgebrochen. Ausgefahrene Wege mit Fahrrinnen verbinden die Basen, dazu Granatkrater, verstreute Steine und geschichtete Felsformationen am Kartenrand. Über die Rohstofffelder legt sich ein pulsierender Leuchtschimmer, Staubschwaden ziehen mit dem Wind.
  - **Vegetation:** Zehntausende Grashalme in Büscheln, Büsche und Kiefern, alles prozedural. Der Wind bewegt sie im Vertex-Shader (`wind.wesl`) mit Böen, die über die Ebene wandern.
  - **Fahrzeuge:** Panzer mit keilförmigem Turm, Kommandantenkuppel, Rohr mit Mündungsbremse, Nebelwerfern, Kettenumlauf, Laufrollen, Antriebsrad und Schürzen. Raketenwerfer mit Rohrbündel, Sammler als Sechsrad-Laster mit Erzbunker. Scheinwerfer, rote Rücklichter, eine blinkende Rundumleuchte am Sammler und eine aufgemalte Einheitennummer. Staub, Schmutzflecken, Laufspuren und abgewetzter Lack stecken in den Vertexfarben.
  - **Fahrzeug-Animation:** Räder, Antriebs- und Leiträder drehen sich passend zur gefahrenen Strecke. Ketten hinterlassen Spuren im Sand, die langsam verblassen, schnelle Fahrt wirbelt Staub auf. Beim Schuss springen Rohr und Wanne zurück, beim Anfahren und Bremsen nickt das Fahrzeug. Stark beschädigte Fahrzeuge werden rußgeschwärzt, qualmen und brennen schließlich.
  - **Licht:** Tief stehende, warme Sonne. Der Himmel wirkt als prozedurale Umgebungskarte (`game/src/sky.rs`), die auf der CPU gefiltert wird, weil WebGL2 das nicht kann: blaues Himmelslicht von oben, warmes Bodenlicht, Spiegelungen auf Lack und Glas. Wolkenschatten wandern über Gelände und Vegetation. Die Gebäude haben warm beleuchtete Fenster. Dazu HDR, Bloom, Dunst und Schatten, die sich an den Zoom anpassen.
  - **Bildnachbearbeitung:** SMAA-Kantenglättung, Farbkorrektur, Vignette und eine Tilt-Shift-Unschärfe, deren Schärfeebene dem Zoom folgt.
  - **Effekte:** Partikelsystem aus kamerazugewandten Flächen mit prozedural erzeugten Rauch-, Feuer- und Funkentexturen. Mündungsfeuer mit Staubaufwirbelung, Raketenstart mit Rückstrahl, Einschläge mit Funken, Erdfontäne, Brocken und Brandfleck. Explosionen mit mehrlagigem Feuerball, Druckwelle, Trümmern, die vom Gelände abprallen, Staubring, Rauchsäule und Lichtblitz. Zerstörte Fahrzeuge bleiben als verkohltes Wrack liegen (Turm teils abgesprengt), brennen, qualmen und versinken nach einer Minute.
  - **Waffen:** Jede Waffenklasse hat ihr eigenes Mündungsfeuer und Geschoss. Die Maschinenkanone des Spähers züngelt kurz und wirft Hülsen aus. Das Bollwerk feuert mit langem Feuerstrahl und seitlichen Rauchfahnen aus der Mündungsbremse. Die Haubitze stößt einen Rauchring aus und schlägt eine Druckwelle in den Boden. Messinghülsen springen über den Boden.
  - **Einschläge:** Treffer auf Panzerung sprühen Funken und Querschläger, Treffer auf Gebäude grauen Betonstaub und Splitter. Im Boden werfen Granaten eine Erdfontäne auf, die Haubitze eine hohe Erdsäule mit Druckwelle und großem Krater.
  - **Gebäudeeinsturz:** Zerstörte Gebäude sacken unter mehreren Explosionen ab und kippen. Eine Staubwolke quillt heraus, zurück bleibt ein brennender Trümmerhaufen mit Wandresten und verbogenen Trägern.
  - **Umgebung:** Die Raffinerie dampft, die Fabrikschornsteine rauchen, auf Baustellen sprühen Schweißfunken mit bläulichem Lichtbogen, über den Plasmaquellen steigen violett leuchtende Teilchen auf.
- Leistung: Statisches (Felsen, Vegetation) ist pro Kartenkachel zu einem Mesh zusammengefasst, jedes Fahrzeug besteht aus drei bis vier Meshes. **F3** blendet FPS, Bildzeit und Objektzahl ein und aus.
- Tests: `cargo test` im Ordner `game` lässt die Spiellogik ohne Grafik über simulierte Zeit laufen. Geprüft wird unter anderem, dass Sammler an der nächsten fertigen Raffinerie abladen, dass die KI ihre Gebäude an gültigen Plätzen errichtet, dass ein zerstörtes Gebäude sofort aus dem Spiel ausscheidet, einstürzt und Trümmer hinterlässt, und wie sich die KI verhält: Konter, Flucht der Sammler, Verteidigung, Wellen mit Rückzug, Reparatur, Ausweichen der Haubitzen, Erkundung. Für die Steuerung werden Formation und kreuzungsfreie Platzverteilung, Befehlsschlange, Patrouille, Stellung halten, Gruppentempo, Stopp, Sammelpunkt und Angriffsmeldungen geprüft. Eine ganze Partie läuft dabei im Zeitraffer durch.
- Steuerung (auf dem Mac zählt auch Cmd als Strg):
  - **Auswahl:** Linksklick oder Auswahlbox. Shift ergänzt, Strg+Klick oder Strg+Box nimmt Einheiten heraus. Doppelklick wählt alle eigenen Einheiten dieses Typs auf dem Bildschirm, F alle Kampfeinheiten. Auch eigene Gebäude lassen sich anklicken.
  - **Kontrollgruppen:** Strg+1–9 speichert die Auswahl, Shift+1–9 ergänzt die Gruppe, 1–9 ruft sie ab, zweimal schnell drücken springt mit der Kamera hin.
  - **Befehle:** Rechtsklick bewegt, greift an oder baut ab. Strg+Rechtsklick ist ein Angriffsmarsch, Alt+Rechtsklick eine Patrouille zwischen Standort und Ziel. Mit Shift wird der Befehl angehängt, so entstehen Wegpunkte. R stoppt, T lässt die Einheiten die Stellung halten (sie schießen nur auf Ziele in Reichweite und fahren nicht nach).
  - **Schutzmauern:** K, dann Start- und Endpunkt klicken. Die Linie wird in 2-m-Stücke zerlegt (10 Ferrit je Stück), die Vorschau zeigt grün und rot, welche Stücke passen. Shift+Klick zieht vom Ende aus weiter, Rechtsklick oder Esc verwirft die angefangene Linie. Mit I setzt man ein Tor in eine eigene Mauer (es rastet ein und ersetzt die Stücke an seiner Stelle). Eigene Fahrzeuge fahren durch, der Flügel senkt sich dann in den Boden. Für den Gegner bleibt das Tor zu.
  - **Wegfindung:** Einheiten fahren um Mauern, Gebäude und Steilhänge herum (A* auf einem 1-m-Raster, `game/src/nav.rs`). Ob eine Lücke reicht, hängt von der Fahrzeuggröße ab. Ist ein Ziel ganz eingemauert, schießen Einheiten im Angriff das Mauerstück auf dem günstigsten Weg zusammen und brechen dort durch. Mauern werden sonst nicht von selbst beschossen.
  - **Einknopf-Steuerung** (Taste L, gut fürs Trackpad): Linksklick wählt aus und gibt Befehle. Ein Klick ins Gelände oder auf einen Gegner bewegt bzw. greift an, Strg, Alt und Shift wirken wie beim Rechtsklick. Ein kurzer Rechtsklick hebt die Auswahl auf, Rechts-Ziehen legt weiter eine Frontlinie. Nochmal L schaltet zurück.
  - **Formation:** Gruppen stellen sich am Ziel in Reihen quer zur Marschrichtung auf, ohne dass sich ihre Wege kreuzen, und fahren im Tempo der langsamsten Einheit. Mit gedrückter rechter Maustaste lässt sich eine Frontlinie ziehen. Die Einheiten verteilen sich dann über ihre Länge und blicken von der Gruppe weg nach vorn.
  - **Sammelpunkt:** HQ anklicken, dann Rechtsklick. Neue Kampfeinheiten fahren dorthin. Liegt der Punkt auf einem Rohstofffeld, bauen neue Sammler dort ab.
  - **Rückmeldung:** Befehlsmarker am Boden (grün bewegen, orange Angriffsmarsch, rot Angriff, blau Patrouille, gelb Sammelpunkt). Für die Auswahl werden Wegpunkte, Ziele und Patrouillenstrecken eingezeichnet. Unter dem Mauszeiger leuchten Einheiten, Gegner und Felder auf, der Mauszeiger wechselt zum Fadenkreuz über Gegnern. Über der unteren Leiste zeigt eine Kartenreihe die ausgewählten Einheiten mit Lebensbalken: Klick wählt nur diese Einheit, Shift+Klick nimmt sie heraus. Eine Hinweiszeile nennt die passenden Befehle.
  - **Kamera:** WASD, Pfeiltasten, Bildschirmrand oder mittlere Maustaste ziehen. Q/E drehen, das Mausrad zoomt zum Mauszeiger hin. Auf dem Trackpad verschiebt Wischen mit zwei Fingern die Kamera, Fingerspreizen zoomt. Leertaste springt zum letzten Angriff auf eigene Einheiten (bis 15 Sekunden danach, sonst zur Basis), Pos1 zur Basis.
  - **Minikarte** (unten rechts): Gelände, Felder, Gebäude, Einheiten, Sichtbereich der Kamera und rot blinkende Angriffsmeldungen. Linksklick oder Ziehen bewegt die Kamera, Rechtsklick schickt die Auswahl hin (mit Strg/Alt/Shift wie oben).

## Duell (Mehrspieler 1 gegen 1)

**Bedienung:** Im Menü auf „Duell erstellen“ klicken. Das Spiel zeigt einen Einladungslink (`/play/?duell=<raum>&k=<schlüssel>`), „Link kopieren“ legt ihn in die Zwischenablage. Wer den Link öffnet, tritt bei. Sobald beide da sind, startet die Partie in beiden Browsern gleichzeitig. Der Gastgeber spielt Blau (Team 0), der Gast Rot (Team 1). Kamera, Steuerung und Nebel gelten jeweils für das eigene Team. Im Duell gibt es weder KI (`ai.rs`) noch VAAR (`vaar.rs`) und keine Kampagne. Pause gibt es nicht: Esc öffnet nur das Menü, das Spiel läuft weiter. Dort lässt sich auch aufgeben. Verlässt der Gegner die Partie, gewinnt man.

**Architektur (deterministischer Lockstep):**
- Beide Browser rechnen dieselbe Simulation. Übertragen werden nur Befehle (`game/src/net/cmd.rs`: Bewegen, Angreifen, Abbauen, Halten, Stopp, Sammelpunkt, Produzieren, Bauen, Aufgeben), kompakt binär kodiert.
- Die Simulation läuft in einem eigenen Schedule `SimTick` mit 20 Ticks/s, single-threaded und festem dt (`game/src/sim.rs`). Auch der Einzelspieler nutzt diesen Takt. Für ruckelfreie Bewegung interpoliert die Darstellung zwischen zwei Ticks (`SimXf`).
- Befehle gelten 4 Ticks (200 ms) nach der Eingabe. Ein Tick läuft erst, wenn die Befehle beider Seiten für ihn da sind, sonst erscheint „Warte auf Mitspieler …“ (`game/src/net/mod.rs`, `Lockstep`).
- Entities werden über eine stabile `NetId` angesprochen, nie über die Bevy-`Entity`. Reihenfolgeabhängige Systeme (Bewegung, Ernte, Geschosse, Navigationsraster, Snapshot) laufen nach NetId sortiert. Lokale Marker (`Selected`, `Fogged`, `Lit`) liegen im SparseSet und verändern die Tabellen der Sim-Entities nicht. Die Sicht beider Teams (`SimSight`) und die Halkyon-Uhr gehören zur Simulation, Nebel, Licht und Zeitsprung-Optik (`era.rs`) bleiben lokal.
- Alle 20 Ticks tauschen beide eine FNV-Prüfsumme über den Spielzustand. Bei Abweichung meldet die Browser-Konsole „Desync“. Im Hello wird außerdem eine Build-ID verglichen (`game/build.rs`), damit nur gleiche Versionen gegeneinander spielen.
- Relay: Ein Cloudflare Durable Object `DuellRaum` pro Raum (`web/worker-duell/`) verbindet zwei WebSockets und reicht die Spielnachrichten unverändert weiter. Wer zuerst kommt, legt den Raumschlüssel fest. Ein dritter Spieler oder ein falscher Schlüssel wird abgewiesen. Auf der Website leitet die Pages Function `web/functions/ws/[raum].js` die Adresse `/ws/<raum>` an das Durable Object weiter.

**Tests:** `cd game && cargo test` prüft unter anderem den Roundtrip der Befehlskodierung, zwei Simulationen über 3000 Ticks im Lockstep (eine davon mit „Optik-Rauschen“ wie zusätzlichen Entities und umgeschalteten Markern, die Prüfsummen müssen übereinstimmen), das Warten auf beide Seiten und den Handshake über eine Verbindung im Speicher.

**Lokal testen** (zwei Terminals):

```bash
cd web/worker-duell && npx wrangler dev --port 8787 --ip 127.0.0.1
```

```bash
cd game && trunk serve --port 8080
```

Unter `localhost:8080` verbindet sich das Spiel direkt mit dem Relay auf Port 8787 (`relay_url` in `game/src/net/mod.rs`), auf der Website mit `/ws/` derselben Adresse. Dann http://localhost:8080 öffnen, „Duell erstellen“ klicken und den Link in einem zweiten Fenster öffnen. Beide Fenster sollten im Vordergrund sichtbar bleiben, weil Browser Hintergrund-Tabs stark bremsen.

**Veröffentlichen** (einmalig, vor dem nächsten Pages-Deploy): Zuerst den Worker mit dem Durable Object deployen.

```bash
cd web/worker-duell && npx wrangler deploy
```

Danach kennt das Pages-Projekt die Klasse über die Bindung in `web/wrangler.toml` (`script_name = "ferrum-nova-duell"`). Läuft der Passwortschutz über `functions/_middleware.js`, gilt er auch für `/ws/`. Ob Browser die Basic-Auth-Daten beim WebSocket-Upgrade mitschicken, ist nicht für jeden Browser sicher. Falls die Verbindung mit 401 scheitert, kann die Middleware `/ws/` ausnehmen. Dort schützt dann der Raumschlüssel im Link.

**Einschränkungen (v1):** Kein Wiederverbinden nach einem Abbruch. Ein Hintergrund-Tab hält die eigene Simulation an, dann wartet auch der Mitspieler. Duelle gibt es nur im Browser, nicht in der Desktop-Version.

## Mögliche nächste Schritte

- Baukasten aus Chassis und Waffe, Forschungsbaum
- Pionierfahrzeug, das Gebäude errichtet, und weitere Gebäudetypen (Forschung, Radar)
- Diplomatie mit mehreren KI-Fraktionen
- Duell: Wiederverbinden, Zuschauer, mehr als zwei Spieler
- Texturen als KTX2 (GPU-komprimiert) statt JPG, sobald KTX-Software verfügbar ist
- Eigene Schriftart mit Umlauten für das HUD (die eingebaute Bevy-Schrift kann keine Umlaute)
