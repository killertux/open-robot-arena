#!/usr/bin/env python3
"""Measure the two shell renderers against each other, screen by screen.

`ra2-render`'s `uishot` draws a `.gib` from `ra2ui`'s resolved rectangles;
`tools/ui_compose.py` reads the same `.gib` and resolves it itself. Neither
reads the other's layout, so this is a real cross-check: the table below says
how far apart the two pictures are, and where.

    tools/ui_parity.sh [--game DIR] [--width W] [--height H] [--check]
                       [--threshold N] [screen.gib ...]

With no screens, the ten below run. Images and a machine-readable report go to
`local/parity/` (the main worktree's `local/`, so a linked worktree uses the
same place as the oracle capture). Only the Python standard library is used;
the numbers are `tools/oracle/compare.py`'s: mean absolute per-channel
difference, in units of 0..255, and 16x16 block means.

A block is counted as *differing* when its mean is above 1.0/255. That is a
deliberately tight reading: a rule that moves a box or a baseline makes whole
blocks differ, while the leftover antialiasing noise does not hide it. `MAE`
is over the whole screen. `quads` is what `uishot` reported drawing, which is
the fair count for the GPU path; the composite's count is in the JSON.
"""

import argparse
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools", "oracle"))

import compare  # noqa: E402  (the shared measurement, stdlib only)

DEFAULT_SCREENS = [
    "UI/mainmenu/mainmenu.gib",
    "UI/common/matchresults.gib",
    "UI/common/statusmeters.gib",
    "UI/botlab/botlabwin.gib",
    "UI/teamhq/hq_league.gib",
    "UI/exhibition/exhib_setup.gib",
    "UI/common/gamewindow.gib",
    "UI/common/result_cell.gib",
    "UI/teamhq/season_results.gib",
    "UI/teamhq/event_bracket.gib",
]

BLOCK_DIFFERENT = 1.0  # mean channel difference, 0..255
BLOCK_VISIBLE = 8.0


def local_root():
    """The main worktree's `local/`: ignored, and shared with the oracle."""
    try:
        common = subprocess.run(
            ["git", "-C", ROOT, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        shared = os.path.join(os.path.dirname(common), "local")
        if os.path.isdir(shared) or not os.path.isdir(os.path.join(ROOT, "local")):
            return shared
    except (OSError, subprocess.CalledProcessError):
        pass
    return os.path.join(ROOT, "local")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("screens", nargs="*", help="screens, relative to the game's UI/ or absolute")
    ap.add_argument("--game", default=None)
    ap.add_argument("--uishot", default=None, help="the uishot binary")
    ap.add_argument("--width", type=int, default=1280)
    ap.add_argument("--height", type=int, default=1005)
    ap.add_argument("--out", default=None, help="image/report directory")
    ap.add_argument("--threshold", type=float, default=0.75,
                    help="--check ceiling for every screen's MAE (default 0.75/255)")
    ap.add_argument("--check", action="store_true",
                    help="exit 1 when a screen is above --threshold")
    args = ap.parse_args()

    game = args.game or os.environ.get("RA2_GAME") or os.path.join(ROOT, "local", "game")
    if not os.path.isdir(os.path.join(game, "UI")):
        print(f"ui_parity: no game data at {game}/UI -- skipping", file=sys.stderr)
        return 0

    uishot = args.uishot or os.environ.get("RA2_UISHOT")
    if not uishot:
        for profile in ("debug", "release"):
            candidate = os.path.join(ROOT, "engine", "target", profile, "examples", "uishot")
            if os.path.exists(candidate):
                uishot = candidate
                break
    if not uishot or not os.path.exists(uishot):
        print("ui_parity: no uishot binary; run `cargo build -p ra2-render --example uishot`",
              file=sys.stderr)
        return 2

    out = args.out or os.environ.get("RA2_PARITY_OUT") or os.path.join(local_root(), "parity")
    os.makedirs(out, exist_ok=True)
    screens = args.screens or DEFAULT_SCREENS

    rows = []
    failed = []
    for item in screens:
        path = item
        if not os.path.exists(path):
            path = os.path.join(game, item)
        if not os.path.exists(path):
            path = os.path.join(game, "UI", item)
        if not os.path.exists(path):
            print(f"ui_parity: no such screen: {item}", file=sys.stderr)
            return 2
        ident = os.path.basename(path)[:-4]
        ours_png = os.path.join(out, f"{ident}__uishot.png")
        compose_png = os.path.join(out, f"{ident}__compose.png")
        diff_png = os.path.join(out, f"{ident}__diff.png")

        env = dict(os.environ, RA2_GAME=game)
        shot = subprocess.run(
            [uishot, path, ours_png, str(args.width), str(args.height)],
            capture_output=True,
            text=True,
            env=env,
        )
        if shot.returncode != 0:
            print(f"ui_parity: uishot failed on {path}:\n{shot.stderr}", file=sys.stderr)
            return 2
        quads = 0
        words = shot.stdout.replace(",", " ").split()
        for i, word in enumerate(words):
            if word == "quads" and i:
                try:
                    quads = int(words[i - 1])
                except ValueError:
                    quads = 0
                break

        compose = subprocess.run(
            [
                sys.executable,
                os.path.join(ROOT, "tools", "ui_compose.py"),
                path,
                compose_png,
                "--width", str(args.width),
                "--height", str(args.height),
                "--game", game,
            ],
            capture_output=True,
            text=True,
            env=env,
        )
        if compose.returncode != 0:
            print(f"ui_parity: ui_compose.py failed on {path}:\n{compose.stderr}", file=sys.stderr)
            return 2
        drawables = 0
        for line in compose.stdout.splitlines():
            if "drawables" in line:
                for i, word in enumerate(line.replace(",", " ").split()):
                    if word == "drawables" and i:
                        try:
                            drawables = int(line.replace(",", " ").split()[i - 1])
                        except ValueError:
                            drawables = 0

        try:
            w, h, a = compare.read_png(ours_png)
            _, _, b = compare.read_png(compose_png)
        except (OSError, ValueError) as e:
            print(f"ui_parity: {e}", file=sys.stderr)
            return 2
        if (w, h) != (args.width, args.height):
            print(f"ui_parity: {ident} rendered {w}x{h}, wanted {args.width}x{args.height}",
                  file=sys.stderr)
            return 2
        whole = compare.stats(a, b, w, h, 8)
        diff, blocks, bx, _by = compare.differences(a, b, w, h)
        means = [bl[0] / bl[1] for bl in blocks]
        differing = sum(1 for v in means if v > BLOCK_DIFFERENT)
        visible = sum(1 for v in means if v > BLOCK_VISIBLE)
        top = sorted(
            ((v, (i % bx) * compare.BLOCK, (i // bx) * compare.BLOCK)
             for i, v in enumerate(means)),
            reverse=True,
        )[:5]
        row = {
            "screen": ident,
            "path": os.path.relpath(path, game),
            "size": [w, h],
            "quads": quads,
            "compose_drawables": drawables,
            "blocks_differing": differing,
            "blocks_visible": visible,
            "blocks_total": len(means),
            "block_mean_over_1": round(differing / len(means), 6),
            "mae": whole["mae"]["all"],
            "mae_rgb": whole["mae"],
            "max": whole["max"],
            "fraction_over_delta_8": whole["fraction_over_delta"],
            "top_blocks": [
                {"x": x, "y": y, "mean": round(v, 3)} for v, x, y in top
            ],
            "images": {
                "uishot": os.path.relpath(ours_png, ROOT),
                "compose": os.path.relpath(compose_png, ROOT),
                "diff": os.path.relpath(diff_png, ROOT),
            },
        }
        rows.append(row)
        compare.make_sheet(
            w, h, a, b, diff,
            compare.heat_map(blocks, bx, _by),
            None,
            diff_png,
        )
        if args.check and row["mae"] > args.threshold:
            failed.append(ident)

    width = max(len(r["screen"]) for r in rows) if rows else 10
    print()
    print(f"  {'screen'.ljust(width)}  {'quads':>5}  {'blocks>1':>8}  {'blocks>8':>8}  {'MAE':>7}")
    print(f"  {'-' * width}  {'-' * 5}  {'-' * 8}  {'-' * 8}  {'-' * 7}")
    for r in rows:
        print(
            f"  {r['screen'].ljust(width)}  {r['quads']:>5}  "
            f"{r['blocks_differing']:>8}  {r['blocks_visible']:>8}  {r['mae']:>7.4f}"
        )
    print()
    for r in rows:
        spots = ", ".join(f"({b['x']},{b['y']}) {b['mean']:.1f}" for b in r["top_blocks"][:3])
        print(f"  {r['screen']}: {spots}")
    print()
    print(f"  {len(rows)} screens, MAE ceiling {args.threshold:.2f}/255; images in {out}")

    with open(os.path.join(out, "report.json"), "w") as f:
        json.dump(
            {
                "size": [args.width, args.height],
                "block": compare.BLOCK,
                "block_different_over": BLOCK_DIFFERENT,
                "threshold": args.threshold,
                "screens": rows,
            },
            f,
            indent=2,
        )
        f.write("\n")

    if failed:
        print(f"ui_parity: above {args.threshold:.2f}: {', '.join(failed)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
