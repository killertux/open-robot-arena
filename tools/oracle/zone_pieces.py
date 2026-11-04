#!/usr/bin/env python3
"""zone_pieces.py live <zonepieces-*.jsonl ...> | ours <ensemble dir> [colliderdump game bot0 bot1]  (docs/29 12au)

Which collidable of a robot enters an arena zone, per piece, live and ours.

  live: captures of frida/ra2_zonepieces.js (C/K rows as ra2_robotcolliders.js plus P, the zone
        listener's pair events, and W, the arena's ZoneEvent).  Each W row (direction, zone id,
        robot, chassis) follows the P row of its pair (the zone's phantom against a solid
        collidable), whose solid side is named by collider_compare.tag_names.  Pairs of the
        zone with a phantom (a traction ring, a SmartZone) raise no W and are counted apart.
  ours: the `zone ... entered by <component>` lines of the log<w>.txt files (FIDTRACE_LOG=1).

Prints enters per fight by (zone id, robot, piece) and by zone id.

`sz` instead of `live`/`ours` as the first argument (then `live-sz`, `ours-sz`): the same for the robots' own
SmartZones, an enter = a pair (SmartZone phantom, the other robot's solid collidable).  Ours needs
RA2_ZONE_LOG=1 in the run.  Rows are (owner robot, other robot, piece).
"""
import collections
import glob
import re
import os
import sys

sys.path.insert(0, __file__.rsplit("/", 1)[0])
import collider_compare as cc  # noqa: E402


T = float(os.environ.get("T", "1e9"))  # only the first T seconds of the match clock


def live(paths, dump, game, bots):
    per = []
    for path in paths:
        rows = cc.load(path)
        names, _ = cc.tag_names(rows, dump, game, bots)
        enters, phantom_pairs, last_p = collections.Counter(), collections.Counter(), []
        for r in rows:
            if r[0] in ("P", "W") and r[2] > T:
                continue
            if r[0] == "P":
                last_p.append(r)
                last_p = last_p[-6:]
                _, _, _, lv, a, b = r
                for x, y in ((a, b), (b, a)):
                    if x and y and x[2] == 0 and x[3] == 1 and y[2] != 0 and y[3] == 1 and lv == 0:
                        phantom_pairs[(names.get(y[1], "phantom"))] += 1
            elif r[0] == "W" and r[3][0] == 1:
                _, seq, _, (d, zid, rob, ch) = r
                piece = "?"
                for p in reversed(last_p):
                    _, _, _, lv, a, b = p
                    if lv != 0:
                        continue
                    for x, y in ((a, b), (b, a)):
                        if x and y and x[2] == 0 and x[3] == 1 and y[2] != 0 and y[3] == 0:
                            piece = names.get(y[1], "tag%d" % y[1])
                            break
                    if piece != "?":
                        break
                enters[(zid, rob, piece.split(":", 1)[-1])] += 1
        per.append((enters, phantom_pairs))
    return per


def live_sz(paths, dump, game, bots):
    per = []
    for path in paths:
        rows = cc.load(path)
        names, _ = cc.tag_names(rows, dump, game, bots)
        enters = collections.Counter()
        for r in rows:
            if r[0] != "P" or r[3] != 0 or r[2] > T:
                continue
            _, _, _, lv, a, b = r
            for x, y in ((a, b), (b, a)):
                # a SmartZone: a phantom of a robot's body (tag +0x54 16 / 32), not a traction ring (20 / 36)
                if x and y and x[3] == 1 and x[2] in (16, 32) and y[3] == 0 and y[2] != 0 and (y[2] >> 4) != (x[2] >> 4):
                    enters[(x[2] >> 4, y[2] >> 4, names.get(y[1], "tag%d" % y[1]).split(":", 1)[-1])] += 1
        per.append((enters, collections.Counter()))
    return per


def ours_sz(d):
    per = []
    files = sorted(glob.glob(d + "/log*.txt"), key=lambda p: int(re.findall(r"\d+", p.rsplit("/", 1)[-1])[0]))
    for p in files:
        enters = collections.Counter()
        for line in open(p):
            m = re.match(r'\[\s*([\d.]+)\] smartzone robot (\d) \(\d+\) entered by robot (\d) "(.*)"', line)
            if m and float(m[1]) <= T:
                enters[(int(m[2]), int(m[3]), m[4])] += 1
        per.append((enters, collections.Counter()))
    return per


def ours(d):
    per = []
    files = sorted(glob.glob(d + "/log*.txt"), key=lambda p: int(re.findall(r"\d+", p.rsplit("/", 1)[-1])[0]))
    for p in files:
        enters = collections.Counter()
        for line in open(p):
            m = re.match(r'\[\s*([\d.]+)\] zone "\w+" \((\d)\) robot (\d) entered by "(.*)" at', line)
            if m and float(m[1]) <= T:
                enters[(int(m[2]), int(m[3]), m[4])] += 1
        per.append((enters, collections.Counter()))
    return per


def main():
    kind = sys.argv[1]
    if kind == "live-sz":
        per = live_sz(sys.argv[2:], "engine/target/release/examples/colliderdump", "local/game",
                      ["AI/Team0/Bot0.bot", "AI/Team0/Bot1.bot"])
    elif kind == "ours-sz":
        per = ours_sz(sys.argv[2])
    elif kind == "live":
        args = sys.argv[2:]
        per = live(args, "engine/target/release/examples/colliderdump", "local/game",
                   ["AI/Team0/Bot0.bot", "AI/Team0/Bot1.bot"])
    else:
        per = ours(sys.argv[2])
    n = len(per)
    tot, by_zone, by_piece = collections.Counter(), collections.Counter(), collections.Counter()
    for e, _ in per:
        for (z, rob, pc), v in e.items():
            tot[(z, rob, pc)] += v
            by_zone[z] += v
            by_piece[(rob, pc)] += v
    print(f"{n} fights; enters per fight by zone: " + ", ".join(f"{z}: {v / n:.2f}" for z, v in sorted(by_zone.items())))
    print("by robot and piece, all zones, per fight:")
    for (rob, pc), v in sorted(by_piece.items(), key=lambda kv: -kv[1]):
        print(f"  robot {rob} {str(pc):32s} {v / n:6.2f}")
    if kind == "live":
        ph = collections.Counter()
        for _, p in per:
            ph.update(p)
        print("zone x phantom pairs (no event) per fight: " + ", ".join(f"{k} {v / n:.1f}" for k, v in ph.most_common(6)))


main()
