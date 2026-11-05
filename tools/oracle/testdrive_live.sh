#!/bin/bash
# The live Test Robot run behind docs/36 9.32: the whole Bot Lab route up to the Test Robot tab, then the key route
# (06-testdrive.steps, an `eval Date.now()` before every key / click / shot) with the robot's pose sampled by
# ra2_python.py. Run under the live lock:   flock ~/.cache/ra2-live.lock tools/oracle/testdrive_live.sh OUTDIR
# The game re-saves Teams/ (Back to HQ) and Custom Textures/: they are copied to OUTDIR/prefix-before and put back, with a
# sha256 list compared (the rules for the shared prefix).
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
for d in Teams AI "Custom Textures"; do [ -d "$GD/$d" ] && cp -a "$GD/$d" "$B/"; done
cp -a "$GD/ra2.cfg" "$B/"
sums() { (cd "$GD" && find Teams AI "Custom Textures" ra2.cfg -type f -exec sha256sum {} + 2>/dev/null | sort -k2); }
sums > "$OUT/sha-before.txt"
cat "$R/00-menu-to-lab.steps" "$R/01-chassis.steps" "$R/02-armor.steps" "$R/03-components-a.steps" "$R/03-components-b.steps" \
    "$R/04-wiring.steps" "$R/05-paint.steps" | sed "s#SHOTS/#$OUT/#" > "$OUT/pre.steps"
grep -v '^#' "$R/06-testdrive.steps" | awk '/^(key|click|shot)/ {print "eval Date.now()"} {print}' | sed "s#SHOTS/#$OUT/#" > "$OUT/drive.steps"
cd "$REPO"
tools/capture/original_session.sh start || echo "start failed"
sleep 5
$PY tools/oracle/ra2_drive.py -f "$OUT/pre.steps" > "$OUT/pre.log" 2>&1
$PY tools/oracle/ra2_python.py --sample "repr([(p, plus.getLocation(p), plus.getHeading(p), plus.getSpeed(p)) for p in plus.getPlayers()]) + ' ' + repr(__import__('time').time())" \
    --every 3 --seconds 32 > "$OUT/pose.tsv" 2> "$OUT/pose.err" &
SAMPLER=$!
sleep 2
$PY tools/oracle/ra2_drive.py -f "$OUT/drive.steps" > "$OUT/drive.log" 2>&1
wait $SAMPLER
tools/capture/original_session.sh stop
for d in Teams AI "Custom Textures"; do rm -rf "${GD:?}/$d"; [ -d "$B/$d" ] && cp -a "$B/$d" "$GD/$d"; done
cp -a "$B/ra2.cfg" "$GD/ra2.cfg"
sums > "$OUT/sha-after.txt"
cmp "$OUT/sha-before.txt" "$OUT/sha-after.txt" && echo "prefix restored"
