#!/bin/bash
# The live decal band-restore probe behind docs/36 9.33: menu -> lab -> chassis -> armour default, then the Frida agent
# frida/ra2_decal.js (FUN_00426130, the snapshot FUN_00423ea0, the four CopyRects) while 05e-export-skins.steps runs
# (export, decal, export, texture fill, export). Run under the lock:  flock ~/.cache/ra2-live.lock tools/oracle/decal_live.sh OUTDIR
# The prefix's Teams/ AI/ "Robot Designs"/ "Custom Textures"/ ra2.cfg are copied to OUTDIR/prefix-before and put back (sha256 lists compared).
set -u
OUT=$(mkdir -p "$1" && cd "$1" && pwd)
REPO=$(cd "$(dirname "$0")/../.." && pwd)
R=$REPO/tools/oracle/routes/botlab
GD="$HOME/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2"
export RA2_CAPTURE_DISPLAY=${RA2_CAPTURE_DISPLAY:-:7} RA2_FRIDA_PORT=${RA2_FRIDA_PORT:-27147}
export RA2_FRIDA_HOST=127.0.0.1:$RA2_FRIDA_PORT
PY=$HOME/tools/frida-venv/bin/python3
B=$OUT/prefix-before
mkdir -p "$B"
for d in Teams AI "Robot Designs" "Custom Textures"; do [ -d "$GD/$d" ] && cp -a "$GD/$d" "$B/"; done
cp -a "$GD/ra2.cfg" "$B/"
sums() { (cd "$GD" && find Teams AI "Robot Designs" "Custom Textures" ra2.cfg -type f -exec sha256sum {} + 2>/dev/null | sort -k2); }
sums > "$OUT/sha-before.txt"
for f in ${PRE:-00-menu-to-lab 01-chassis 02-armor}; do cat "$R/$f.steps"; done | sed "s#SHOTS/#$OUT/#" > "$OUT/pre.steps"
sed "s#SHOTS/#$OUT/#" "$R/05e-export-skins.steps" > "$OUT/drive.steps"
cd "$REPO"
tools/capture/original_session.sh start || echo "start failed"
sleep 5
$PY tools/oracle/ra2_drive.py -f "$OUT/pre.steps" > "$OUT/pre.log" 2>&1
AGENT=tools/oracle/frida/ra2_decal.js
[ -n "${NODUMP:-}" ] && { sed "s/^const DUMP = true;/const DUMP = false;/" $AGENT > "$OUT/agent-nodump.js"; AGENT=$OUT/agent-nodump.js; }
$PY tools/oracle/ra2_hook.py $AGENT --seconds "${HOOK_SECONDS:-200}" --bindir "$OUT/bin" > "$OUT/decal.jsonl" 2> "$OUT/decal.err" &
HOOK=$!
sleep 3
$PY tools/oracle/ra2_drive.py -f "$OUT/drive.steps" > "$OUT/drive.log" 2>&1
sleep 2
mkdir -p "$OUT/exports"; cp -a "$GD/Custom Textures/"*.bmp "$OUT/exports/" 2>/dev/null
kill $HOOK 2>/dev/null; wait $HOOK 2>/dev/null
tools/capture/original_session.sh stop
for d in Teams AI "Robot Designs" "Custom Textures"; do rm -rf "${GD:?}/$d"; [ -d "$B/$d" ] && cp -a "$B/$d" "$GD/$d"; done
cp -a "$B/ra2.cfg" "$GD/ra2.cfg"
sums > "$OUT/sha-after.txt"
cmp "$OUT/sha-before.txt" "$OUT/sha-after.txt" && echo "prefix restored"
