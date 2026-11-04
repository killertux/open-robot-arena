#!/usr/bin/env python3
"""Impact sounds against the collision events that caused them (docs/35 section 15).

    impactsound.py CAPTURE.jsonl [...]         (captures of frida/ra2_impactsound.js)
    impactsound.py --tsv CAPTURE.jsonl [...]   (write CAPTURE.impacts.tsv for the ra2-app tests)

Per capture, from the bell (`match_countdown_high.wav`) on:
  * every `IMP` row (0x45f3a0 entry: the struck component's hit handler, one per priced
    event with a non-zero amount) and whether a sound was created at that instant
    (a `C` row at the same instant (a frame later at most: -5..60 ms)) and at which volume, against the volume rule
    (damage 4 -> -1500, 100 -> 0, linear; silent at 4 or less);
  * the counts by amount bin, the crowd's excitement steps (`EX`), mood changes and shouts.
"""
import json
import sys
from collections import Counter


def vol(h):
    v = int(h, 16)
    return v - (1 << 32) if v >= 1 << 31 else v


def rule(d):
    if d <= 4.0:
        return None
    return round(-1500 + (min(d, 100.0) - 4.0) / 96.0 * 1500)


def load(path):
    rows = [json.loads(l) for l in open(path)][1:]
    bell = next((r[2] for r in rows if r[0] == "C" and r[5] and "countdown_high" in r[5]), None)
    return rows, bell


def report(path, window=1e9):
    rows, bell = load(path)
    if bell is None:
        print(path, "no bell")
        return Counter()
    creates = [(r[2], vol(r[4][2])) for r in rows if r[0] == "C"]
    c = Counter()
    bad = []
    for r in rows:
        if r[0] != "IMP" or not 0 <= (r[2] - bell) / 1000 <= window:
            continue
        d = r[4]
        c["imp"] += 1
        if d > 4.0:
            c["gt4"] += 1
            near = [v for t, v in creates if -5 <= t - r[2] <= 60]
            exp = rule(d)
            if any(abs(v - exp) <= 2 for v in near):
                c["sound_at_rule_volume"] += 1
            else:
                bad.append((round(d, 2), exp, near))
        c["gt10"] += d >= 10
    c["gt4_arena"] = sum(1 for r in rows if r[0] == "HIT" and r[7] > 4 and r[3] < 0 and 0 <= (r[2] - bell) / 1000 <= window)
    print(path.split("/")[-1], dict(c), "mismatch", bad[:5])
    return c


def game_clock(rows, bell):
    """The crowd's own clock against wall time. The original ticks the crowd (and the match) with its
    frame time capped at 1/30 s, so under Wine its clock runs at 0.85 to 0.95 of the wall clock; the
    `TICK` rows' idle clock (+0xa8, seconds since the last gain of 10) is that clock. Returns
    f(wall_ms) -> crowd seconds from the bell, integrating the idle clock's growth between samples
    (an interval in which it was reset keeps the previous interval's ratio)."""
    ticks = [(r[2], r[5]) for r in rows if r[0] == "TICK"]
    pts = [(ticks[0][0], 0.0)]
    ratio = 1.0
    for (t0, i0), (t1, i1) in zip(ticks, ticks[1:]):
        wall = (t1 - t0) / 1000.0
        if i1 >= i0:
            ratio = (i1 - i0) / wall
            pts.append((t1, pts[-1][1] + (i1 - i0)))
        else:
            pts.append((t1, pts[-1][1] + ratio * wall))

    def at(t):
        if t <= pts[0][0]:
            return pts[0][1] - (pts[0][0] - t) / 1000.0
        for (ta, ga), (tb, gb) in zip(pts, pts[1:]):
            if ta <= t <= tb:
                return ga + (gb - ga) * (t - ta) / max(tb - ta, 1)
        return pts[-1][1] + (t - pts[-1][0]) / 1000.0

    g0 = at(bell)
    return lambda t: at(t) - g0


def tsv(path):
    """CAPTURE.jsonl -> CAPTURE.impacts.tsv: t_s (from the bell), kind, a, b, c, d for the rows
    `ra2-app`'s `impact_sounds` tests replay: IMP (damage, rule volume, created volume or empty),
    DMG (amount), EX (n, burst, excitement before, idle), MOOD (new, stop previous, old, excitement),
    SHOUT (mood, taunt, excitement)."""
    rows, bell = load(path)
    creates = [(r[2], vol(r[4][2])) for r in rows if r[0] == "C"]
    clock = game_clock(rows, bell)
    rows_t = []
    out = ["t_s\tkind\ta\tb\tc\td\tcrowd_s"]
    for r in rows:
        t = (r[2] - bell) / 1000.0
        if r[0] == "IMP":
            exp = rule(r[4])
            near = [v for ct, v in creates if -5 <= ct - r[2] <= 60 and exp is not None and abs(v - exp) <= 2]
            rows_t.append(r[2])
            out.append(f"{t:.3f}\tIMP\t{r[4]:.4f}\t{'' if exp is None else exp}\t{near[0] if near else ''}\t")
        elif r[0] == "DMG":
            rows_t.append(r[2])
            out.append(f"{t:.3f}\tDMG\t{r[3]:.4f}\t\t\t")
        elif r[0] == "EX":
            rows_t.append(r[2])
            out.append(f"{t:.3f}\tEX\t{r[3]}\t{r[4]}\t{r[5]}\t{r[6]:.3f}")
        elif r[0] == "MOOD":
            rows_t.append(r[2])
            out.append(f"{t:.3f}\tMOOD\t{r[3]}\t{r[4]}\t{r[5]}\t{r[6]}")
        elif r[0] == "SHOUT":
            rows_t.append(r[2])
            out.append(f"{t:.3f}\tSHOUT\t{r[3]}\t{r[4]}\t{r[5]}\t")
    out = [line if i == 0 else f"{line}\t{clock(rows_t[i - 1]):.3f}" for i, line in enumerate(out)]
    dest = path[: -len(".jsonl")] + ".impacts.tsv"
    open(dest, "w").write("\n".join(out) + "\n")
    return dest


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if "marks" not in a and a != "--tsv"]
    for p in args:
        if "--tsv" in sys.argv:
            print("wrote", tsv(p))
        else:
            report(p, 60.0)
