"""tailfight_data.py capture.jsonl [...] > engine/crates/ra2sim/tests/data/tailfight_live.txt: the placed states of `capture_tailfight.py` trials
(`tipprobe live`'s state lines, tools/oracle/sideways/live_state.py) with the Scout's lean (up.y) at every substep after the release
for the first --steps substeps, for tests/tailfight_live.rs (docs/29 12bg). Run from the repository root."""
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
steps = 24
files = []
for a in sys.argv[1:]:
    if a.startswith("--steps="):
        steps = int(a.split("=")[1])
    else:
        files.append(a)
for f in files:
    L = [json.loads(line) for line in open(f)]
    meta = json.load(open(f.replace(".jsonl", ".json")))
    trial = meta["grid"]["trials"][meta["trial"]]
    pi = [i for i, r in enumerate(L) if r[0] == "X" and r[2] == "place"][0]
    d = tempfile.mkdtemp()
    subprocess.run([sys.executable, "-I", HERE + "/live_state.py", f, str(pi), d + "/st.txt"], check=True)
    lean, cur = [], None
    for r in L[pi:]:
        if r[0] == "A":
            if cur is not None:
                lean.append(cur)
            cur = None
        elif r[0] == "B" and abs(r[3] - 163.49) < 0.1:
            cur = r[7][4]
    print("trial %s" % trial["label"].replace(" ", "_"))
    for line in open(d + "/st.txt"):
        print("b " + line.strip())
    print("lean " + " ".join("%.4f" % x for x in lean[:steps]))
    print("end")
