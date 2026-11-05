"""al_check.py <actorder jsonl>...: replay a capture's `AD`/`RM` rows (addAction / removeAction of ra2_actorder.js) through the original's sort (al_sort.py)
and compare the list with every `AS` row the capture read (docs/29 12ba). Prints the number of lists that agree and any that do not."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import al_sort
for fn in sys.argv[1:]:
    L = []; ok = bad = 0
    for l in open(fn):
        try: r = json.loads(l)
        except Exception: continue
        if r[0] == 'AD': al_sort.add(L, r[2])
        elif r[0] == 'RM': al_sort.remove(L, r[2])
        elif r[0] == 'AS':
            real = [x[0] for x in r[2]]
            if not L:
                L = [(x, 0) for x in real]  # the first list holds what was added before the agent hooked (the drag action)
                continue
            if real == [x[0] for x in L]: ok += 1
            else:
                bad += 1; print('MISMATCH at', r[1], len(real), len(L))
    print(fn, 'lists equal', ok, 'different', bad)
