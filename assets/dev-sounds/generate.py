"""Generate placeholder sound effects for development.

These are synthesised, not recorded: they exist so the sound engine, groups and
anti-repeat logic can be exercised before a real library is imported. They are
deliberately crude and are not shipped content.
"""
import math, os, random, struct, wave

SR = 44100
OUT = "assets/dev-sounds"

def write(name, samples):
    path = os.path.join(OUT, name)
    peak = max(1e-9, max(abs(s) for s in samples))
    with wave.open(path, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR)
        w.writeframes(b"".join(
            struct.pack("<h", int(max(-1.0, min(1.0, s / peak * 0.85)) * 32767))
            for s in samples))
    print(f"{name}  {len(samples)/SR:.2f}s  {os.path.getsize(path)//1024} KB")

def env(i, n, attack=0.01, release=0.3):
    a, r = int(n * attack), int(n * release)
    if i < a: return i / max(1, a)
    if i > n - r: return (n - i) / max(1, r)
    return 1.0

class LowPass:
    """One-pole low-pass, used to shape white noise into something less harsh."""
    def __init__(self, cutoff): self.a = math.exp(-2 * math.pi * cutoff / SR); self.y = 0.0
    def __call__(self, x): self.y = (1 - self.a) * x + self.a * self.y; return self.y

def door_creak(seed, dur=1.4, base=180.0):
    rnd = random.Random(seed); n = int(SR * dur); out = []; phase = 0.0
    lp = LowPass(2500)
    for i in range(n):
        t = i / n
        # A creak is a slow stick-slip oscillation that rises then dies.
        freq = base * (1 + 1.4 * t) * (1 + 0.12 * math.sin(2 * math.pi * 7 * t * (1 + seed % 3)))
        phase += 2 * math.pi * freq / SR
        saw = 2 * ((phase / (2 * math.pi)) % 1.0) - 1
        grit = lp(rnd.uniform(-1, 1)) * 0.5
        out.append((saw * 0.5 + grit) * env(i, n, 0.05, 0.45) * (0.4 + 0.6 * (1 - t)))
    return out

def thunder(seed, dur=3.2):
    rnd = random.Random(seed); n = int(SR * dur); out = []
    lp1, lp2 = LowPass(120), LowPass(400)
    for i in range(n):
        t = i / n
        noise = rnd.uniform(-1, 1)
        body = lp1(noise) * 1.8 + lp2(noise) * 0.4
        crack = math.exp(-t * 22) * rnd.uniform(-1, 1) * 0.8
        rumble = 1.0 if t < 0.08 else math.exp(-(t - 0.08) * 2.2)
        out.append((body * rumble + crack) * env(i, n, 0.002, 0.35))
    return out

def sword_swing(seed, dur=0.55):
    rnd = random.Random(seed); n = int(SR * dur); out = []
    lp = LowPass(6000)
    for i in range(n):
        t = i / n
        # Whoosh: band of noise that sweeps up then away as the blade passes.
        sweep = math.sin(math.pi * t) ** 2
        noise = lp(rnd.uniform(-1, 1))
        tone = math.sin(2 * math.pi * (600 + 2200 * sweep) * i / SR) * 0.15 * sweep
        out.append((noise * sweep * 1.2 + tone) * env(i, n, 0.08, 0.5))
    return out

def wolf_growl(seed, dur=1.8):
    rnd = random.Random(seed); n = int(SR * dur); out = []
    lp = LowPass(900); phase = 0.0
    for i in range(n):
        t = i / n
        freq = 90 * (1 + 0.25 * math.sin(2 * math.pi * 5.5 * t))
        phase += 2 * math.pi * freq / SR
        buzz = math.sin(phase) + 0.5 * math.sin(2 * phase) + 0.3 * math.sin(3 * phase)
        out.append((buzz * 0.5 + lp(rnd.uniform(-1, 1)) * 0.6) * env(i, n, 0.12, 0.3))
    return out

def rain_loop(seed, dur=6.0):
    """Loopable: the last 0.25 s crossfades into the first, so there is no seam."""
    rnd = random.Random(seed); n = int(SR * dur)
    lp1, lp2 = LowPass(7000), LowPass(1200)
    raw = []
    for _ in range(n):
        noise = rnd.uniform(-1, 1)
        raw.append(lp1(noise) * 0.8 + lp2(noise) * 0.5)
    fade = int(SR * 0.25)
    for i in range(fade):
        k = i / fade
        raw[i] = raw[i] * k + raw[n - fade + i] * (1 - k)
    return raw[:n - fade]

os.makedirs(OUT, exist_ok=True)
for i in range(1, 4):
    write(f"door_wood_creak_0{i}.wav", door_creak(i, base=150 + 40 * i))
for i in range(1, 3):
    write(f"thunder_0{i}.wav", thunder(10 + i))
for i in range(1, 4):
    write(f"sword_swing_0{i}.wav", sword_swing(20 + i))
write("wolf_growl_01.wav", wolf_growl(30))
write("rain_loop_01.wav", rain_loop(40))
