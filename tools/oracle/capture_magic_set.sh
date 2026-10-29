#!/bin/sh
# Drive-model captures (capture_magic.py) of arbitrary stock robots in the
# Combat Arena with its hazards OFF, as capture_contacts_set.sh sets them up.
#
#   TURN_THROTTLE=100 tools/oracle/capture_magic_set.sh "AI/Team0/Bot2.bot:sentinel:0:2.0" [...]
#
# TURN_THROTTLE (default 0) drives the turn phase too, which a car-steered
# robot (a `Steering` unit) needs to turn at all; AIM=0 skips the aim leg.
#
# Each argument is <bot>:<name>:<player>:<straight seconds>; the robot is
# copied over the *Wine prefix's* AI/Team0/Bot<player>.bot (backed up first and
# restored at the end, as capture_assembly_set.sh does; local/game is never
# touched), the exhibition is set up from the main menu
# (routes/exhibition_box_scout_alarm.steps), the arena's "Hazards enabled" box
# is cleared, and the robot drives straight from its spawn pose.  Needs
# original_session.sh start and the game at the main menu.
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
PY=${RA2_FRIDA_PYTHON:-$HOME/tools/frida-venv/bin/python3}
GAME="$HOME/.var/app/org.winehq.Wine/data/wine/drive_c/Program Files (x86)/Robot Arena 2"
SRC=${RA2_GAME_SRC:-$HERE/../../local/game}
T0="$GAME/AI/Team0"
[ -f "$T0/Bot0.bot.magic-backup" ] || cp -p "$T0/Bot0.bot" "$T0/Bot0.bot.magic-backup"
[ -f "$T0/Bot1.bot.magic-backup" ] || cp -p "$T0/Bot1.bot" "$T0/Bot1.bot.magic-backup"
restore() {
    cp -p "$T0/Bot0.bot.magic-backup" "$T0/Bot0.bot" && rm -f "$T0/Bot0.bot.magic-backup"
    cp -p "$T0/Bot1.bot.magic-backup" "$T0/Bot1.bot" && rm -f "$T0/Bot1.bot.magic-backup"
}
trap restore EXIT
put() {
    case "$1" in
        AI/Team0/Bot0.bot) cp "$T0/Bot0.bot.magic-backup" "$2" ;;
        AI/Team0/Bot1.bot) cp "$T0/Bot1.bot.magic-backup" "$2" ;;
        *) cp "$SRC/$1" "$2" ;;
    esac
}
for spec in "$@"; do
    bot=$(echo "$spec" | cut -d: -f1); name=$(echo "$spec" | cut -d: -f2)
    player=$(echo "$spec" | cut -d: -f3); secs=$(echo "$spec" | cut -d: -f4)
    cp "$T0/Bot0.bot.magic-backup" "$T0/Bot0.bot"
    cp "$T0/Bot1.bot.magic-backup" "$T0/Bot1.bot"
    put "$bot" "$T0/Bot$player.bot"
    echo "== $name ($bot) as player $player, straight $secs s"
    "$PY" "$HERE/ra2_drive.py" -f "$HERE/routes/exhibition_box_scout_alarm.steps" >/dev/null
    # Arena tab, Combat Arena, and clear "Hazards enabled" if it is checked
    # (a checked box is bright at its centre).
    shot="${SHOT_DIR:-/tmp}/magic-$name-setup.png"
    "$PY" "$HERE/ra2_drive.py" click 176 177 wait 1.5 click 322 188 wait 1 hide shot "$shot" >/dev/null
    if python3 -c "import sys; sys.path.insert(0, '$HERE'); from compare import read_png
w, h, rgb = read_png('$shot')
v = [rgb[3 * (y * w + x) + c] for y in range(484, 493) for x in range(291, 300) for c in range(3)]
sys.exit(0 if sum(v) / len(v) > 130 else 1)"; then
        "$PY" "$HERE/ra2_drive.py" click 295 488 wait 1 hide shot "$shot" >/dev/null
    fi
    # Start, skip the intro and neuter the AI at once: a fighting AI leaves
    # its spawn within seconds.
    "$PY" "$HERE/ra2_drive.py" click 936 647 hide wait 14 click 512 735 hide wait 1 >/dev/null
    "$PY" -c "import sys; sys.path.insert(0, '$HERE'); import capture_drive as cd; cd.py(cd.NEUTER)" >/dev/null
    sleep 7
    SKIP_START=1 PLAYER=$player BOT=$name STRAIGHT=$secs "$PY" "$HERE/capture_magic.py"
done
