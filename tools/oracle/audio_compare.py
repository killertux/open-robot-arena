#!/usr/bin/env python3
"""Original (audio_events.py TSV) against ours (RA2_AUDIO_TRACE=1 log): the same
match, event by event.

    audio_compare.py ORIGINAL.events.tsv OURS.log [--fight-seconds 60]

Both timelines are aligned on the bell (`match_countdown_high.wav`).  The report
lists, in seconds from the bell, the countdown, the Begin_* line, the crowd's
one-shots and the intro lines, then counts per class over the first
`--fight-seconds` of the fight: impacts, arena material sounds, weapon
triggers, motor loop starts, crowd one-shots, immobility ticks.  Random choices
(which announcer line, which impact sample) differ run to run; the timing and
the class counts are what is compared.
"""
import re
import sys
from collections import Counter

CLASSES = [
    ("impact", lambda f: f.startswith(("titanium_hit", "hammer_hit", "bracket_hit", "steel_collision", "rubber_collision", "aluminum_collision", "plastic_collision", "metal_collision"))),
    ("arena material", lambda f: f in ("hzd_spike_hit.wav", "sawblade_hits_loop.wav", "hzd_comp_hit.wav", "hzd_hammer_thud.wav", "cinderblock1.wav")),
    ("piston trigger", lambda f: f == "air_trigger.wav"),
    ("burst trigger", lambda f: f == "burst_trigger.wav"),
    ("motor loop start", lambda f: f.startswith(("motorloop", "bigmotor", "heavy_motor"))),
    ("crowd one-shot", lambda f: f.startswith(("normal_to", "somewhat_to", "mildly_to", "excited_to", "ecstatic_to", "bored_to", "loud_burst", "med_burst", "chant", "stomp", "waytogo", "hitemagain", "youmakemesick", "boo", "comeon"))),
    ("immobile tick", lambda f: f == "immobile_countdown.wav"),
    ("critical hit", lambda f: f == "critical_hit.wav"),
]


def original(path):
    out = []
    for line in open(path).read().splitlines()[1:]:
        f = line.split("\t")
        if f[1] in ("play", "loop") and f[2]:
            if f[1] == "loop" and not f[2].startswith(("motorloop", "bigmotor", "heavy_motor")):
                continue
            out.append((float(f[0]), f[2]))
    return out


def ours(path):
    out = []
    for line in open(path):
        m = re.match(r"audio-trace\s+([0-9.,]+)s\s+(\S+)", line)
        if m:
            out.append((float(m.group(1).replace(",", ".")), m.group(2)))
    return out


def align(events):
    bell = [t for t, f in events if f == "match_countdown_high.wav"]
    return bell[0] if bell else 0.0


def main():
    a = original(sys.argv[1])
    b = ours(sys.argv[2])
    span = float(sys.argv[sys.argv.index("--fight-seconds") + 1]) if "--fight-seconds" in sys.argv else 60.0
    ta, tb = align(a), align(b)
    print(f"{'event':34s}{'original':>10s}{'ours':>10s}   (s from the bell)")
    names = ["match_countdown.wav", "match_countdown_high.wav"]
    for n in names:
        xa = [round(t - ta, 2) for t, f in a if f == n]
        xb = [round(t - tb, 2) for t, f in b if f == n]
        print(f"{n:34s}{str(xa[:4]):>10s}{str(xb[:4]):>10s}")
    for prefix in ("Begin_", "Arena_", "Intro_", "Hazard_", "Bots_", "End_"):
        xa = [round(t - ta, 2) for t, f in a if f.startswith(prefix)]
        xb = [round(t - tb, 2) for t, f in b if f.startswith(prefix)]
        print(f"{prefix + '*':34s}{str(xa[:2]):>10s}{str(xb[:2]):>10s}")
    for n in ("normal_to_somewhat.wav", "somewhat_to_bored.wav", "loud_burstcheer.wav"):
        xa = [round(t - ta, 2) for t, f in a if f == n]
        xb = [round(t - tb, 2) for t, f in b if f == n]
        print(f"{n:34s}{str(xa[:5]):>10s}{str(xb[:5]):>10s}")
    print()
    print(f"{'class (first %d s of the fight)' % span:34s}{'original':>10s}{'ours':>10s}")
    for label, pred in CLASSES:
        ca = sum(1 for t, f in a if ta <= t < ta + span and pred(f))
        cb = sum(1 for t, f in b if tb <= t < tb + span and pred(f))
        print(f"{label:34s}{ca:>10d}{cb:>10d}")


if __name__ == "__main__":
    main()
