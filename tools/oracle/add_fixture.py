#!/usr/bin/env python3
"""Copy a capture of the original into local/oracle/original/ and index it.

    add_fixture.py ID SRC.png "state / route description" [--oracle DIR]

The file is stored as ``<ID>__<W>x<H>__<SEQ>.png`` with the next free
three-digit SEQ, and one row is appended to ``<oracle>/manifest.tsv``
(id, size, file, captured, wine, renderer, state, sha256).  A directory SRC
(a frame sequence) is copied to ``original/<ID>__<W>x<H>__<SEQ>/`` and gets a
single row whose sha256 is over the sorted per-frame digests.
"""
import argparse
import datetime
import hashlib
import os
import shutil
import subprocess

from PIL import Image


def main_root():
    here = os.path.dirname(os.path.abspath(__file__))
    try:
        common = subprocess.check_output(
            ["git", "-C", here, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            text=True, stderr=subprocess.DEVNULL).strip()
        return os.path.dirname(common)
    except Exception:
        return os.path.abspath(os.path.join(here, "..", ".."))


def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("id")
    ap.add_argument("src")
    ap.add_argument("state")
    ap.add_argument("--oracle", default=os.path.join(main_root(), "local", "oracle"))
    ap.add_argument("--wine", default="flatpak org.winehq.Wine 9.0")
    ap.add_argument("--renderer", default="wined3d/llvmpipe")
    a = ap.parse_args()

    orig = os.path.join(a.oracle, "original")
    os.makedirs(orig, exist_ok=True)
    is_dir = os.path.isdir(a.src)
    frames = sorted(f for f in os.listdir(a.src) if f.endswith(".png")) if is_dir else None
    probe = os.path.join(a.src, frames[0]) if is_dir else a.src
    w, h = Image.open(probe).size
    size = f"{w}x{h}"
    for n in range(1000):
        name = f"{a.id}__{size}__{n:03d}"
        if not os.path.exists(os.path.join(orig, name + ".png")) and \
           not os.path.exists(os.path.join(orig, name)):
            break
    if is_dir:
        dst = os.path.join(orig, name)
        shutil.copytree(a.src, dst)
        digest = hashlib.sha256("".join(sha(os.path.join(dst, f)) for f in frames).encode()).hexdigest()
        fname = name + "/"
        state = f"{a.state} ({len(frames)} frames)"
    else:
        dst = os.path.join(orig, name + ".png")
        shutil.copyfile(a.src, dst)
        digest = sha(dst)
        fname = name + ".png"
        state = a.state
    manifest = os.path.join(a.oracle, "manifest.tsv")
    if not os.path.exists(manifest):
        with open(manifest, "w") as f:
            f.write("id\tsize\tfile\tcaptured\twine\trenderer\tstate\tsha256\n")
    row = [a.id, size, fname, datetime.date.today().isoformat(), a.wine, a.renderer,
           state.replace("\t", " "), digest]
    with open(manifest, "a") as f:
        f.write("\t".join(row) + "\n")
    print(fname)


if __name__ == "__main__":
    main()
