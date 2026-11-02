#!/bin/bash
# Fights in the ORIGINAL with frida/ra2_parts.js (component damage, break-off,
# who is out and why, docs/29 section 12ar), from the bell. Derived from
# capture_boxweapons_set.sh; the two bots are copied over the prefix's AI/Team0
# Bot0/Bot1 slots (the exhibition route picks those).
#
#   OUT=<abs dir> SCRATCH=<abs dir> RA2_CAPTURE_DISPLAY=:9 RA2_FRIDA_PORT=27149 \
#     NAME=scout-alarm-box ARENA_ROW_Y=188 BOT0=AI/Team0/Bot0.bot BOT1=AI/Team0/Bot1.bot \
#     flock ~/.cache/ra2-live.lock tools/oracle/capture_parts_set.sh [runs]
#
# Compressor: NAME=sentinel-wideload-compressor ARENA_ROW_Y=206 BOT0=AI/Team0/Bot2.bot
# BOT1=AI/Team8/Bot2.bot. Copies the prefix's Teams/, AI/, Robot Designs/ and ra2.cfg to
# SCRATCH first and puts them back (sha256 compared) when it ends; local/game is only
# read. Writes OUT/partsfight-<NAME>__live__<n>.jsonl/.json/.tsv.
set -u
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
ROOT=$(CDPATH= cd -- "$HERE/../.." && pwd -P)
OUT=${OUT:?set OUT to an absolute directory}
SCRATCH=${SCRATCH:?set SCRATCH to an absolute directory}
RUNS=${1:-4}
export RA2_CAPTURE_DISPLAY=${RA2_CAPTURE_DISPLAY:-:9} RA2_FRIDA_PORT=${RA2_FRIDA_PORT:-27149}
export RA2_FRIDA_HOST=127.0.0.1:$RA2_FRIDA_PORT
PY=${RA2_FRIDA_PYTHON:-$HOME/tools/frida-venv/bin/python3}
GAME="$HOME/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2"
BOT0=${BOT0:-AI/Team0/Bot0.bot}; BOT1=${BOT1:-AI/Team0/Bot1.bot}
mkdir -p "$SCRATCH/prefix-before" "$OUT"
for d in Teams AI "Robot Designs"; do cp -a "$GAME/$d" "$SCRATCH/prefix-before/"; done
cp -a "$GAME/ra2.cfg" "$SCRATCH/prefix-before/"
sums() { ( cd "$GAME" && find Teams AI "Robot Designs" ra2.cfg -type f -exec sha256sum {} + | sort -k2 ); }
sums > "$SCRATCH/prefix-before.sha"
restore() {
  "$ROOT/tools/capture/original_session.sh" stop || true
  for d in Teams AI "Robot Designs"; do rm -rf "${GAME:?}/$d"; cp -a "$SCRATCH/prefix-before/$d" "$GAME/$d"; done
  cp -a "$SCRATCH/prefix-before/ra2.cfg" "$GAME/ra2.cfg"
  sums > "$SCRATCH/prefix-after.sha"
  cmp "$SCRATCH/prefix-before.sha" "$SCRATCH/prefix-after.sha" && echo "prefix restored" || echo "PREFIX MISMATCH"
}
trap restore EXIT
"$ROOT/tools/capture/original_session.sh" start || exit 1
cp "$ROOT/local/game/$BOT0" "$GAME/AI/Team0/Bot0.bot"
cp "$ROOT/local/game/$BOT1" "$GAME/AI/Team0/Bot1.bot"
"$PY" "$HERE/ra2_drive.py" -f "$HERE/routes/exhibition_box_scout_alarm.steps" >/dev/null
for run in $(seq 1 "$RUNS"); do
  echo "== run $run"
  PRIME=1 AGENT="${AGENT:-$HERE/frida/ra2_parts.js}" NAME=${NAME:-scout-alarm-box} OUT="$OUT" \
    ARENA_ROW_Y=${ARENA_ROW_Y:-188} SIM_SECONDS=${SIM_SECONDS:-125} "$PY" "$HERE/capture_boxstart.py"
  "$PY" -c "import sys; sys.path.insert(0, '$HERE'); import capture_drive as cd; cd.py('import plus; plus.eliminatePlayer(1)')" >/dev/null
  sleep 6
  "$PY" "$HERE/ra2_drive.py" click 717 575 hide wait 3 >/dev/null
done
