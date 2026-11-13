"""tailfight_cmp.py 'local/oracle/original/tailfight-<name>__live__0*.jsonl' [ENV=VAL ...]: every trial of a `capture_tailfight.py` capture
(a Scout stood on its tail at (dx, dz) from its rest centre, ALARM `dist` m away in direction `az`, both shipped AIs on from the release;
docs/29 12bg) run in ours (`examples/tipprobe` with TIPPROBE_AI=1, the brains switched on `AION` substeps after the release as in
the capture) and the Scout's outcome of both side by side: the share of the first SECS s with |up.y| < 0.3, the time it leaves the tail
for good (|up.y| >= 0.5 for 0.5 s) and its last up.y. Run from the repository root; needs examples/tipprobe built in engine/."""
import glob
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
GAME = os.environ.get("GAME", os.path.join(ROOT, "local", "game"))
ARENA = os.environ.get("ARENA", "octagon")


def live_summary(path):
    t = 0.0
    placed = None
    series = []
    trial = aion = None
    for line in open(path):
        r = json.loads(line)
        k = r[0]
        if k == "TRIAL":
            trial = r[2]
        elif k == "AION":
            aion = r[2]
        elif k == "A":
            t += r[2]
        elif k == "X" and r[2] == "place" and placed is None:
            placed = t
        elif k == "B" and placed is not None and abs(r[3] - 163.49) < 0.1:
            series.append((t - placed, r[7][4]))
    return trial, aion, series


def summarize(series, secs):
    series = [s for s in series if s[0] <= secs]
    if not series:
        return None
    off = None
    for i, (tt, u) in enumerate(series):
        if abs(u) >= 0.5:
            j = i
            while j < len(series) and series[j][0] - tt < 0.5 and abs(series[j][1]) >= 0.5:
                j += 1
            if j == len(series) or series[j][0] - tt >= 0.5:
                off = tt
                break
    dt = [series[i][0] - series[i - 1][0] for i in range(1, len(series))]
    tail = sum(d for d, s in zip(dt, series[1:]) if abs(s[1]) < 0.3) / max(sum(dt), 1e-9)
    return tail, off, series[-1][1]


def ours(trial, aion, secs, extra):
    env = dict(os.environ)
    env.update({
        "LC_ALL": "C", "RA2_HAZARDS": "1", "TIPPROBE_AI": "1", "TIPPROBE_ROBOT": "0", "TIPPROBE_ALARM_REL": "1",
        "TIPPROBE_COM_SHIFT": "%g,%g" % (trial["dx"], trial["dz"]),
        "TIPPROBE_ALARM": "%g,%g" % (trial["az"], trial["dist"]),
        "TIPPROBE_AI_AT": str(0 if os.environ.get("AI_AT0") else (aion or 0)),
    })
    env.update(extra)
    out = subprocess.run(
        [os.path.join(ROOT, "engine", "target", "release", "examples", "tipprobe"),
         GAME + "/Arenas/%s.py" % ARENA, GAME + "/AI/Team0/Bot0.bot", GAME + "/AI/Team0/Bot1.bot",
         "self", "1", "0", "0", "-90", "0", str(secs)],
        cwd=os.path.join(ROOT, "engine"), env=env, capture_output=True, text=True).stdout
    series = []
    for line in out.splitlines():
        p = line.split()
        if p and p[0] == "S":
            series.append((float(p[1]), float(p[8])))
    return series


def main():
    files = sorted(glob.glob(sys.argv[1]))
    extra = dict(a.split("=", 1) for a in sys.argv[2:])
    secs = float(os.environ.get("SECS", "12"))
    jobs = []
    for f in files:
        trial, aion, series = live_summary(f)
        if trial is None:
            continue
        jobs.append((f, trial, aion, series))
    with ThreadPoolExecutor(max_workers=int(os.environ.get("JOBS", "6"))) as ex:
        res = list(ex.map(lambda j: ours(j[1], j[2], secs, extra), jobs))
    fmt = lambda s: "tail %.2f off %s final %+.2f" % (s[0], ("%.1fs" % s[1]) if s[1] is not None else "never", s[2]) if s else "-"
    n = [0, 0]
    for (f, trial, aion, series), o in zip(jobs, res):
        a, b = summarize(series, secs), summarize(o, secs)
        print("%-28s AI at %2s | live %-34s | ours %-34s" % (trial["label"], aion, fmt(a), fmt(b)))
        if a and b:
            n[0] += a[0]
            n[1] += b[0]
    k = max(len(jobs), 1)
    print("mean tail share: live %.3f ours %.3f (%d trials)" % (n[0] / k, n[1] / k, len(jobs)))


if __name__ == "__main__":
    main()
