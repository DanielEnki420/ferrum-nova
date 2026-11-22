#!/usr/bin/env python3
"""Erzeugt alle Geräusche und die Musik von Ferrum Nova synthetisch (keine fremden Aufnahmen).

Aufruf:  python3 tools/make_sounds.py            alle Fassungen
         python3 tools/make_sounds.py y2000      nur die 2000-Fassung (ebenso: now, retro)
Ergebnis: assets/sounds/*.ogg, assets/sounds/y2000/*.ogg (2000-Fassung) und assets/sounds/retro/*.ogg
(1996-Fassung)  (braucht ffmpeg zum Umwandeln von WAV nach Ogg Vorbis)

Der Zufall ist fest geseedet; jede Fassung wird aus denselben Rohklängen abgeleitet, auch wenn nur
eine davon geschrieben wird.

Nur Standardbibliothek: Schwingungen, Rauschen, einfache Filter und Hüllkurven.
"""

import math
import os
import random
import struct
import subprocess
import sys
import tempfile
import wave

SR = 22050
TAU = 2 * math.pi
OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "sounds")
rnd = random.Random(7)
# Welche Fassungen geschrieben werden (Kommandozeile), sonst alle
ONLY = set(sys.argv[1:]) or {"now", "y2000", "retro"}


def buf(seconds):
    return [0.0] * int(seconds * SR)


def lp_coeff(fc):
    return 1.0 - math.exp(-TAU * max(fc, 1.0) / SR)


def add_noise(out, start, dur, amp_env, cutoff, hp=0.0, gain=1.0):
    """Gefiltertes Rauschen: Tiefpass mit zeitabhängiger Grenzfrequenz, optional Hochpass"""
    y = 0.0
    low = 0.0
    n0 = int(start * SR)
    for i in range(int(dur * SR)):
        t = i / SR
        x = rnd.uniform(-1, 1)
        y += lp_coeff(cutoff(t)) * (x - y)
        s = y
        if hp > 0:
            low += lp_coeff(hp) * (s - low)
            s -= low
        if n0 + i < len(out):
            out[n0 + i] += s * amp_env(t) * gain


def add_tone(out, start, dur, freq, amp_env, harmonics=((1, 1.0),), gain=1.0):
    """Ton mit Tonhöhenverlauf freq(t) und Obertönen (Vielfaches, Lautstärke)"""
    phase = 0.0
    n0 = int(start * SR)
    for i in range(int(dur * SR)):
        t = i / SR
        phase += TAU * freq(t) / SR
        s = 0.0
        for h, a in harmonics:
            s += a * math.sin(phase * h)
        if n0 + i < len(out):
            out[n0 + i] += s * amp_env(t) * gain


def sweep(a, b, k):
    """Exponentieller Übergang von a nach b mit Rate k"""
    return lambda t: b + (a - b) * math.exp(-t * k)


def decay(k, attack=0.002):
    return lambda t: min(1.0, t / attack) * math.exp(-t * k)


def echo(out, delay, feedback, mix):
    d = int(delay * SR)
    res = out[:]
    for i in range(d, len(res)):
        res[i] += res[i - d] * feedback
    return [a * (1 - mix) + b * mix for a, b in zip(out, res)]


def normalize(out, peak=0.89, fade_out=True):
    m = max(1e-9, max(abs(s) for s in out))
    # sanfte Sättigung gegen Spitzen, dann auf Zielpegel
    out = [math.tanh(s / m * 1.4) for s in out]
    m = max(1e-9, max(abs(s) for s in out))
    fade = int(0.01 * SR) if fade_out else 0
    for i in range(min(fade, len(out))):
        out[-1 - i] *= i / fade
    return [s / m * peak for s in out]


def write_ogg(dst, out, quality):
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
        tmp = f.name
    with wave.open(tmp, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1, min(1, s)) * 32767)) for s in out))
    subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-y", "-i", tmp, "-ac", "2", "-c:a", "vorbis", "-strict", "-2", "-q:a", str(quality), dst],
        check=True,
    )
    os.unlink(tmp)


def crush(out, hold=3, levels=48):
    """Klang einer Soundkarte von 1996: niedrige Abtastrate (Halteglied) und grobe Stufen"""
    res = []
    v = 0.0
    for i, s in enumerate(out):
        if i % hold == 0:
            v = round(s * levels) / levels
        res.append(v)
    return res


def dull(out, cutoff=4200, room=0.12):
    """Klang einer Soundkarte um 2000: 22 kHz / 16 Bit, Höhen gedämpft (zweifacher Tiefpass),
    dazu ein kurzer, kleiner Raumhall wie bei den Hall-Effekten der damaligen Karten"""
    a = lp_coeff(cutoff)
    y1 = y2 = 0.0
    res = []
    for s in out:
        y1 += a * (s - y1)
        y2 += a * (y1 - y2)
        res.append(y2)
    if room > 0:
        res = echo(res, 0.045, 0.25, room)
    m = max(1e-9, max(abs(s) for s in out))
    r = max(1e-9, max(abs(s) for s in res))
    return [s / r * m for s in res]


def put(variant, dst, out, quality):
    if variant in ONLY:
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        write_ogg(dst, out, quality)
        print(f"{os.path.relpath(dst, OUT):24s} {len(out) / SR:5.2f} s  {os.path.getsize(dst) // 1024:4d} KB")


def save(name, out, quality=3, retro=True):
    """Speichert den Klang; `retro` legt zusätzlich die Fassungen für 2000 (sounds/y2000) und
    1996 (sounds/retro) ab"""
    put("now", os.path.join(OUT, name + ".ogg"), out, quality)
    if retro:
        put("y2000", os.path.join(OUT, "y2000", name + ".ogg"), dull(out), quality)
        put("retro", os.path.join(OUT, "retro", name + ".ogg"), crush(out), quality)


# ------------------------------------------------------------------ Waffen


def cannon(name, f0, f1, length, body, crack_cut):
    o = buf(length)
    add_tone(o, 0, length, sweep(f0, f1, 10), decay(body), ((1, 1.0), (2, 0.3)))
    add_noise(o, 0, length, decay(26), sweep(crack_cut, 400, 18), gain=0.9)
    add_noise(o, 0, length, decay(5), lambda t: 260, gain=0.8)
    save(name, normalize(echo(o, 0.11, 0.3, 0.25)))


def mg():
    o = buf(0.11)
    add_noise(o, 0, 0.11, decay(55), sweep(5000, 900, 40))
    add_tone(o, 0, 0.11, sweep(700, 220, 50), decay(70), gain=0.5)
    save("shot_light", normalize(o, 0.75))


def artillery():
    o = buf(1.3)
    add_tone(o, 0, 1.3, sweep(75, 28, 6), decay(4.5), ((1, 1.0), (2, 0.25)))
    add_noise(o, 0, 1.3, decay(14), sweep(2200, 300, 9), gain=0.8)
    add_noise(o, 0, 1.3, decay(3.2), lambda t: 180, gain=0.9)
    save("shot_artillery", normalize(echo(o, 0.19, 0.35, 0.35)))


def rocket():
    o = buf(0.9)
    env = lambda t: min(1, t / 0.025) * math.exp(-t * 3.5)
    add_noise(o, 0, 0.9, env, sweep(900, 3200, 3), hp=500, gain=1.0)
    add_noise(o, 0, 0.9, decay(20), lambda t: 1500, gain=0.6)
    save("rocket_launch", normalize(o, 0.8))


# ------------------------------------------------------------------ Treffer und Explosionen


def impacts():
    o = buf(0.35)
    add_noise(o, 0, 0.35, decay(14), sweep(1400, 200, 10))
    add_tone(o, 0, 0.35, sweep(95, 45, 12), decay(16), gain=0.7)
    save("impact_ground", normalize(o, 0.75))

    o = buf(0.5)
    for f, a, k in ((523, 0.5, 9), (1307, 0.6, 12), (2011, 0.4, 15), (2780, 0.3, 20), (3570, 0.2, 26)):
        add_tone(o, 0, 0.5, lambda t, f=f: f, decay(k, 0.001), gain=a)
    add_noise(o, 0, 0.5, decay(60), lambda t: 6000, gain=0.8)
    save("impact_armor", normalize(o, 0.7))


def explosion(name, length, k, cut0, sub0, crackles):
    o = buf(length)
    env = lambda t: min(1, t / 0.006) * math.exp(-t * k)
    add_noise(o, 0, length, env, sweep(cut0, 120, k * 0.9))
    add_noise(o, 0, length, lambda t: env(t) * 0.7, lambda t: 140)
    add_tone(o, 0, length, sweep(sub0, 26, 3), decay(k * 1.1), gain=0.9)
    # Knistern: kurze Splitter in der Abklingphase
    for _ in range(crackles):
        s = rnd.uniform(0.05, length * 0.7)
        add_noise(o, s, 0.03, decay(120, 0.001), lambda t: 4000, gain=0.35 * math.exp(-s * k * 0.6))
    save(name, normalize(echo(o, 0.23, 0.3, 0.3)))


# ------------------------------------------------------------------ Bauen und Oberfläche


def bell(o, start, f, length, gain=1.0):
    add_tone(o, start, length, lambda t: f, decay(5, 0.004), ((1, 1.0), (2.0, 0.35), (3.01, 0.15), (4.2, 0.08)), gain)


def interface():
    o = buf(0.5)
    for s in (0.0, 0.13):
        add_tone(o, s, 0.3, sweep(260, 180, 20), decay(28), ((1, 1.0), (1.5, 0.5), (2.7, 0.3)))
        add_noise(o, s, 0.2, decay(45), lambda t: 2500, gain=0.7)
    save("build_place", normalize(o, 0.7))

    o = buf(1.1)
    bell(o, 0.0, 659.3, 1.1)
    bell(o, 0.16, 987.8, 0.95)
    save("build_done", normalize(o, 0.6))

    o = buf(0.6)
    for i, f in enumerate((1046.5, 1318.5, 1568.0)):
        add_tone(o, i * 0.09, 0.3, lambda t, f=f: f, decay(16, 0.003), ((1, 1.0), (3, 0.18)))
    save("unit_ready", normalize(o, 0.55))

    o = buf(0.05)
    add_tone(o, 0, 0.05, lambda t: 1900, decay(140, 0.0005))
    add_noise(o, 0, 0.05, decay(200, 0.0005), lambda t: 7000, gain=0.4)
    save("ui_click", normalize(o, 0.5))

    # Funk-Bestätigung: Rauschen wie beim Sprechfunk und zwei kurze Töne
    o = buf(0.22)
    add_noise(o, 0, 0.22, lambda t: 0.25 if t < 0.2 else 0.0, lambda t: 3500, hp=900)
    add_tone(o, 0.02, 0.06, lambda t: 1250, lambda t: 1.0, ((1, 1.0), (3, 0.2)))
    add_tone(o, 0.10, 0.07, lambda t: 1660, lambda t: 1.0, ((1, 1.0), (3, 0.2)))
    save("ack", normalize(o, 0.45))

    o = buf(0.1)
    add_tone(o, 0, 0.1, lambda t: 980, decay(30, 0.002), ((1, 1.0), (2, 0.2)))
    save("select", normalize(o, 0.4))

    o = buf(0.32)
    add_tone(o, 0, 0.32, lambda t: 150, lambda t: 1.0 if t < 0.28 else 0.0, ((1, 1.0), (3, 0.33), (5, 0.2), (7, 0.14)))
    save("denied", normalize(o, 0.5))

    # Alarm: wechselnde Töne
    o = buf(1.05)
    for i in range(5):
        f = 740 if i % 2 == 0 else 554
        add_tone(o, i * 0.2, 0.19, lambda t, f=f: f, lambda t: min(1, t / 0.01) * (1 if t < 0.17 else 0), ((1, 1.0), (3, 0.3), (5, 0.12)))
    save("alert", normalize(o, 0.55))


def stings():
    o = buf(3.4)
    for i, f in enumerate((523.3, 659.3, 784.0, 1046.5)):
        bell(o, i * 0.16, f, 2.8, 0.8)
    for f in (261.6, 392.0, 523.3):
        add_tone(o, 0.5, 2.9, lambda t, f=f: f, lambda t: min(1, t / 0.4) * math.exp(-t * 1.1), ((1, 1.0), (2, 0.3)), 0.35)
    save("victory", normalize(echo(o, 0.3, 0.3, 0.3), 0.7))

    o = buf(3.6)
    for i, f in enumerate((440.0, 349.2, 293.7)):
        add_tone(o, i * 0.55, 1.6, lambda t, f=f: f * (1 - 0.01 * t), lambda t: min(1, t / 0.02) * math.exp(-t * 2.2), ((1, 1.0), (2, 0.4), (3, 0.2)), 0.8)
    add_tone(o, 0.0, 3.6, lambda t: 73.4, lambda t: min(1, t / 0.5) * math.exp(-t * 0.8), ((1, 1.0), (2, 0.5), (3, 0.25)), 0.6)
    save("defeat", normalize(echo(o, 0.35, 0.35, 0.35), 0.7))


# ------------------------------------------------------------------ Musik


def note(midi):
    return 440.0 * 2 ** ((midi - 69) / 12)


def music():
    """Ruhige, dunkle Schleife: Bordun auf D, langsam wechselnde Flächen, vereinzelte Zupftöne"""
    loop = 64.0
    tail = 4.0
    o = buf(loop + tail)
    n = len(o)
    # Bordun D2 + A2, leicht schwebend
    for f, g in ((note(38), 0.55), (note(45), 0.3), (note(38) * 1.003, 0.25)):
        add_tone(o, 0, loop + tail, lambda t, f=f: f, lambda t: 0.8 + 0.2 * math.sin(TAU * t / 16), ((1, 1.0), (2, 0.35), (3, 0.12)), g)
    # Flächen: Dm – B – F – C, je 8 s, weiche Ein- und Ausblendung
    chords = [(62, 65, 69), (58, 62, 65), (60, 65, 69), (60, 64, 67)] * 2
    for i, ch in enumerate(chords):
        start = i * 8.0 - 1.0
        length = 10.0
        env = lambda t: math.sin(math.pi * min(1, max(0, t / length))) ** 2
        for m in ch:
            for det in (0.997, 1.003):
                s0 = max(0.0, start)
                add_tone(o, s0, length - (s0 - start), lambda t, m=m, det=det: note(m - 12) * det, lambda t, s=s0 - start: env(t + s), ((1, 1.0), (2, 0.22), (3, 0.08)), 0.11)
    # Zupftöne in D-Moll-Pentatonik, mit Echo
    scale = [62, 65, 67, 69, 72, 74, 77]
    pl = buf(loop + tail)
    t = 2.0
    while t < loop - 1.0:
        m = rnd.choice(scale)
        add_tone(pl, t, 1.6, lambda x, m=m: note(m), decay(3.2, 0.003), ((1, 1.0), (2, 0.3), (3, 0.1)), 0.22)
        t += rnd.choice((0.75, 1.5, 1.5, 2.25, 3.0))
    pl = echo(pl, 0.75, 0.45, 0.5)
    o = [a + b for a, b in zip(o, pl)]
    # Puls: dumpfer, leiser Schlag auf jeder zweiten Zählzeit (60 BPM)
    beat = 0.0
    while beat < loop:
        add_tone(o, beat, 0.5, sweep(70, 40, 12), decay(9), gain=0.35)
        beat += 2.0
    # Nahtlose Schleife: Ende in den Anfang überblenden
    L = int(loop * SR)
    for i in range(n - L):
        k = i / (n - L)
        o[i] = o[i] * k + o[L + i] * (1 - k)
    save("music", normalize(o[:L], 0.6, fade_out=False), quality=2, retro=False)


def pulse(o, start, dur, freq, gain, duty=0.5, env=lambda t: 1.0):
    """Rechteckwelle wie aus einem Soundchip"""
    n0 = int(start * SR)
    phase = 0.0
    for i in range(int(dur * SR)):
        t = i / SR
        phase = (phase + freq / SR) % 1.0
        if n0 + i < len(o):
            o[n0 + i] += (1.0 if phase < duty else -1.0) * gain * env(t)


def music_retro():
    """Dieselbe Akkordfolge als Chipmusik: Arpeggios, Rechteck-Bass, Rauschen als Schlagzeug"""
    loop = 64.0
    o = buf(loop)
    chords = [(62, 65, 69), (58, 62, 65), (60, 65, 69), (60, 64, 67)] * 2
    step = 1 / 16  # schnelle Arpeggios wie im Tracker
    for i, ch in enumerate(chords):
        t0 = i * 8.0
        k = 0
        t = t0
        while t < t0 + 8.0 - 1e-6:
            m = ch[k % 3] + (12 if (k // 3) % 2 else 0)
            pulse(o, t, step, note(m), 0.07, 0.25, lambda x: 1.0 - x / step * 0.5)
            t += step
            k += 1
        # Bass: Grundton in Achteln, auf und ab
        for j in range(16):
            m = ch[0] - 24 + (12 if j % 2 else 0)
            pulse(o, t0 + j * 0.5, 0.42, note(m), 0.11, 0.5, decay(2.5, 0.003))
    # Melodie: einfache Figur in D-Moll, jede zweite Runde
    mel = [74, 72, 69, 72, 74, 77, 76, 72, 70, 69, 65, 67, 69, 67, 65, 62]
    for r in (1, 3, 5, 7):
        for j, m in enumerate(mel):
            pulse(o, r * 8.0 + j * 0.5, 0.45, note(m), 0.06, 0.125, decay(1.6, 0.004))
    # Schlagzeug: Rauschen kurz (Hi-Hat) und lang (Snare), tiefer Puls als Bass-Trommel
    beat = 0.0
    while beat < loop:
        add_tone(o, beat, 0.2, sweep(140, 45, 25), decay(18), gain=0.35)
        add_noise(o, beat + 0.5, 0.06, decay(60), lambda t: 9000, gain=0.12)
        if int(beat) % 2 == 1:
            add_noise(o, beat, 0.18, decay(18), lambda t: 5000, gain=0.25)
        beat += 1.0
    o = crush(o, 2, 64)
    put("retro", os.path.join(OUT, "retro", "music.ogg"), normalize(o, 0.55, fade_out=False), 2)


def saw(o, start, dur, freq, gain, env=lambda t: 1.0, partials=8):
    """Sägezahn mit wenigen Obertönen, wie ein einfacher Synthesizer-Klang einer Soundkarte"""
    add_tone(o, start, dur, lambda t: freq, env, tuple((k, 1.0 / k) for k in range(1, partials + 1)), gain)


def music_y2000():
    """Dieselbe Akkordfolge als vereinfachte Musik um 2000: Synthesizer-Flächen, Sägezahn-Bass,
    Schlagzeug-Schleife mit 120 BPM und eine schlichte Melodie, alles mit gedämpften Höhen"""
    loop = 64.0
    o = buf(loop)
    chords = [(62, 65, 69), (58, 62, 65), (60, 65, 69), (60, 64, 67)] * 2
    beat = 0.5  # 120 BPM
    for i, ch in enumerate(chords):
        t0 = i * 8.0
        # Fläche: je Akkord ein gehaltener Klang mit weichem Ein- und Ausklang
        env = lambda t: min(1.0, t / 0.6) * min(1.0, (8.0 - t) / 0.6)
        for m in ch:
            saw(o, t0, 8.0, note(m - 12), 0.045, env, partials=5)
        # Bass: Grundton in Achteln, jede vierte Note eine Oktave höher
        for j in range(32):
            m = ch[0] - 24 + (12 if j % 4 == 3 else 0)
            saw(o, t0 + j * beat / 2, beat / 2 * 0.9, note(m), 0.09, decay(5.0, 0.004), partials=6)
    # Melodie: dieselbe Figur wie in der Chipmusik, aber halb so schnell und weicher
    mel = [74, 72, 69, 72, 74, 77, 76, 72]
    for r in (2, 3, 6, 7):
        for j, m in enumerate(mel):
            add_tone(o, r * 8.0 + j * 1.0, 0.95, lambda t, m=m: note(m), decay(2.2, 0.01), ((1, 1.0), (2, 0.25), (3, 0.3), (5, 0.1)), 0.07)
    # Schlagzeug: Bass-Trommel auf 1 und 3, Snare auf 2 und 4, geschlossene Hi-Hat in Achteln
    k = 0
    t = 0.0
    while t < loop - 1e-6:
        if k % 2 == 0:
            add_tone(o, t, 0.25, sweep(120, 45, 22), decay(14), gain=0.4)
        else:
            add_noise(o, t, 0.2, decay(20), lambda x: 4500, hp=300, gain=0.22)
            add_tone(o, t, 0.12, sweep(220, 160, 30), decay(30), gain=0.12)
        add_noise(o, t, 0.05, decay(70), lambda x: 9000, hp=5000, gain=0.07)
        add_noise(o, t + beat / 2, 0.05, decay(70), lambda x: 9000, hp=5000, gain=0.05)
        t += beat
        k += 1
    o = dull(o, cutoff=5500, room=0.15)
    put("y2000", os.path.join(OUT, "y2000", "music.ogg"), normalize(o, 0.55, fade_out=False), 2)


if __name__ == "__main__":
    cannon("shot_medium", 150, 50, 0.5, 9, 3200)
    cannon("shot_heavy", 110, 34, 0.75, 6, 2400)
    mg()
    artillery()
    rocket()
    impacts()
    explosion("explosion_small", 1.3, 3.8, 1900, 70, 8)
    explosion("explosion_big", 2.8, 1.7, 1400, 55, 22)
    interface()
    stings()
    music()
    music_retro()
    music_y2000()
