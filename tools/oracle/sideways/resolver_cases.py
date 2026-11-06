#!/usr/bin/env python3
"""resolver_cases.py capture.jsonl out.txt [chassis_mass]: for every resolver entry of the capture with a contact of the
chassis, one block of text for examples/resolverreplay (docs/29 12be): the chassis' state at the entry (B row), its local
inertia (P row), its contacts and the velocities the resolver left (V row).  Needs a capture with V rows (ra2_tipab.js, 12be).

  case <trial> <step> <seq>
  body <mass> <com3> <v3> <w3> <R9 row-major> <I9 row-major>
  contact <normal3> <point3> <dist> <muA> <muB> <eA> <eB> <a_is_chassis>
  post <v3> <w3>
"""
import json, sys

path, out = sys.argv[1], sys.argv[2]
mass = float(sys.argv[3]) if len(sys.argv) > 3 else 163.49465942382812
rows = [json.loads(l) for l in open(path)]
inertia = {}
for r in rows:
    if r[0] == 'P' and abs(r[3][0x190 // 4] - mass) < 0.01:
        f = r[3]
        inertia[r[2]] = [f[0x19c // 4 + 4 * i + j] for i in range(3) for j in range(3)]
trial, step = -1, 0
o = open(out, 'w')
cur = None
n = 0
for i, r in enumerate(rows):
    if r[0] == 'X' and r[2] == 'place':
        trial += 1
        step = 0
    elif r[0] == 'X' and r[2] == 'done':
        trial = -1 if False else trial
        cur = None
        step = 10**9
    elif r[0] == 'A':
        step += 1
    elif r[0] == 'R' and 0 <= trial and step < 10**8:
        ch = None
        for c in rows[i + 1:i + 40]:
            if c[0] == 'B' and abs(c[3] - mass) < 0.01:
                ch = c
                break
            if c[0] not in ('B',):
                break
        if not ch or ch[2] not in inertia:
            continue
        cs = [c for c in r[3] if c[0] == ch[2] or c[1] == ch[2]]
        if not cs:
            continue
        post = None
        for c in rows[i + 1:i + 80]:
            if c[0] == 'V' and c[2] == ch[2] and c[1] == r[1]:
                post = c
                break
        if post is None:
            continue
        m = ch[7]
        R = [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]]  # B row = columns consecutively
        o.write('case %d %d %d tol %r\n' % (trial, step, r[1], r[2]))
        o.write('body %r %s\n' % (ch[3], ' '.join(repr(v) for v in ch[4] + ch[5] + ch[6] + R + inertia[ch[2]])))
        for c in cs:
            o.write('contact %s %r %r %r %r %r %r %d\n' % (' '.join(repr(v) for v in c[2] + c[3]), c[4], c[5], c[6], c[7], c[8], 0, 1 if c[0] == ch[2] else 0))
        o.write('post %s\n' % ' '.join(repr(v) for v in post[3] + post[4]))
        n += 1
print(n, 'cases')
