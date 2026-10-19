#!/usr/bin/env python3
"""Fidelity harness: our app against genuine captures of the ORIGINAL game.

    tools/oracle/fidelity.py                      # screens, both renderers
    tools/oracle/fidelity.py --only mainmenu,botlab   # fixtures whose id starts with
    tools/oracle/fidelity.py --renderer app       # app | shell | both (default)
    tools/oracle/fidelity.py --gameplay           # + simulation traces vs live traces
    tools/oracle/fidelity.py --gameplay-only
    tools/oracle/fidelity.py --list

The reference side is `local/oracle/original/` (root-window captures of the
real game under Wine, 1024x768; see `tools/capture/README.md`).  Nothing here
reads a reconstruction (`local/parity/*__compose.png` is NOT a reference).

Screens.  Each fixture is a route of widget-name clicks from the main menu.
"ours" is rendered by two paths, because they are different products:

  app    `ra2-app --offscreen out.png --size 1024x768 --click-names ...`: what a
         player sees.  The shell is laid out at its 1280x1005 design size and
         the frame is resampled to 1024x768.
  shell  `ra2-shell --script ... --size 1024x768`: the same screens laid out
         natively at the reference size (what the original does).

Per fixture it writes (under `--out`, default `local/oracle/fidelity/latest`):
  ours/<renderer>/<id>.png         our render
  side/<renderer>/<id>.png         reference | ours | |diff| x4 side by side
and a `summary.md` / `summary.json` with, per fixture and renderer:
  mae       mean absolute RGB difference, 0..255 (the repo's historic metric)
  masked    same with the OS pointer rectangle excluded (when one was found)
  over8     fraction of pixels whose largest channel difference exceeds 8
  lowfreq   MAE after a 16x16 box downscale: layout/colour agreement without
            glyph-level detail
A fixture our app cannot reach is listed as `unreachable` with the reason; a
missing reference is `no-reference`.  Nothing is invented.

Gameplay (`--gameplay`).  Runs `ra2sim`'s `fidtrace` example for the same
robots in the same arena (Scout vs ALARM, Combat Arena) and compares with the
original's live traces: per-robot speed distribution, distance travelled,
hitpoints lost, damage events per second and the magnitude histogram.  If a
deterministic drive capture exists (`local/oracle/original/drive-*.tsv`) the
motor comparison (time to speed, top speed, turn rate) is added.

Exit status is always 0 unless a tool is missing; the numbers are the product.
"""
import argparse
import ast
import concurrent.futures
import datetime
import json
import math
import os
import shutil
import statistics
import subprocess
import sys
import tempfile

from PIL import Image, ImageChops, ImageDraw, ImageStat

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import compare  # noqa: E402  (pointer scan shared with the historic harness)

ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
SIZE = (1024, 768)


def main_root():
    try:
        common = subprocess.check_output(
            ["git", "-C", ROOT, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            text=True, stderr=subprocess.DEVNULL).strip()
        return os.path.dirname(common)
    except Exception:
        return ROOT


LOCAL = os.environ.get("RA2_ORACLE_LOCAL") or (
    os.path.join(ROOT, "local") if os.path.isdir(os.path.join(ROOT, "local", "oracle"))
    else os.path.join(main_root(), "local"))
ORIG = os.path.join(LOCAL, "oracle", "original")
GAME = os.environ.get("RA2_GAME") or os.path.join(LOCAL, "game")
BIN = os.path.join(ROOT, "engine", "target", "release")
APP = os.path.join(BIN, "ra2-app")
SHELL = os.path.join(BIN, "ra2-shell")
FIDTRACE = os.path.join(BIN, "examples", "fidtrace")

# --------------------------------------------------------------------------
# Fixtures.  `route` is a list of widget names (clicked in order from the main
# menu); `play` / `match` mark fixtures produced by the app's own runners.
# --------------------------------------------------------------------------
TEAMHQ = ["Team HQ button", "continue button"]
FIXTURES = [
    dict(id="mainmenu", ref="mainmenu__1024x768__000.png", route=[]),
    dict(id="exhibition-settings", ref="exhibition-settings__1024x768__000.png",
         route=["exhibition button"]),
    dict(id="exhibition-arena", ref="exhibition-arena__1024x768__000.png",
         route=["exhibition button", "arena button"]),
    dict(id="exhibition-arena-box", ref="exhibition-arena-box__1024x768__000.png",
         play="setup",
         note="ours: ra2-app --play (arena row clicked, hazards UNCHECKED; reference has hazards on)"),
    dict(id="exhibition-robots", ref="exhibition-robots__1024x768__000.png",
         route=["exhibition button", "robots button"]),
    dict(id="exhibition-deathmatch", ref="exhibition-deathmatch__1024x768__000.png",
         route=["exhibition button", "gametype button", "deathmatch checkbox"]),
    dict(id="exhibition-pick-ai", ref="exhibition-pick-ai__1024x768__000.png",
         route=["exhibition button", "robots button", "computer player button"]),
    dict(id="exhibition-robots-filled", ref="exhibition-robots-filled__1024x768__000.png",
         unreachable="picker clicks need pixel points; ra2-app cannot mix pixel and named clicks"),
    dict(id="teamhq-select", ref="teamhq-select__1024x768__000.png", route=["Team HQ button"]),
    dict(id="teamhq-inventory", ref="teamhq-inventory__1024x768__000.png",
         route=TEAMHQ + ["robots button"]),
    dict(id="teamhq-standings", ref="teamhq-standings__1024x768__000.png",
         route=TEAMHQ + ["league button"]),
    dict(id="teamhq-events", ref="teamhq-events__1024x768__000.png",
         route=TEAMHQ + ["events button"]),
    dict(id="teamhq-history", ref="teamhq-history__1024x768__000.png",
         route=TEAMHQ + ["history button"]),
    dict(id="botlab-overview", ref="botlab-overview__1024x768__000.png",
         route=TEAMHQ + ["robots button", "workshop button"],
         note="reference has C.R.U.S.H. selected and a random tip line"),
    dict(id="botlab-chassis", ref="botlab-chassis__1024x768__000.png",
         route=TEAMHQ + ["robots button", "workshop button", "chassis button"]),
    dict(id="botlab-components", ref="botlab-components__1024x768__000.png",
         route=TEAMHQ + ["robots button", "workshop button", "components button"]),
    dict(id="botlab-wiring", ref="botlab-wiring__1024x768__000.png",
         route=TEAMHQ + ["robots button", "workshop button", "wiring button"]),
    dict(id="botlab-paint", ref="botlab-paint__1024x768__000.png",
         route=TEAMHQ + ["robots button", "workshop button", "paintshop button"]),
    dict(id="match-countdown", ref="match-countdown__1024x768__000.png", match="countdown.png",
         note="ours: ra2-app --match Scout vs ALARM, Combat Arena, watch camera"),
    dict(id="match-hud", ref="match-hud__1024x768__000.png", match="fight-mid.png",
         note="robot positions/HUD values differ by construction (live fight)"),
    dict(id="match-results", ref="match-results__1024x768__000.png", match="results.png",
         note="scores differ by construction (live fight)"),
]


# arena id (the `arena-<id>-*` capture name) -> the arena script `ra2-app --arena` loads.
ARENA_SCRIPTS = {
    "box": "box.py", "bridge": "Bridge.py", "compressor": "compressor.py",
    "electric": "electric.py", "halfcircle": "half.py", "metalskull": "Skull.py",
    "octagon": "octagon.py", "parkinglot": "parkinglot.py",
}
# capture shot -> `--camera-shots` entry (the same key presses as capture_arenas.py:
# F2, F3, F3 again, F4; ours at 5-11 fight seconds because our fights end sooner; intro ~2.5 s after Start Game).
ARENA_SHOTS = {
    "intro": "intro@2.5", "countdown": "countdown@2", "f2": "f2@5", "f3": "f3@7",
    "f3b": "f3b@9", "f4": "f4@11",
}


# The capture script clicks `Skip Intro` a fixed wall time after `Start Game`, but each arena loads for a
# different time, so the intro clock reads differently at the click.  The watch camera keeps the pose
# the intro flight had at that moment (docs/30), hence the countdown still depends on it.  These are
# FITTED capture-timing parameters (the intro-clock second whose countdown best matches the reference),
# not game behaviour; `skip@T` is `--camera-shots`' "click Skip Intro at intro second T".
ARENA_SKIP = {
    "box": 4.6, "bridge": 5.1, "compressor": 6.8, "electric": 7.5, "halfcircle": 4.5,
    "metalskull": 4.8, "octagon": 4.65, "parkinglot": 4.9,
}


def find_arena_fixtures():
    """arena-<id>-<shot> references from `capture_arenas.py`.  Ours comes from
    `ra2-app --match --arena <script> --camera-shots ...` (one run per arena;
    the key presses are made in order, so f3 then f3b is F3 twice)."""
    out = []
    if os.path.isdir(ORIG):
        for f in sorted(os.listdir(ORIG)):
            if f.startswith("arena-") and f.endswith(".png"):
                fid = f.split("__")[0]
                _, arena, shot = fid.split("-", 2)
                if arena in ARENA_SCRIPTS and shot in ARENA_SHOTS:
                    out.append(dict(id=fid, ref=f, camshot=(arena, shot),
                                    note="ours: ra2-app --match camera shot; robot poses differ (live fight)"))
                else:
                    out.append(dict(id=fid, ref=f, unreachable=
                                    "no app route for this arena still (the select screen is a shell fixture)"))
    return out


def run_camera_shots(arena, tmp):
    """One `--camera-shots` run for `arena`; returns the directory of PNGs or None."""
    d = os.path.join(tmp, "cam-" + arena)
    os.makedirs(d, exist_ok=True)
    shots = ",".join(
        [ARENA_SHOTS["intro"], "skip@%g" % ARENA_SKIP.get(arena, 4.8)]
        + [ARENA_SHOTS[k] for k in ("countdown", "f2", "f3", "f3b", "f4")])
    bots = ",".join(os.path.join(GAME, "AI", "Team0", b) for b in ("Bot0.bot", "Bot1.bot"))
    p = subprocess.run([APP, "--game", GAME, "--match", "--seconds", "40", "--out", d,
                        "--size", "%dx%d" % SIZE, "--bots", bots, "--arena", os.path.join(GAME, "Arenas", ARENA_SCRIPTS[arena]),
                        "--camera-shots", shots],
                       capture_output=True, text=True, env=env(), timeout=900)
    return d if p.returncode == 0 else None


# --------------------------------------------------------------------------
# Rendering ours
# --------------------------------------------------------------------------

def env():
    e = dict(os.environ)
    e["RA2_GAME"] = GAME
    return e


def render_app(fx, out_png):
    cmd = [APP, "--game", GAME, "--offscreen", out_png, "--size", "%dx%d" % SIZE]
    if fx["route"]:
        cmd += ["--click-names", ";".join(fx["route"])]
    p = subprocess.run(cmd, capture_output=True, text=True, env=env(), timeout=180)
    if p.returncode or not os.path.exists(out_png):
        return (p.stderr or p.stdout).strip().splitlines()[-1:] or ["failed"]
    return None


def render_shell(fx, out_png):
    # Team HQ / bot lab need the career (the original's captures were taken with the
    # shipped team "The Rookies", Teams/Team0); only ra2-app loads it, and it lays the
    # shell out natively at the target size too, so it is the shell column for them.
    if "Team HQ button" in fx["route"]:
        return render_app(fx, out_png)
    with tempfile.TemporaryDirectory() as d:
        script = os.path.join(d, "r.steps")
        lines = ['click "%s"' % n for n in fx["route"]] + ["shot out.png"]
        open(script, "w").write("\n".join(lines) + "\n")
        p = subprocess.run([SHELL, "--game", GAME, "--script", script, "--out", d,
                            "--size", "%dx%d" % SIZE],
                           capture_output=True, text=True, env=env(), timeout=180)
        if p.returncode or not os.path.exists(os.path.join(d, "out.png")):
            return (p.stderr or p.stdout).strip().splitlines()[-1:] or ["failed"]
        shutil.copy(os.path.join(d, "out.png"), out_png)
    return None


def run_match_frames(tmp):
    """One ra2-app --match run; the frames are shared by the match fixtures."""
    d = os.path.join(tmp, "match")
    os.makedirs(d, exist_ok=True)
    bots = ",".join(os.path.join(GAME, "AI", "Team0", b) for b in ("Bot0.bot", "Bot1.bot"))
    p = subprocess.run([APP, "--game", GAME, "--match", "--seconds", "20", "--out", d,
                        "--size", "%dx%d" % SIZE, "--bots", bots],
                       capture_output=True, text=True, env=env(), timeout=300)
    return d if p.returncode == 0 else None


def run_play_frames(tmp):
    d = os.path.join(tmp, "play")
    os.makedirs(d, exist_ok=True)
    p = subprocess.run([APP, "--game", GAME, "--play", "--drive", "0-1:1,0", "--seconds", "1",
                        "--out", d, "--size", "%dx%d" % SIZE],
                       capture_output=True, text=True, env=env(), timeout=300)
    return d if p.returncode == 0 else None


# --------------------------------------------------------------------------
# Measuring
# --------------------------------------------------------------------------

def load(path):
    im = Image.open(path)
    if im.mode != "RGB":
        im = im.convert("RGB")
    return im


def measure(ours, ref, ref_path):
    """MAE (and masked / over8 / lowfreq) between two same-size RGB images."""
    def mae(a, b, exclude=None):
        diff = ImageChops.difference(a, b)
        if exclude:
            x, y, w, h = exclude
            diff.paste((0, 0, 0), (x, y, x + w, y + h))
            n = a.size[0] * a.size[1] - w * h
            total = sum(ImageStat.Stat(diff).sum) / 3.0
            return total / n
        return sum(ImageStat.Stat(diff).mean) / 3.0

    res = {"mae": mae(ours, ref)}
    r, g, b = ImageChops.difference(ours, ref).split()
    worst = ImageChops.lighter(ImageChops.lighter(r, g), b)
    hist = worst.histogram()
    res["over8"] = sum(hist[9:]) / float(ours.size[0] * ours.size[1])
    side = ref_path + ".nopointer"
    pointer = None
    if not os.path.exists(side):
        scan = compare.detect_pointer(bytearray(ref.tobytes()), *ref.size)
        pointer = scan["rect"] if scan else None
    res["pointer"] = pointer
    res["masked"] = mae(ours, ref, pointer) if pointer else res["mae"]
    small = (SIZE[0] // 16, SIZE[1] // 16)
    res["lowfreq"] = mae(ours.resize(small, Image.BOX), ref.resize(small, Image.BOX))
    return res


def side_by_side(ours, ref, out_path):
    w, h = ref.size
    diff = ImageChops.difference(ours, ref).point(lambda v: min(255, v * 4))
    sheet = Image.new("RGB", (w * 3, h + 16), (24, 24, 24))
    for i, (im, label) in enumerate(((ref, "original"), (ours, "ours"), (diff, "|diff| x4"))):
        sheet.paste(im, (i * w, 16))
        ImageDraw.Draw(sheet).text((i * w + 4, 2), label, fill=(255, 255, 255))
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    sheet.save(out_path)


def screens(args, out):
    renderers = ["app", "shell"] if args.renderer == "both" else [args.renderer]
    fixtures = FIXTURES + find_arena_fixtures()
    if args.only:
        keys = args.only.split(",")
        fixtures = [f for f in fixtures if any(f["id"].startswith(k) for k in keys)]
    tmp = tempfile.mkdtemp(prefix="ra2-fidelity-")
    match_dir = play_dir = None
    if any("match" in f for f in fixtures) and "app" in renderers:
        match_dir = run_match_frames(tmp)
    if any("play" in f for f in fixtures) and "app" in renderers:
        play_dir = run_play_frames(tmp)

    cam_dirs = {}
    cam_arenas = sorted({f["camshot"][0] for f in fixtures if "camshot" in f
                         and os.path.exists(os.path.join(ORIG, f["ref"]))}) if "app" in renderers else []
    if cam_arenas:
        with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, min(args.jobs, 2))) as pool:
            for arena, d in zip(cam_arenas, pool.map(lambda a: run_camera_shots(a, tmp), cam_arenas)):
                cam_dirs[arena] = d

    jobs = []
    for fx in fixtures:
        ref_path = os.path.join(ORIG, fx["ref"])
        for rd in renderers:
            jobs.append((fx, rd, ref_path))

    def work(job):
        fx, rd, ref_path = job
        row = dict(id=fx["id"], renderer=rd, note=fx.get("note", ""))
        if not os.path.exists(ref_path):
            row["status"] = "no-reference"
            return row
        if "unreachable" in fx:
            row["status"] = "unreachable"
            row["reason"] = fx["unreachable"]
            return row
        out_png = os.path.join(out, "ours", rd, fx["id"] + ".png")
        os.makedirs(os.path.dirname(out_png), exist_ok=True)
        err = None
        if "camshot" in fx:
            if rd != "app":
                row["status"] = "unreachable"
                row["reason"] = "the match is only rendered by ra2-app"
                return row
            arena, shot = fx["camshot"]
            src = os.path.join(cam_dirs.get(arena) or "", "cam-%s.png" % shot)
            if not os.path.exists(src):
                row["status"], row["reason"] = "render-failed", "ra2-app --camera-shots failed for " + arena
                return row
            shutil.copy(src, out_png)
        elif "match" in fx:
            if rd != "app":
                row["status"] = "unreachable"
                row["reason"] = "the match is only rendered by ra2-app"
                return row
            src = os.path.join(match_dir or "", fx["match"])
            if not os.path.exists(src):
                row["status"], row["reason"] = "render-failed", "ra2-app --match failed"
                return row
            shutil.copy(src, out_png)
        elif "play" in fx:
            if rd != "app":
                row["status"] = "unreachable"
                row["reason"] = "only ra2-app --play drives the arena list"
                return row
            src = os.path.join(play_dir or "", "play-setup.png")
            if not os.path.exists(src):
                row["status"], row["reason"] = "render-failed", "ra2-app --play failed"
                return row
            shutil.copy(src, out_png)
        elif rd == "app":
            err = render_app(fx, out_png)
        else:
            err = render_shell(fx, out_png)
        if err:
            row["status"], row["reason"] = "render-failed", " ".join(err)
            return row
        ours, ref = load(out_png), load(ref_path)
        if ours.size != ref.size:
            ours = ours.resize(ref.size, Image.LANCZOS)
            row["resized"] = True
        row.update(measure(ours, ref, ref_path))
        side_by_side(ours, ref, os.path.join(out, "side", rd, fx["id"] + ".png"))
        row["status"] = "ok"
        return row

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        rows = list(pool.map(work, jobs))
    shutil.rmtree(tmp, ignore_errors=True)
    return rows


# --------------------------------------------------------------------------
# Gameplay
# --------------------------------------------------------------------------

def read_trace(path):
    rows = []
    for line in open(path):
        if line.startswith("time_elapsed") or not line.strip():
            continue
        t, data = line.rstrip("\n").split("\t", 1)
        rows.append((float(t), ast.literal_eval(data)))
    return rows


def last_segment(rows):
    """The live traces can span a reset of the game clock; keep the last run
    that starts near zero (the match itself)."""
    start = 0
    for i in range(1, len(rows)):
        if rows[i][0] < rows[i - 1][0] - 1.0:
            start = i
    return rows[start:]


def pct(xs, p):
    if not xs:
        return float("nan")
    xs = sorted(xs)
    k = (len(xs) - 1) * p
    lo, hi = int(math.floor(k)), int(math.ceil(k))
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)


def trace_metrics(rows, until=None):
    rows = [r for r in rows if until is None or r[0] <= until]
    out = {}
    n = len(rows[0][1]) if rows else 0
    out["duration"] = rows[-1][0] - rows[0][0] if rows else 0
    for p in range(n):
        speeds, ground, path = [], [], 0.0
        for (t0, d0), (t1, d1) in zip(rows, rows[1:]):
            dt = t1 - t0
            if dt <= 0 or dt > 1.0:
                continue
            a, b = d0[p][1], d1[p][1]
            step = math.hypot(b[0] - a[0], b[2] - a[2])
            path += step
            ground.append(step / dt)
        speeds = [abs(d[p][3]) for _, d in rows]
        hp0, hp1 = rows[0][1][p][4], rows[-1][1][p][4]
        first, last = rows[0][1][p][1], rows[-1][1][p][1]
        out[p] = {
            "speed_mean": statistics.fmean(speeds), "speed_p50": pct(speeds, .5),
            "speed_p90": pct(speeds, .9), "speed_max": max(speeds),
            "ground_p50": pct(ground, .5), "ground_p90": pct(ground, .9),
            "moving_frac": sum(1 for s in ground if s > 0.5) / max(1, len(ground)),
            "path_m": path,
            "net_m": math.hypot(last[0] - first[0], last[2] - first[2]),
            "hp_start": hp0, "hp_end": hp1,
        }
    return out


BINS = [(0, 5), (5, 10), (10, 20), (20, 50), (50, 1e9)]


def damage_metrics(events, duration):
    hist = [0] * len(BINS)
    weapon = env_hits = 0
    total = 0.0
    for e in events:
        a = e["amount"]
        total += a
        for i, (lo, hi) in enumerate(BINS):
            if lo <= a < hi:
                hist[i] += 1
        if e["attacker"] < 0:
            env_hits += 1
        else:
            weapon += 1
    return {"events": len(events), "weapon_events": weapon, "env_events": env_hits,
            "events_per_s": len(events) / duration if duration else 0.0,
            "total": total, "hist": hist}


def read_original_damage(path):
    ev = []
    for line in open(path):
        j = json.loads(line)
        if j.get("ev") != "damage":
            continue
        atk = j["u32"][0]
        ev.append({"attacker": -1 if atk == 4294967295 else atk, "victim": j["u32"][2],
                   "amount": j["f32"][4]})
    return ev


def gameplay(args, out):
    res = {}
    tr = os.path.join(ORIG, "match-trace-box-scout-alarm__live__002.tsv")
    dm = os.path.join(ORIG, "match-damage__live__000.jsonl")
    if not os.path.exists(FIDTRACE):
        return {"error": "missing %s (cd engine && cargo build --release --offline -p ra2sim "
                         "--example fidtrace)" % FIDTRACE}
    if not os.path.exists(tr):
        return {"error": "no original match trace at " + tr}
    orig_rows = last_segment(read_trace(tr))
    dur = orig_rows[-1][0]
    res["original_fight_seconds"] = dur
    gdir = os.path.join(out, "gameplay")
    os.makedirs(gdir, exist_ok=True)
    ours_tsv = os.path.join(gdir, "ours-match-trace.tsv")
    ours_dmg = os.path.join(gdir, "ours-match-damage.jsonl")
    box = os.path.join(GAME, "Arenas", "box.py")
    bots = [os.path.join(GAME, "AI", "Team0", b) for b in ("Bot0.bot", "Bot1.bot")]
    p = subprocess.run([FIDTRACE, "match", box, bots[0], bots[1], "%.1f" % dur, ours_tsv, ours_dmg],
                       capture_output=True, text=True, env=env(), timeout=900)
    if p.returncode:
        return {"error": "fidtrace failed: " + p.stderr[-300:]}
    res["ours_summary"] = p.stderr.strip().splitlines()[-1]
    our_rows = read_trace(ours_tsv)
    res["trace"] = {"original": trace_metrics(orig_rows), "ours": trace_metrics(our_rows, dur)}
    o_ev = read_original_damage(dm) if os.path.exists(dm) else []
    s_ev = [json.loads(l) for l in open(ours_dmg)]
    res["damage"] = {"original": damage_metrics(o_ev, dur), "ours": damage_metrics(s_ev, dur)}
    res["drive"] = drive_compare(gdir)
    return res


def drive_compare(gdir):
    """Our single-robot drive test against the original's, when captured."""
    ref = None
    if os.path.isdir(ORIG):
        for f in sorted(os.listdir(ORIG)):
            if f.startswith("drive-") and f.endswith(".tsv"):
                ref = os.path.join(ORIG, f)
    if not ref:
        return {"skipped": "no local/oracle/original/drive-*.tsv yet"}
    meta = ref[:-4] + ".json"
    if not os.path.exists(meta):
        return {"skipped": "missing " + meta}
    m = json.load(open(meta))
    ours_tsv = os.path.join(gdir, "ours-drive-trace.tsv")
    box = os.path.join(GAME, "Arenas", "box.py")
    bot = os.path.join(GAME, m["bot"])
    p = subprocess.run([FIDTRACE, "drive", box, bot, m["ours_script"], str(m["seconds"]), ours_tsv],
                       capture_output=True, text=True, env=env(), timeout=300)
    if p.returncode:
        return {"error": p.stderr[-300:]}
    orows = read_trace(ref)
    # original: phase edges from the recorded command columns (cmd_throttle, cmd_turn)
    def first(pred, start=0):
        for i in range(start, len(orows)):
            if pred(orows[i][1][0]):
                return i
        return None
    i_turn = first(lambda d: d[7] != 0)
    i_turn_end = first(lambda d: d[7] == 0, i_turn)
    i_str = first(lambda d: d[6] != 0, i_turn_end)
    i_str_end = first(lambda d: d[6] == 0, i_str)
    oe = {k: orows[i][0] for k, i in (("turn_start", i_turn), ("turn_end", i_turn_end),
                                      ("str_start", i_str), ("str_end", i_str_end))}
    segs = [seg.split(":")[0].split("-") for seg in m["ours_script"].split(";")]
    se = {"turn_start": float(segs[0][0]), "turn_end": float(segs[0][1]),
          "str_start": float(segs[2][0]), "str_end": float(segs[2][1])}
    return {"reference": os.path.basename(ref), "meta": m, "original_edges": oe,
            "original": drive_metrics(orows, oe), "ours": drive_metrics(read_trace(ours_tsv), se)}


def drive_metrics(rows, e):
    """Per-phase numbers of the single-robot drive test (robot 0)."""
    pts = [(t, d[0]) for t, d in rows]
    def seg(a, b):
        return [(t, d) for t, d in pts if a <= t <= b]
    # turn in place: mean yaw rate after a 0.5 s spin-up, and the peak
    turn = seg(e["turn_start"] + 0.5, e["turn_end"])
    rates, prev = [], None
    acc = 0.0
    for t, d in turn:
        if prev is not None:
            dh = (d[2] - prev[1] + math.pi) % (2 * math.pi) - math.pi
            acc += dh
            if t > prev[0]:
                rates.append(dh / (t - prev[0]))
        prev = (t, d[2])
    span = turn[-1][0] - turn[0][0] if len(turn) > 1 else float("nan")
    st = seg(e["str_start"], e["str_end"])
    top = max((abs(d[3]) for _, d in st), default=float("nan"))
    t90 = next((t - e["str_start"] for t, d in st if abs(d[3]) >= 0.9 * top), float("nan"))
    def disp(a, b):
        s0 = seg(a, b)
        if len(s0) < 2:
            return float("nan")
        p0, p1 = s0[0][1][1], s0[-1][1][1]
        return math.hypot(p1[0] - p0[0], p1[2] - p0[2])
    peak_after = max((abs(d[3]) for _, d in seg(e["str_end"], e["str_end"] + 0.2)), default=float("nan"))
    return {"turn_rate_rad_s": acc / span if span == span and span > 0 else float("nan"),
            "turn_rate_peak": max((abs(r) for r in rates), default=float("nan")),
            "top_speed": top, "t_to_90pct_s": t90,
            "straight_distance_m": disp(e["str_start"], e["str_end"]),
            "coast_distance_m": disp(e["str_end"], e["str_end"] + 3.0),
            "speed_at_release": peak_after}


# --------------------------------------------------------------------------
# Summaries
# --------------------------------------------------------------------------

def fmt(x, nd=2):
    return "-" if x is None or (isinstance(x, float) and math.isnan(x)) else f"{x:.{nd}f}"


def screens_md(rows):
    lines = ["| fixture | renderer | MAE | masked | >8 px | lowfreq | status |",
             "|---|---|---:|---:|---:|---:|---|"]
    arena = [r for r in rows if r["id"].startswith("arena-") and r["status"] == "unreachable"]
    rows = [r for r in rows if r not in arena]
    for r in rows:
        if r["status"] == "ok":
            lines.append(f"| {r['id']} | {r['renderer']} | {r['mae']:.2f} | {r['masked']:.2f} | "
                         f"{r['over8'] * 100:.1f}% | {r['lowfreq']:.2f} | ok |")
        else:
            lines.append(f"| {r['id']} | {r['renderer']} | - | - | - | - | "
                         f"{r['status']}: {r.get('reason', '')} |")
    if arena:
        lines.append("| arena-* (%d references: select/intro/countdown/f2/f3/f3b/f4 x 8 arenas) | - | - | - | - | - | "
                     "unreachable: no app route to a per-arena match camera still |" % (len(arena) // 2 or len(arena)))
    ok = [r for r in rows if r["status"] == "ok"]
    for rd in sorted({r["renderer"] for r in ok}):
        sub = [r for r in ok if r["renderer"] == rd]
        lines.append(f"| **mean of {len(sub)}** | {rd} | {statistics.fmean(r['mae'] for r in sub):.2f} | "
                     f"{statistics.fmean(r['masked'] for r in sub):.2f} | "
                     f"{statistics.fmean(r['over8'] for r in sub) * 100:.1f}% | "
                     f"{statistics.fmean(r['lowfreq'] for r in sub):.2f} | |")
    return "\n".join(lines)


def gameplay_md(g):
    if "error" in g:
        return "gameplay: " + g["error"]
    L = ["Original fight length used: %.1f s.  Ours: %s" % (g["original_fight_seconds"], g["ours_summary"]),
         "", "| robot | metric | original | ours |", "|---|---|---:|---:|"]
    for p in (0, 1):
        o, s = g["trace"]["original"].get(p), g["trace"]["ours"].get(p)
        if not o or not s:
            continue
        for k in ("speed_mean", "speed_p50", "speed_p90", "speed_max", "ground_p90",
                  "moving_frac", "path_m", "net_m", "hp_start", "hp_end"):
            L.append(f"| {p} | {k} | {fmt(o[k])} | {fmt(s[k])} |")
    d = g["damage"]
    L += ["", "| damage | original | ours |", "|---|---:|---:|"]
    for k in ("events", "weapon_events", "env_events", "events_per_s", "total"):
        L.append(f"| {k} | {fmt(d['original'][k])} | {fmt(d['ours'][k])} |")
    labels = ["<5", "5-10", "10-20", "20-50", ">=50"]
    for i, lab in enumerate(labels):
        L.append(f"| magnitude {lab} | {d['original']['hist'][i]} | {d['ours']['hist'][i]} |")
    dv = g.get("drive", {})
    if "original" in dv:
        L += ["", f"Drive test ({dv['reference']}):", "", "| metric | original | ours |", "|---|---:|---:|"]
        for k in ("turn_rate_rad_s", "turn_rate_peak", "top_speed", "t_to_90pct_s", "straight_distance_m", "speed_at_release", "coast_distance_m"):
            L.append(f"| {k} | {fmt(dv['original'][k])} | {fmt(dv['ours'][k])} |")
    elif dv:
        L += ["", "Drive test: " + (dv.get("skipped") or dv.get("error", ""))]
    return "\n".join(L)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=None)
    ap.add_argument("--only", default=None, help="comma list of fixture id prefixes")
    ap.add_argument("--renderer", choices=("app", "shell", "both"), default="both")
    ap.add_argument("--gameplay", action="store_true")
    ap.add_argument("--gameplay-only", action="store_true")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--list", action="store_true")
    args = ap.parse_args()
    if args.list:
        for f in FIXTURES + find_arena_fixtures():
            print(f["id"], f["ref"], "UNREACHABLE" if "unreachable" in f else "")
        return 0
    for exe in (APP, SHELL):
        if not os.path.exists(exe) and not args.gameplay_only:
            sys.exit("fidelity: missing %s (cd engine && cargo build --release --offline)" % exe)
    if not os.path.isdir(ORIG):
        sys.exit("fidelity: no originals at %s (git-ignored local data)" % ORIG)
    out = args.out or os.path.join(LOCAL, "oracle", "fidelity", "latest")
    os.makedirs(out, exist_ok=True)
    result = {"date": datetime.datetime.now().isoformat(timespec="seconds"), "size": list(SIZE)}
    md = ["# Fidelity run %s" % result["date"], ""]
    if not args.gameplay_only:
        rows = screens(args, out)
        result["screens"] = rows
        md += ["## Screens (ours vs original, 1024x768)", "", screens_md(rows), ""]
    if args.gameplay or args.gameplay_only:
        g = gameplay(args, out)
        result["gameplay"] = g
        md += ["## Gameplay", "", gameplay_md(g), ""]
    open(os.path.join(out, "summary.json"), "w").write(json.dumps(result, indent=1))
    open(os.path.join(out, "summary.md"), "w").write("\n".join(md))
    print("\n".join(md))
    print("wrote", out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
