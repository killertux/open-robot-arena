#!/bin/sh
# Weapon captures (capture_weapons.py) of two stock robots in the Combat Arena
# with its hazards OFF and both AIs neutered.
#
#   NAME=weapons-tornado-ripblade PLAN="0:Spin:1:6;1:Spin:1:6;0:Spin:0:5;1:Spin:0:5" \
#     tools/oracle/capture_weapons_set.sh AI/Team13/Bot1.bot AI/Team14/Bot1.bot
#
# The two robots are copied over the *Wine prefix's* AI/Team0/Bot0.bot and
# Bot1.bot (backed up first and restored at the end; local/game is never
# touched).  Needs original_session.sh start and the game at the main menu or
# the exhibition screen.  Neutering patches the brain classes for the rest of
# the game session, so a later fight capture needs a fresh session.
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
PY=${RA2_FRIDA_PYTHON:-$HOME/tools/frida-venv/bin/python3}
GAME="$HOME/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2"
SRC=${RA2_GAME_SRC:-$HERE/../../local/game}
T0="$GAME/AI/Team0"
[ -f "$T0/Bot0.bot.weapons-backup" ] || cp -p "$T0/Bot0.bot" "$T0/Bot0.bot.weapons-backup"
[ -f "$T0/Bot1.bot.weapons-backup" ] || cp -p "$T0/Bot1.bot" "$T0/Bot1.bot.weapons-backup"
restore() {
    cp -p "$T0/Bot0.bot.weapons-backup" "$T0/Bot0.bot" && rm -f "$T0/Bot0.bot.weapons-backup"
    cp -p "$T0/Bot1.bot.weapons-backup" "$T0/Bot1.bot" && rm -f "$T0/Bot1.bot.weapons-backup"
}
trap restore EXIT
put() {
    case "$1" in
        AI/Team0/Bot0.bot) cp "$T0/Bot0.bot.weapons-backup" "$2" ;;
        AI/Team0/Bot1.bot) cp "$T0/Bot1.bot.weapons-backup" "$2" ;;
        *) cp "$SRC/$1" "$2" ;;
    esac
}
put "$1" "$T0/Bot0.bot"
put "$2" "$T0/Bot1.bot"
"$PY" "$HERE/ra2_drive.py" -f "$HERE/routes/exhibition_box_scout_alarm.steps" >/dev/null
shot="${SHOT_DIR:-/tmp}/weapons-setup.png"
"$PY" "$HERE/ra2_drive.py" click 176 177 wait 1.5 click 322 188 wait 1 hide shot "$shot" >/dev/null
if python3 -c "import sys; sys.path.insert(0, '$HERE'); from compare import read_png
w, h, rgb = read_png('$shot')
v = [rgb[3 * (y * w + x) + c] for y in range(484, 493) for x in range(291, 300) for c in range(3)]
sys.exit(0 if sum(v) / len(v) > 130 else 1)"; then
    "$PY" "$HERE/ra2_drive.py" click 295 488 wait 1 hide shot "$shot" >/dev/null
fi
"$PY" "$HERE/ra2_drive.py" click 936 647 hide wait 14 click 512 735 hide wait 1 >/dev/null
"$PY" -c "import sys; sys.path.insert(0, '$HERE'); import capture_drive as cd; cd.py(cd.NEUTER)" >/dev/null
sleep 5
"$PY" "$HERE/capture_weapons.py"
"$PY" -c "import sys; sys.path.insert(0, '$HERE'); import capture_drive as cd; cd.py('import plus; plus.eliminatePlayer(1)')" >/dev/null
sleep 6
"$PY" "$HERE/ra2_drive.py" click 717 575 hide wait 3 >/dev/null
