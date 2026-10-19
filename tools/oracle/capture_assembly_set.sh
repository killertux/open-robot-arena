#!/bin/sh
# Capture the original's assembly of arbitrary stock robots (docs/29 section 8).
#
#   tools/oracle/capture_assembly_set.sh "AI/Team13/Bot1.bot:tornado" "AI/Team8/Bot0.bot:flapjack" [...]
#
# The exhibition picker only offers Red Zone (AI/Team0) robots under our mouse
# injection, so each pair is copied over the *Wine prefix's* AI/Team0/Bot0.bot
# and Bot1.bot (the shipped files in local/game are never touched; the prefix
# copies are backed up first and restored at the end), the match is set up with
# routes/exhibition_box_scout_alarm.steps from the main menu and
# capture_assembly.py runs with START=1.  Needs original_session.sh start, the
# game at the main menu, hazards off in the arena tab.
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
PY=${RA2_FRIDA_PYTHON:-$HOME/tools/frida-venv/bin/python3}
GAME="$HOME/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2"
SRC=${RA2_GAME_SRC:-$HERE/../../local/game}
T0="$GAME/AI/Team0"
[ -f "$T0/Bot0.bot.assembly-backup" ] || cp -p "$T0/Bot0.bot" "$T0/Bot0.bot.assembly-backup"
[ -f "$T0/Bot1.bot.assembly-backup" ] || cp -p "$T0/Bot1.bot" "$T0/Bot1.bot.assembly-backup"
restore() {
    cp -p "$T0/Bot0.bot.assembly-backup" "$T0/Bot0.bot" && rm -f "$T0/Bot0.bot.assembly-backup"
    cp -p "$T0/Bot1.bot.assembly-backup" "$T0/Bot1.bot" && rm -f "$T0/Bot1.bot.assembly-backup"
}
trap restore EXIT
# Copy a stock robot into a Red Zone slot.  The Red Zone's own files are read
# from their backups (the slots themselves hold whatever was copied last).
put() {
    src="$SRC/$1"
    case $(realpath -m "$src") in
        "$(realpath -m "$T0/Bot0.bot")") src="$T0/Bot0.bot.assembly-backup" ;;
        "$(realpath -m "$T0/Bot1.bot")") src="$T0/Bot1.bot.assembly-backup" ;;
        "$(realpath -m "$T0/Bot2.bot")") src="$T0/Bot2.bot" ;;
    esac
    cp "$src" "$2"
}
while [ $# -gt 0 ]; do
    A=$1; shift
    if [ $# -gt 0 ]; then B=$1; shift; else B="AI/Team0/Bot1.bot:alarm"; fi
    fa=${A%%:*}; na=${A##*:}; fb=${B%%:*}; nb=${B##*:}
    put "$fa" "$T0/Bot0.bot"
    put "$fb" "$T0/Bot1.bot"
    echo "== $na ($fa) vs $nb ($fb)"
    "$PY" "$HERE/ra2_drive.py" -f "$HERE/routes/exhibition_box_scout_alarm.steps" >/dev/null
    "$PY" "$HERE/ra2_drive.py" shot "${SHOT_DIR:-/tmp}/assembly-$na-setup.png" >/dev/null || true
    START=1 BOTS="$na,$nb" "$PY" "$HERE/capture_assembly.py"
    "$PY" "$HERE/ra2_python.py" -c 'import plus
for p in plus.getPlayers()[1:]: plus.eliminatePlayer(p)' >/dev/null
    sleep 9
    "$PY" "$HERE/ra2_drive.py" click 717 575 hide wait 4 click 526 745 hide wait 4 >/dev/null
done
