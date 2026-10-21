#!/usr/bin/env python3
"""Burst screen grabber (system python3 with python-xlib): `xgrab.py DISPLAY DIR` writes
DIR/<unix time>.bgrx (raw root window) as fast as the server answers until SIGTERM; the geometry is
in DIR/geometry."""
import os
import signal
import sys
import time

import Xlib.X
import Xlib.display

if sys.argv[1] == "convert":
    # xgrab.py convert DIR BASE FIRST EVERY: write every EVERY-th frame as BASE-<k>.png, print "k time"
    from PIL import Image
    d, base, k, every = sys.argv[2], sys.argv[3], int(sys.argv[4]), int(sys.argv[5])
    w, h = map(int, open(os.path.join(d, "geometry")).read().split())
    names = sorted(f for f in os.listdir(d) if f.endswith(".bgrx"))
    for i, name in enumerate(names):
        if i % every == 0:
            data = open(os.path.join(d, name), "rb").read()
            Image.frombytes("RGB", (w, h), data, "raw", "BGRX").save(f"{base}-{k:03d}.png")
            print(k, name[:-5], flush=True)
            k += 1
        os.remove(os.path.join(d, name))
    sys.exit(0)
display, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
d = Xlib.display.Display(display)
root = d.screen().root
g = root.get_geometry()
open(os.path.join(out, "geometry"), "w").write(f"{g.width} {g.height}\n")
run = True


def stop(*_):
    global run
    run = False


signal.signal(signal.SIGTERM, stop)
while run:
    t = time.time()
    raw = root.get_image(0, 0, g.width, g.height, Xlib.X.ZPixmap, 0xffffffff)
    with open(os.path.join(out, "%.4f.bgrx" % t), "wb") as fh:
        fh.write(raw.data)
