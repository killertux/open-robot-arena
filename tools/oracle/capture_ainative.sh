#!/bin/bash
# capture_ainative.sh <OUT dir (absolute, scratch)> [sim seconds]
#
# Every native query the shipped brains make in a Scout v ALARM fight in the Combat
# Arena of the ORIGINAL (ainative_install.py, run through ra2_python.py: nothing is
# written to the game folder), plus the edge-case probes of ainative_probes/ run
# at 8 s and at the end, then plus.eliminatePlayer(1) and the same state probe
# again (docs/33 section 8, docs/29 section 12an).  Needs the live lock and the
# agent's display / port in the environment (rules: one flatpak Wine instance):
#   exec 9>$HOME/.cache/ra2-live.lock; flock 9
#   RA2_CAPTURE_DISPLAY=:7 RA2_FRIDA_PORT=27147 RA2_FRIDA_HOST=127.0.0.1:27147 \
#     tools/oracle/capture_ainative.sh /abs/scratch/dir 40
# Writes OUT/ainative.txt (time, player, name, args, result[, direction, location,
# heading, upside-down for GetSpeed / GetTurning]), OUT/probe1.txt and OUT/p<x>.txt.
# The prefix's Teams/, AI/, Robot Designs/ and ra2.cfg are hashed before and after;
# copy them first (the game re-saves .bot files).
set -u
OUT=${1:?absolute scratch dir}; SECS=${2:-40}
case $OUT in /*) ;; *) echo "OUT must be absolute" >&2; exit 2;; esac
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
PY=${RA2_FRIDA_PYTHON:-$HOME/tools/frida-venv/bin/python3}
DC=$HOME/.var/app/org.winehq.Wine/data/wine/drive_c
GD="$DC/Program Files (x86)/Robot Arena 2"
mkdir -p "$OUT" "$DC/ra2oracle"
hashes() { (cd "$GD" && find Teams AI "Robot Designs" ra2.cfg -type f -print0 | sort -z | xargs -0 sha256sum); }
hashes > "$OUT/prefix.before"
trap '"$HERE/../capture/original_session.sh" stop; hashes > "$OUT/prefix.after"; cmp -s "$OUT/prefix.before" "$OUT/prefix.after" && echo PREFIX_OK || echo PREFIX_CHANGED' EXIT
"$HERE/../capture/original_session.sh" start || exit 1
cd "$HERE"
$PY ra2_drive.py -f routes/exhibition_box_scout_alarm.steps >/dev/null
$PY -c "import sys; sys.path.insert(0,'.'); import capture_drive as cd; cd.start_match('188')"
$PY ra2_python.py ainative_install.py
sleep 6
$PY ra2_python.py ainative_probes/probe1.py > "$OUT/probe1.txt"
while :; do
  t=$($PY ra2_python.py -c 'import plus; out.write(repr(plus.getTimeElapsed()))')
  python3 -c "import sys; sys.exit(0 if float('$t')>=$SECS else 1)" && break
  sleep 8
done
for p in a b c d e f g; do   # d (GetComponentType(99)) hangs the exec: the timeout is the answer
  timeout 40 $PY ra2_python.py ainative_probes/p$p.py > "$OUT/p$p.txt" 2>&1
done
$PY ra2_python.py -c 'import plus; plus.eliminatePlayer(1); out.write("elim")'
sleep 2
timeout 40 $PY ra2_python.py ainative_probes/pa.py > "$OUT/pa.after_elimination.txt" 2>&1
cp "$DC/ra2oracle/ainative.txt" "$OUT/ainative.txt"
sleep 4
$PY ra2_drive.py click 717 575 hide wait 3 >/dev/null
