"""ph_contacts_ours.py <dump.jsonl> <out.tsv>: from an RA2_H17_DUMP of a Compressor fight of ours the contacts between the two robots
(docs/29 12ba): per resolver entry and contact `t owner(0: Sentinel body, 1: WL body) bodyA bodyB nx ny nz (world) px py pz (world)
dist` plus the two chassis' centre, up and rotation, in WL-frame columns by the reader (ph_contacts_cmp.py)."""
import sys, json
rob = lambda i: 0 if 5 <= i <= 10 else (1 if 11 <= i <= 16 else -1)
out = open(sys.argv[2], 'w')
t = 0.0; pend = None; B = {}
for l in open(sys.argv[1]):
    if not l.startswith('["'): continue
    r = json.loads(l); k = r[0]
    if k == 'A': t += r[2]
    elif k == 'R':
        pend = [c for c in r[3] if rob(int(c[0][1:])) >= 0 and rob(int(c[1][1:])) >= 0 and rob(int(c[0][1:])) != rob(int(c[1][1:]))]
        B = {}
    elif k == 'B' and pend is not None and r[2] in ('h5', 'h11'):
        B[r[2]] = r
        if len(B) == 2:
            a, b = B['h5'], B['h11']
            for c in pend:
                f = lambda v: ' '.join('%.4f' % x for x in v)
                out.write('%.4f %s %s %s %s %.4f | %s | %s\n' % (t, c[0], c[1], f(c[2]), f(c[3]), c[4], f(a[4] + a[7]), f(b[4] + b[7])))
            pend = None
