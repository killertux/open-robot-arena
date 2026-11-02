#!/bin/sh
# AI-v-AI fights in the ORIGINAL with every priced hit (capture_fight.py) and,
# at the same time, every robot's pose and brain state (ra2_python.py --sample
# every 6 frames): location, isUpsideDown, the last tactic's name, the
# commanded throttle and turn, and the sensor table (robot, contacts, chassis).
#
#   RUNS=3 SIM_SECONDS=120 ARENA_ROW_Y=206 NAME=sentinel-wideload-compressor \
#     tools/oracle/capture_fightpos_set.sh AI/Team0/Bot2.bot AI/Team8/Bot2.bot
#
# The two robots are copied over the *Wine prefix's* AI/Team0/Bot0.bot and
# Bot1.bot (backed up first and restored at the end, sha256 checked; local/game
# is only read), the exhibition is set up from the main menu
# (routes/exhibition_box_scout_alarm.steps, which picks those two slots), and
# each run starts the match on the arena row at ARENA_ROW_Y (Compressor 206),
# captures, eliminates player 1 and leaves the results screen.  Needs
# original_session.sh start and the game at the main menu.  Writes
#   fight-<NAME>__live__<seq>.jsonl/.json     (capture_fight.py)
#   fightpos-<NAME>__live__<seq>.tsv          (the samples)
# into OUT (default local/oracle/original).
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
PY=${RA2_FRIDA_PYTHON:-$HOME/tools/frida-venv/bin/python3}
GAME="$HOME/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2"
SRC=${RA2_GAME_SRC:-$HERE/../../local/game}
OUT=${OUT:-$HERE/../../local/oracle/original}
NAME=${NAME:-sentinel-wideload-compressor}
RUNS=${RUNS:-3}
SECS=${SIM_SECONDS:-120}
ROW=${ARENA_ROW_Y:-206}
T0="$GAME/AI/Team0"
S0=$(sha256sum "$T0/Bot0.bot" | cut -d' ' -f1)
S1=$(sha256sum "$T0/Bot1.bot" | cut -d' ' -f1)
[ -f "$T0/Bot0.bot.fightpos-backup" ] || cp -p "$T0/Bot0.bot" "$T0/Bot0.bot.fightpos-backup"
[ -f "$T0/Bot1.bot.fightpos-backup" ] || cp -p "$T0/Bot1.bot" "$T0/Bot1.bot.fightpos-backup"
restore() {
    cp -p "$T0/Bot0.bot.fightpos-backup" "$T0/Bot0.bot" && rm -f "$T0/Bot0.bot.fightpos-backup"
    cp -p "$T0/Bot1.bot.fightpos-backup" "$T0/Bot1.bot" && rm -f "$T0/Bot1.bot.fightpos-backup"
    [ "$(sha256sum "$T0/Bot0.bot" | cut -d' ' -f1)" = "$S0" ] || echo "WARNING: Bot0.bot not restored" >&2
    [ "$(sha256sum "$T0/Bot1.bot" | cut -d' ' -f1)" = "$S1" ] || echo "WARNING: Bot1.bot not restored" >&2
}
trap restore EXIT
cp "$SRC/$1" "$T0/Bot0.bot"
cp "$SRC/$2" "$T0/Bot1.bot"
EXPR="repr([(p, plus.getLocation(p), plus.isUpsideDown(p), getattr(__import__('AI').running_ai[p].lastTactic, 'name', '-'), __import__('AI').running_ai[p].set_throttle, __import__('AI').running_ai[p].set_turn_throttle, [(s.robot, s.contacts, s.chassis) for s in __import__('AI').running_ai[p].sensors.values()]) for p in plus.getPlayers()])"
"$PY" "$HERE/ra2_drive.py" -f "$HERE/routes/exhibition_box_scout_alarm.steps" >/dev/null
run=0
while [ $run -lt "$RUNS" ]; do
    run=$((run + 1))
    echo "== run $run of $RUNS"
    "$PY" -c "import sys; sys.path.insert(0, '$HERE'); import capture_drive as cd; cd.start_match('$ROW')"
    tmp=$(mktemp)
    "$PY" "$HERE/ra2_python.py" --sample "$EXPR" --every 6 --seconds "$(python3 -c "print($SECS * ${SAMPLE_FACTOR:-1.15} + 5)")" >"$tmp" &
    sp=$!
    SKIP_START=1 NAME=$NAME OUT=$OUT SIM_SECONDS=$SECS "$PY" "$HERE/capture_fight.py"
    wait $sp
    n=0
    while [ -e "$OUT/fightpos-${NAME}__live__$(printf %03d $n).tsv" ]; do n=$((n + 1)); done
    {
        echo "time_elapsed	[(player, location, isUpsideDown, lastTactic, cmd_throttle, cmd_turn, [(sensor robot, contacts, chassis)])]"
        cat "$tmp"
    } >"$OUT/fightpos-${NAME}__live__$(printf %03d $n).tsv"
    rm -f "$tmp"
    echo "wrote fightpos-${NAME}__live__$(printf %03d $n).tsv"
    "$PY" -c "import sys; sys.path.insert(0, '$HERE'); import capture_drive as cd; cd.py('import plus; plus.eliminatePlayer(1)')" >/dev/null
    sleep 6
    "$PY" "$HERE/ra2_drive.py" click 717 575 hide wait 3 >/dev/null
done
