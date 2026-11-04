#!/usr/bin/env python3
"""Per-file sound-call diff, original (audio_events.py TSV) against ours (RA2_AUDIO_TRACE=1 log).

    audio_filediff.py ORIGINAL.events.tsv OURS.log [--fight-seconds 60]

Both are aligned on the bell (`match_countdown_high.wav`).  For every file named
in either log: how many play/loop starts before the bell (the intro) and in the
first `--fight-seconds` after it, and the time of the first one from the bell.
Looped files count once per `loop` call.  A `*` marks a file the original plays
and ours does not (or the reverse); random picks (which announcer, which impact
sample) show up as paired rows, so read them in groups.
"""
import re
import sys
from collections import defaultdict


# The crowd's 22 sounds in creation order (`0x4d46e0`).  The call log sometimes decodes
# their file names to garbage ("JEM"); the creation order names them.
CROWD = ["bored_loop", "somewhat_bored_loop", "normal_loop", "mildly_excited_loop", "excited_loop",
         "loud_loop", "somewhat_to_bored", "normal_to_somewhat", "mildly_to_normal",
         "excited_to_mildly", "ecstatic_to_excited", "loud_burstcheer", "med_burstcheer",
         "somewhat_to_normal", "bored_to_somewhat", "chant", "stompstompclap", "waytogo",
         "hitemagain", "youmakemesick", "boo", "comeon"]


def original(path):
    out = []
    rows = [line.split("\t") for line in open(path).read().splitlines()[1:]]
    garbled = {}
    for f in rows:
        if f[1] == "create" and f[2] == "JEM" and len(garbled) < len(CROWD):
            garbled[f[4]] = CROWD[len(garbled)] + ".wav"
    for f in rows:
        if f[1] in ("play", "loop") and f[2]:
            name = garbled.get(f[4], f[2]) if f[2] == "JEM" else f[2]
            out.append((float(f[0]), name, f[1]))
    return out


def ours(path):
    out = []
    for line in open(path):
        m = re.match(r"audio-trace\s+(-?[0-9.,]+)s\s+(\S+)\s+hdb=(-?\d+)( loop)?", line)
        if m:
            out.append((float(m.group(1).replace(",", ".")), m.group(2), "loop" if m.group(4) else "play"))
    return out


def bell(ev):
    t = [t for t, f, _ in ev if f == "match_countdown_high.wav"]
    return t[0] if t else 0.0


def main():
    a, b = original(sys.argv[1]), ours(sys.argv[2])
    span = float(sys.argv[sys.argv.index("--fight-seconds") + 1]) if "--fight-seconds" in sys.argv else 60.0
    ta, tb = bell(a), bell(b)
    rows = defaultdict(lambda: [[0, 0], [0, 0], [None, None]])
    for side, ev, t0 in ((0, a, ta), (1, b, tb)):
        for t, f, kind in ev:
            r = t - t0
            if r < 0:
                rows[f][0][side] += 1
            elif r < span:
                rows[f][1][side] += 1
            else:
                continue
            if rows[f][2][side] is None:
                rows[f][2][side] = r
    print(f"{'file':38s}{'intro o/u':>10s}{'fight o/u':>11s}{'first o':>9s}{'first u':>9s}")
    for f in sorted(rows, key=lambda f: min(x for x in rows[f][2] if x is not None)):
        (i0, i1), (f0, f1), (s0, s1) = rows[f]
        mark = "*" if (i0 + f0 == 0) != (i1 + f1 == 0) else " "
        fa = "-" if s0 is None else f"{s0:.1f}"
        fb = "-" if s1 is None else f"{s1:.1f}"
        print(f"{mark}{f:37s}{i0:>5d}/{i1:<4d}{f0:>6d}/{f1:<4d}{fa:>9s}{fb:>9s}")


if __name__ == "__main__":
    main()
