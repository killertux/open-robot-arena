#!/usr/bin/env python3
"""run_gjkreduce.py <cases.json> <out.jsonl>: run ra2_gjkreduce.js in the live original (a match must be
running: the agent fires on the first 0x55c390 call).  Needs the session on RA2_FRIDA_HOST."""
import json, os, sys, time
import frida
HERE = os.path.dirname(os.path.abspath(__file__))
import shutil
shutil.copy(sys.argv[1], os.path.expanduser("~/.var/app/org.winehq.Wine/data/wine/drive_c/ra2oracle/reduce_cases.json"))
src = open(os.path.join(HERE, "frida", "ra2_gjkreduce.js")).read()
host = os.environ.get("RA2_FRIDA_HOST", "127.0.0.1:27142")
dev = frida.get_device_manager().add_remote_device(host)
pid = [p for p in dev.enumerate_processes() if p.name == "Robot Arena 2.exe"][0].pid
sess = dev.attach(pid)
rows = []; fin = []
def on_msg(m, d):
    if m.get("type") == "send":
        p = m["payload"]
        if p[0] == "K*": rows.extend(p[1])
        elif p[0] == "done": fin.append(p[1])
        else: rows.append(p); print(p, file=sys.stderr)
    else:
        print("agent error", m, file=sys.stderr)
sc = sess.create_script(src); sc.on("message", on_msg); sc.load()
t0 = time.time()
while not fin and time.time() - t0 < 120: time.sleep(0.5)
sc.unload(); sess.detach()
with open(sys.argv[2], "w") as f:
    for r in rows: f.write(json.dumps(r) + "\n")
print("rows", len(rows), "done", fin)
