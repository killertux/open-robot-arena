#!/usr/bin/env python3
"""live_ref.py [--list] [--dir D]: the live references of the four reference fights, rebuilt per
capture class (docs/29 section 12bc).

Every live capture of the four fights (Box Scout/ALARM, Razor/FlapJack, Compressor Sentinel/Wide
Load, Octagon Scout/ALARM) with the gate's `H` rows is classified:
  bell   the agent was loaded before Start Game (capture_boxstart.py: partsfight-*, firststroke-*);
         the `MC`/`T` rows give the match clock, so the window is the match clock 0..120 s;
  late   the match was started (capture_drive.start_match: Skip Intro, 8 s, a Python call) and the
         agent attached after (capture_fight.py: fight-*, arenahits-*, flipfight-*): the first A row is
         the match clock ~6 s (the first sample of the sampling python call is at 5.9-6.0 s), so
         the window 0..120 s of capture time is the match clock 6..126 s.
Per capture and attacker (0, 1 = the robots, -1 = the arena): counted hits (priced >= 10 hp, the
0x417330 gate), the hp of every priced hit, and the match length (the first robot out, for bell).

Ours: `ours <dir>` reads the ensemble.sh directory (h<w>.jsonl, log<w>.txt).
"""
import collections
import glob
import json
import math
import os
import re
import statistics
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import parts_live as pl  # noqa: E402

ORIG = os.path.join(HERE, "..", "..", "local", "oracle", "original")
PAIRS = {
    "box": "scout-alarm-box",
    "razor": "razor-flapjack-box",
    "compressor": "sentinel-wideload-compressor",
    "octagon": "scout-alarm-octagon",
}
BELL = ("partsfight", "firststroke", "partsfightns")  # partsfightns: NOSAMPLE=1, no sampling call
LATE = ("fight", "arenahits", "flipfight")
LATE_OFFSET = 6.0  # the match clock at the first A row of a late capture
CAP = 120.0


def ms(x):
    n = len(x)
    if n == 0:
        return (float("nan"), float("nan"), 0)
    m = sum(x) / n
    se = statistics.stdev(x) / math.sqrt(n) if n > 1 else float("nan")
    return (m, se, n)


def fmt(t):
    return "%.1f +- %.1f" % (t[0], t[1])


def hits_of(path):
    """[(sim time since the first A row, att, vic, amount)] of the gate rows."""
    t = 0.0
    out = []
    for line in open(path):
        r = json.loads(line)
        if r[0] == "A":
            t += r[2]
        elif r[0] == "H":
            out.append((t, r[2], r[4], r[6]))
    return out, t


def summarise(hs, lo, hi, shift, end=None):
    """counted hits and hp per attacker with match clock t + shift in [lo, hi] (and <= end)."""
    cnt = collections.Counter()
    hp = collections.Counter()
    for t, att, vic, amt in hs:
        c = t + shift
        if c < lo or c > hi or (end is not None and c > end):
            continue
        hp[att] += amt
        if amt >= 10:
            cnt[att] += 1
    return cnt, hp


def captures(pair):
    name = PAIRS[pair]
    res = []
    for p in sorted(glob.glob(os.path.join(ORIG, "*-%s__live__*.jsonl" % name))):
        fam = os.path.basename(p).split("-")[0]
        if fam in BELL or fam in LATE:
            res.append((fam, p))
    return res


def live_rows(pair):
    rows = []
    for fam, p in captures(pair):
        hs, total = hits_of(p)
        if not hs:
            continue
        row = {"file": os.path.basename(p).replace("__live__", "/").replace(".jsonl", ""), "fam": fam}
        if fam in BELL:
            try:
                a = pl.analyse(p)
            except Exception:
                continue
            off = a["off"]
            if off is None:
                continue
            out = a["out"][:1]
            end = out[0][0] if out else None
            row.update(cls="bellns" if fam == "partsfightns" else "bell", shift=-off, end=end, hs=hs)
        else:
            row.update(cls="late", shift=LATE_OFFSET, end=None, hs=hs)
        rows.append(row)
    return rows


def ours_rows(d):
    rows = []
    for h in sorted(glob.glob(os.path.join(d, "h*.jsonl")), key=lambda s: int(re.findall(r"h(\d+)", s)[-1])):
        w = re.findall(r"h(\d+)\.jsonl", h)[0]
        try:
            log = [x for x in open(os.path.join(d, "log%s.txt" % w)) if x.startswith("fight")]
        except OSError:
            continue
        hs = [json.loads(x) for x in open(h)]
        cnt = collections.Counter()
        hp = collections.Counter()
        for x in hs:
            hp[x["att"]] += x["amount"]
            if x["amount"] >= 10:
                cnt[x["att"]] += 1
        ln = float(log[0].split()[1].rstrip("s,")) if log else 0.0
        rows.append((cnt, hp, ln))
    return rows


def stats_block(label, cs, hps, lens):
    return (label, ms([c[0] for c in cs]), ms([c[1] for c in cs]), ms([h[0] for h in hps]),
            ms([h[1] for h in hps]), ms(lens))


def live_sets(pair):
    """name -> list of (cnt, hp, length) per capture."""
    rows = live_rows(pair)
    sets = collections.defaultdict(list)
    for r in rows:
        hs = r["hs"]
        if r["cls"] == "late":
            c, h = summarise(hs, 0, CAP, r["shift"])
            sets["late, match clock 6-120 s"].append((c, h, None, r))
            c2, h2 = summarise(hs, 0, CAP, 0.0)
            sets["late, own 120 s (the old reference)"].append((c2, h2, None, r))
        else:
            end = r["end"]
            lab = "bell" if r["cls"] == "bell" else "bell unsampled"
            for key in ([lab, "bell all"] if r["cls"] == "bellns" else [lab, "bell all"]):
                c, h = summarise(hs, 0, CAP, r["shift"], end)
                ln = min(end, CAP) if end is not None else CAP
                sets[key + ", 0-120 s to the end"].append((c, h, ln, r))
                c2, h2 = summarise(hs, 0, CAP, r["shift"])
                sets[key + ", 0-120 s no end cut"].append((c2, h2, None, r))
                c3, h3 = summarise(hs, 6.0, CAP, r["shift"], end)
                sets[key + ", 6-120 s to the end"].append((c3, h3, None, r))
                c4, h4 = summarise(hs, 6.0, CAP, r["shift"])
                sets[key + ", 6-120 s no end cut"].append((c4, h4, None, r))
    return sets, rows


def table(ours_dir):
    """One table: bell-primed live (0-120 s of the match clock, to the end of the match) against ours,
    with the difference in standard errors ((ours - live) / sqrt(se_o^2 + se_l^2))."""
    print("| fight | n live / ours | metric | live bell-primed | ours | diff (se) |")
    print("|---|---|---|---|---|---|")
    for pair in PAIRS:
        sets, rows = live_sets(pair)
        v = sets.get("bell all, 0-120 s to the end", [])
        o = ours_rows(os.path.join(ours_dir, pair))
        if not v or not o:
            continue
        sd = {0: "side 0", 1: "side 1"}
        metrics = [
            ("counted hits, side 0", [x[0][0] for x in v], [c[0] for c, h, l in o]),
            ("counted hits, side 1", [x[0][1] for x in v], [c[1] for c, h, l in o]),
            ("damage hp, side 0", [x[1][0] for x in v], [h[0] for c, h, l in o]),
            ("damage hp, side 1", [x[1][1] for x in v], [h[1] for c, h, l in o]),
            ("match length s", [x[2] for x in v], [l for c, h, l in o]),
        ]
        for k, (name, a, b) in enumerate(metrics):
            la, ob = ms(a), ms(b)
            d = (ob[0] - la[0]) / math.sqrt(la[1] ** 2 + ob[1] ** 2) if la[1] == la[1] and ob[1] == ob[1] else float("nan")
            print("| %s | %s | %s | %s | %s | %+.1f |" % (pair if k == 0 else "", "%d / %d" % (la[2], ob[2]) if k == 0 else "", name, fmt(la), fmt(ob), d))


def main():
    args = sys.argv[1:]
    if "--table" in args:
        table(args[args.index("--table") + 1])
        return
    listing = "--list" in args
    ours_dir = None
    if "--ours" in args:
        ours_dir = args[args.index("--ours") + 1]
    for pair in PAIRS:
        sets, rows = live_sets(pair)
        print("== %s (%s)" % (pair, PAIRS[pair]))
        if listing:
            for r in rows:
                c, h = summarise(r["hs"], 0, CAP, r["shift"], r["end"])
                print("  %-52s %-4s n0 %3d n1 %3d hp0 %7.0f hp1 %7.0f arena %3d end %s" % (
                    r["file"], r["cls"], c[0], c[1], h[0], h[1], c[-1], "-" if r["end"] is None else "%.0f" % r["end"]))
        print("  %-40s %3s %-14s %-14s %-16s %-16s %s" % ("set", "n", "cnt side0", "cnt side1", "hp side0", "hp side1", "length"))
        for name, v in sets.items():
            if not v:
                continue
            cs = [x[0] for x in v]
            hp = [x[1] for x in v]
            lens = [x[2] for x in v if x[2] is not None]
            s0 = ms([c[0] for c in cs])
            s1 = ms([c[1] for c in cs])
            h0 = ms([h[0] for h in hp])
            h1 = ms([h[1] for h in hp])
            print("  %-40s %3d %-14s %-14s %-16s %-16s %s" % (
                name, len(v), fmt(s0), fmt(s1), fmt(h0), fmt(h1), fmt(ms(lens)) if lens else "-"))
        if "--opening" in args:
            for lab in ("bell", "bell unsampled"):
                rr = [r for r in rows if r["cls"] == ("bell" if lab == "bell" else "bellns")]
                fh = []
                for r in rr:
                    f = [(t + r["shift"], a, amt) for t, a, v, amt in r["hs"] if amt >= 10 and a in (0, 1) and t + r["shift"] > 0]
                    if f:
                        fh.append(f[0])
                n10 = [sum(1 for t, a, v, amt in r["hs"] if amt >= 10 and a in (0, 1) and 0 < t + r["shift"] <= 15) for r in rr]
                if rr:
                    print("  opening %-16s n %2d first counted robot hit at %s s, size %s hp, counted hits in the first 15 s %s" % (
                        lab, len(rr), fmt(ms([x[0] for x in fh])), fmt(ms([x[2] for x in fh])), fmt(ms(n10))))
        if ours_dir:
            o = ours_rows(os.path.join(ours_dir, pair))
            if o:
                print("  %-40s %3d %-14s %-14s %-16s %-16s %s" % (
                    "ours", len(o), fmt(ms([c[0] for c, h, l in o])), fmt(ms([c[1] for c, h, l in o])),
                    fmt(ms([h[0] for c, h, l in o])), fmt(ms([h[1] for c, h, l in o])), fmt(ms([l for c, h, l in o]))))


if __name__ == "__main__":
    main()
