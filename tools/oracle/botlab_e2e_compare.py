#!/usr/bin/env python3
"""Compare two directories of same-named screenshots of the Bot Lab e2e build.

    tools/oracle/botlab_e2e_compare.py LIVE_DIR OURS_DIR OUT_DIR [--only PREFIX]

LIVE_DIR holds the frames `ra2_drive.py` took of the original (`tools/oracle/botlab_e2e.sh live`), OURS_DIR the frames
`labshot` wrote for the same route (`... ours`). Per frame it prints the mean absolute RGB difference over the whole
frame and over the lab's regions (3D pane, right panel, weight row, tip line) and writes
`OUT_DIR/<name>.png` = live | ours | |diff| x4 (the 1024x768 frames at half size) plus `OUT_DIR/summary.tsv`.
The OS pointer (drawn by the original, never by ours) is not masked: a frame whose pointer is in view says so.
"""
import os
import sys

from PIL import Image, ImageChops, ImageStat

REGIONS = {
    "frame": (0, 0, 1024, 768),
    "pane3d": (20, 147, 492, 618),
    "right": (500, 110, 1024, 680),
    "tip": (0, 688, 1024, 712),
    "weight": (20, 630, 320, 658),
}


def mae(a, b, box):
    d = ImageChops.difference(a.crop(box), b.crop(box))
    return sum(ImageStat.Stat(d).mean) / 3


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    only = ""
    if "--only" in sys.argv:
        only = sys.argv[sys.argv.index("--only") + 1]
        args = [a for a in args if a != only]
    live, ours, out = args[:3]
    os.makedirs(out, exist_ok=True)
    rows = []
    for name in sorted(os.listdir(live)):
        if not name.endswith(".png") or not name.startswith(only):
            continue
        if not os.path.exists(os.path.join(ours, name)):
            rows.append((name, None))
            continue
        a = Image.open(os.path.join(live, name)).convert("RGB")
        b = Image.open(os.path.join(ours, name)).convert("RGB")
        if a.size != b.size:
            b = b.resize(a.size)
        vals = {k: mae(a, b, box) for k, box in REGIONS.items()}
        rows.append((name, vals))
        d = ImageChops.difference(a, b).point(lambda v: min(255, v * 4))
        w, h = a.size[0] // 2, a.size[1] // 2
        sheet = Image.new("RGB", (w * 3, h))
        for i, im in enumerate((a, b, d)):
            sheet.paste(im.resize((w, h), Image.LANCZOS), (i * w, 0))
        sheet.save(os.path.join(out, name))
    with open(os.path.join(out, "summary.tsv"), "w") as f:
        f.write("frame\t" + "\t".join(REGIONS) + "\n")
        for name, vals in rows:
            if vals is None:
                f.write(f"{name}\tmissing in ours\n")
                print(f"{name:52s} missing in ours")
                continue
            f.write(name + "\t" + "\t".join(f"{vals[k]:.2f}" for k in REGIONS) + "\n")
            print(f"{name:52s} " + " ".join(f"{k} {vals[k]:6.2f}" for k in REGIONS))


if __name__ == "__main__":
    main()
