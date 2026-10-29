#!/usr/bin/env python3
"""Turn a `capture_flame_look.py` run with CASTLOG=1 (frida/ra2_flamecast.js rows) into the text the `flamecasts`
example reads (docs/37 section 6.2): the held poses and one line per particle step.

  flamecast_text.py <run.jsonl> [phase number, default 0] > run.txt

  gap G                      the range phase's gap
  heading H                  the flame robot's heading read before the first pin
  scout X Y Z                its chassis body (centre of mass)
  body M X Y Z QW QX QY QZ   ALARM's rigid bodies before the pin (mass, centre of mass, orientation), chassis first
  phase FX FY FZ FH AX AY AZ AH   `plus` poses of the two robots during the phase
  frame WALL DT PX PY PZ VX VY VZ   one emitter update while the casts were logged (the jet's nozzle and velocity)
  step K C FX FY FZ TX TY TZ HX HY HZ NX NY NZ DT   one particle step: live answer C (B damage, F floor, s own robot,
                             . nothing), the segment from -> to, the live hit point and normal, the step's dt
"""
import json
import sys

rows = [json.loads(l) for l in open(sys.argv[1])]
want = int(sys.argv[2]) if len(sys.argv) > 2 else 0
# the rows of range phase number `want`: from its `phase` row to the next one
phases = [r for r in rows if r["ev"] == "phase" and "gap" in r]
phase = phases[want]
lo = rows.index(phase)
hi = rows.index(phases[want + 1]) if want + 1 < len(phases) else len(rows)
sel = rows[lo:hi]
by = {}
for r in sel:
    if r["ev"].startswith("c_"):
        by.setdefault(r["k"], {})[r["ev"] + r["ph"]] = r
bod = next(r for r in rows if r["ev"] == "bodies")
pose0 = next(r for r in rows if r["ev"] == "pose")
f = lambda v: " ".join(repr(float(x)) for x in v)
print("gap", phase["gap"])
print("heading", pose0["pose"][2])
print("scout", f(bod["scout"][2]))
for b, s in zip(bod["alarm"], bod["states"]):
    print("body", b[1], f(s[0]), f(s[1]))
walls = [d["c_stepin"]["wall"] for d in by.values() if "c_stepin" in d]
first_wall, last_wall = min(walls), max(walls)
p = phase["pose"]
print("phase", f(p[1]), p[2], f(p[3]), p[4])
for r in sel:
    # the emitter's frames while the casts were logged: wall time, dt, nozzle, velocity
    if r["ev"] == "upd" and r["emitting"] and first_wall <= r["wall"] <= last_wall:
        print("frame", r["wall"], r["dt"], f(r["pos"]), f(r["vel"]))
for k, d in sorted(by.items()):
    if "c_wrapin" not in d or "c_rayout" not in d or "c_cbout" not in d:
        continue
    a = d["c_wrapin"]["a"]
    hit = d["c_rayout"]["al"] != 0
    body = "c_compout" in d
    if not hit:
        c = "."
    elif body:
        c = "B" if d["c_cbout"]["al"] else "s"
    else:
        c = "F"
    hp = d["c_rayout"]["v"][0] if hit else [0, 0, 0]
    n = d["c_rayout"]["v"][1] if hit else [0, 0, 0]
    dt = d["c_stepin"]["a"][0]["f"]
    print("step", k, c, f(a[2]["v"]), f(a[3]["v"]), f(hp), f(n), dt)
