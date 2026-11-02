#!/usr/bin/env python3
"""live_runs.py live <boxstart-*octagon*.jsonl ...> | ours <ensemble dir>  (docs/29 12ao)

Hellraiser firings as runs of ZoneEvent enters (zone ids 1 and 2, robot 1 = Scout, 2 =
ALARM, gap < 0.5 s between enters of the same zone and robot), against the match clock.

  live: boxfight-/boxstart- captures (frida/ra2_boxfight.js, W rows). For a
        boxstart capture the clock is the first movement of a chassis (the A rows
        before it are the intro); a boxfight capture attached ~5 s late, so its
        times are relative to the attach.
  ours: the log<w>.txt files of ensemble.sh run with FIDTRACE_LOG=1.

Prints one line per fight and the runs per fight, the share of fights with a run
before 45 s, and the runs before 15 / 30 / 45 / 60 s.
"""
import glob, json, math, re, sys


def live_runs(path):
    t, start, xs, ev = 0.0, None, [], []
    for line in open(path):
        r = json.loads(line)
        if r[0] == "A":
            t += r[2]
        elif r[0] == "X":
            xs.append(r[2])
            if start is None and len(xs) > 1:
                if any(abs(p[1][0] - q[1][0]) + abs(p[1][2] - q[1][2]) > 0.05 for p, q in zip(xs[0], r[2])):
                    start = t
        elif r[0] == "W":
            ev.append((t, r[2]))
    start = start or 0.0
    runs = []
    for tt, (d, z, rob, _) in ev:
        if d != 1 or z > 2 or rob < 1:
            continue
        tt -= start
        if runs and runs[-1][1:3] == [z, rob] and tt - runs[-1][3] < 0.5:
            runs[-1][3] = tt
        else:
            runs.append([tt, z, rob, tt])
    return runs


def ours_runs(path):
    runs = []
    for line in open(path):
        m = re.match(r'\[\s*([\d.]+)\] zone "\w+" \((\d)\) robot (\d) entered', line)
        if not m or int(m[2]) > 2:
            continue
        t, z, rob = float(m[1]), int(m[2]), int(m[3])
        if runs and runs[-1][1:3] == [z, rob] and t - runs[-1][3] < 0.5:
            runs[-1][3] = t
        else:
            runs.append([t, z, rob, t])
    return runs


def main():
    kind, args = sys.argv[1], sys.argv[2:]
    if kind == "live":
        fights = [live_runs(p) for p in sorted(args)]
    else:
        fights = [ours_runs(p) for p in sorted(glob.glob(args[0] + "/log*.txt"), key=lambda p: int(re.findall(r"\d+", p.rsplit("/", 1)[-1])[0]))]
    for i, r in enumerate(fights):
        print(i, [(round(a, 1), z, rob) for a, z, rob, _ in r])
    n = len(fights)
    print(f"{n} fights, {sum(len(r) for r in fights) / n:.2f} runs per fight; "
          f"{sum(1 for r in fights if any(a < 45 for a, *_ in r))} with a run before 45 s")
    for T in (15, 30, 45, 60):
        print(f"  runs before {T} s: {sum(1 for r in fights for a, *_ in r if a < T) / n:.2f} per fight")


main()
