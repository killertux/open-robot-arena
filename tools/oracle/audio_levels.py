#!/usr/bin/env python3
"""Per-class sound LEVELS, the original's call log against ours (docs/35 section 13).

    audio_levels.py ORIGINAL.events.tsv OURS.log [--fight-seconds 60]

The original side is every `play` / `loop` row (file, the handle's volume in hundredths of a
dB at that moment: `SoundWizard` volume reaches the DirectSound buffer unchanged, `B` rows) in the
window; ours is the `hdb=` field of each `RA2_AUDIO_TRACE=1` start. Both windows start at
the bell (`match_countdown_high.wav`).  For each class: count, mean / min / max level and
the share louder than -900 (the impact distribution of docs/35 section 6).
"""
import re
import sys

CLASSES = [
    ("impact", ("titanium_hit", "hammer_hit", "bracket_hit", "steel_collision", "rubber_collision", "aluminum_collision", "plastic_collision", "metal_collision")),
    ("arena material", ("hzd_spike_hit", "sawblade_hits_loop", "hzd_comp_hit", "hzd_hammer_thud", "cinderblock1")),
    ("piston trigger", ("air_trigger",)),
    ("burst trigger", ("burst_trigger",)),
    ("motor loop", ("motorloop", "bigmotor", "heavy_motor")),
    ("crowd", ("normal_", "somewhat_", "mildly_", "excited_", "ecstatic_", "bored_", "loud_", "med_burst", "chant", "stomp")),
]


def klass(name):
    n = name.lower()
    for label, prefixes in CLASSES:
        if n.startswith(prefixes):
            return label
    return None


def original(path):
    """One row per start (`play` / `loop`), at the handle's level then (create, then SetVolume)."""
    rows = []
    name, vol = {}, {}
    for line in open(path).read().splitlines()[1:]:
        f = line.split("\t")
        h = f[4]
        if f[1] == "create" and f[2]:
            name[h], vol[h] = f[2], float(f[3]) if f[3] else 0.0
        elif f[1] == "setvolume" and h in vol and f[3]:
            vol[h] = float(f[3])
        elif f[1] in ("play", "loop") and h in name:
            rows.append((float(f[0]), name[h], vol[h]))
    return rows


def ours(path):
    rows = []
    for line in open(path):
        m = re.match(r"audio-trace\s+([0-9.,]+)s\s+(\S+)\s+hdb=(-?[0-9.]+)", line)
        if m:
            rows.append((float(m.group(1).replace(",", ".")), m.group(2), float(m.group(3))))
    return rows


def bell(rows):
    for t, f, _ in rows:
        if f == "match_countdown_high.wav":
            return t
    return 0.0


def table(rows, span):
    t0 = bell(rows)
    out = {}
    for t, f, v in rows:
        k = klass(f)
        if k and 0 <= t - t0 <= span:
            out.setdefault(k, []).append(v)
    return out


def main():
    span = float(sys.argv[sys.argv.index("--fight-seconds") + 1]) if "--fight-seconds" in sys.argv else 60.0
    a, b = table(original(sys.argv[1]), span), table(ours(sys.argv[2]), span)
    print(f"{'class':16} {'':>9} {'n':>5} {'mean':>7} {'min':>7} {'max':>7} {'>-900':>7}")
    for label, _ in CLASSES:
        for tag, d in (("original", a), ("ours", b)):
            v = d.get(label, [])
            if v:
                loud = 100.0 * sum(1 for x in v if x > -900) / len(v)
                print(f"{label:16} {tag:>9} {len(v):5d} {sum(v)/len(v):7.0f} {min(v):7.0f} {max(v):7.0f} {loud:6.1f}%")
            else:
                print(f"{label:16} {tag:>9} {0:5d}")


if __name__ == "__main__":
    main()
