#!/bin/sh
# A drivable session of the original Robot Arena 2 under flatpak Wine.
#
#   tools/capture/original_session.sh start    # Xephyr + game + frida-server
#   tools/capture/original_session.sh status
#   tools/capture/original_session.sh stop     # kill all of it, restore ra2.cfg
#
# After `start` the game sits at the main menu and these work (host side, with
# the frida venv's python, default ~/tools/frida-venv/bin/python3):
#
#   tools/oracle/ra2_drive.py click 514 346 wait 3 hide shot /tmp/x.png
#   tools/oracle/ra2_drive.py -f tools/oracle/routes/exhibition_box_scout_alarm.steps
#   tools/oracle/ra2_python.py -c 'import plus; out.write(repr(plus.getPlayers()))'
#   tools/oracle/ra2_hook.py tools/oracle/frida/ra2_damage.js --seconds 60
#   DISPLAY=:5 tools/capture/xtest.py key F2      # keyboard reaches the game
#
# Why frida-server runs *inside* the sandbox: from the host, `frida -p <pid>`
# fails with "unable to perform ptrace pokedata: Input/output error" (the
# injector resolves libc symbols from the host's /lib/i386-linux-gnu, not the
# flatpak runtime's).  `flatpak enter <instance> frida-server` shares the
# sandbox's mount and pid namespaces and the host network, so the host client
# connects to 127.0.0.1:$RA2_FRIDA_PORT.  The game is pid 2 in there.
#
# Environment:
#   RA2_CAPTURE_DISPLAY  Xephyr display (default :5)
#   RA2_ORIGINAL_SIZE    1024x768 (default) or 1280x1024
#   RA2_FRIDA_PORT       27142
#   RA2_FRIDA_PYTHON     python with the frida package (~/tools/frida-venv/bin/python3)
#   RA2_FRIDA_SERVER     frida-server binary visible in the sandbox (default
#                        ~/.var/app/org.winehq.Wine/data/frida/frida-server,
#                        i.e. /var/data/frida/frida-server inside).  Get it with:
#     curl -L https://github.com/frida/frida/releases/download/17.24.2/frida-server-17.24.2-linux-x86_64.xz \
#       | xz -d > ~/.var/app/org.winehq.Wine/data/frida/frida-server && chmod +x "$_"
#                        (its version must match the host frida package).

set -eu

DISPLAY_NAME=${RA2_CAPTURE_DISPLAY:-:5}
SIZE=${RA2_ORIGINAL_SIZE:-1024x768}
WIDTH=${SIZE%x*}
HEIGHT=${SIZE#*x}
PORT=${RA2_FRIDA_PORT:-27142}
FRIDA_PY=${RA2_FRIDA_PYTHON:-$HOME/tools/frida-venv/bin/python3}
APPDATA="$HOME/.var/app/org.winehq.Wine/data"
SERVER=${RA2_FRIDA_SERVER:-$APPDATA/frida/frida-server}
GAMEDIR="$APPDATA/wine/drive_c/Program Files (x86)/Robot Arena 2"
BACKUP="$GAMEDIR/ra2.cfg.oracle-backup"
RUN=${XDG_RUNTIME_DIR:-/tmp}/ra2-oracle-session
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
XTEST=$SCRIPT_DIR/xtest.py

die() { echo "original_session: $*" >&2; exit 2; }

instance() { flatpak ps --columns=instance,application 2>/dev/null | awk '$2=="org.winehq.Wine"{print $1; exit}'; }

start() {
    mkdir -p "$RUN"
    [ -x "$SERVER" ] || die "no frida-server at $SERVER (see the header for the download)"
    [ -f "$GAMEDIR/Robot Arena 2.exe" ] || die "the game is not installed in $GAMEDIR"
    [ -z "$(instance)" ] || die "a Wine flatpak instance is already running (stop it first)"
    # Back up the prefix copy's ra2.cfg once; `stop` restores it.
    [ -f "$BACKUP" ] || cp -p "$GAMEDIR/ra2.cfg" "$BACKUP"
    sed -i \
        -e "s/^FullScreen=.*/FullScreen=1/" \
        -e "s/^FSDisplayWidth=.*/FSDisplayWidth=$WIDTH/" \
        -e "s/^FSDisplayHeight=.*/FSDisplayHeight=$HEIGHT/" \
        -e "s/^FS3DDisplayWidth=.*/FS3DDisplayWidth=$WIDTH/" \
        -e "s/^FS3DDisplayHeight=.*/FS3DDisplayHeight=$HEIGHT/" \
        "$GAMEDIR/ra2.cfg"

    if ! DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1; then
        setsid Xephyr "$DISPLAY_NAME" -screen "$SIZE" -ac -noreset >"$RUN/xephyr.log" 2>&1 &
        echo $! >"$RUN/xephyr.pid"
        for _ in $(seq 1 40); do DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1 && break; sleep 0.25; done
    fi
    DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1 || die "display $DISPLAY_NAME is not live"

    DISPLAY=$DISPLAY_NAME WINEDEBUG=-all setsid flatpak run --command=sh org.winehq.Wine -c \
        'cd "$WINEPREFIX/drive_c/Program Files (x86)/Robot Arena 2" && exec wine "Robot Arena 2.exe"' \
        >"$RUN/game.log" 2>&1 &
    # The two startup dialogs (font, DirectInput users) take Return.
    sleep 9; DISPLAY=$DISPLAY_NAME python3 "$XTEST" key Return >/dev/null
    sleep 6; DISPLAY=$DISPLAY_NAME python3 "$XTEST" key Return >/dev/null
    sleep 10

    INST=$(instance)
    [ -n "$INST" ] || die "the Wine flatpak instance did not start (see $RUN/game.log)"
    # The sandbox path of the server: $APPDATA is mounted at /var/data.
    case $SERVER in
        "$APPDATA"/*) INNER=/var/data${SERVER#"$APPDATA"} ;;
        *) die "RA2_FRIDA_SERVER must live under $APPDATA to be visible in the sandbox" ;;
    esac
    setsid flatpak enter "$INST" "$INNER" -l "127.0.0.1:$PORT" >"$RUN/frida-server.log" 2>&1 &
    sleep 3
    # Ready = the per-frame UI update is running (a startup dialog that is still
    # up blocks it); otherwise send another Return and look again.
    READY=
    for _ in 1 2 3 4 5 6; do
        F=$(timeout 30 "$FRIDA_PY" "$SCRIPT_DIR/../oracle/ra2_drive.py" --display "$DISPLAY_NAME" \
            eval uiFrames wait 1 eval uiFrames 2>/dev/null | tr '\n' ' ') || F=
        set -- $F
        if [ $# -ge 2 ] && [ "$2" -gt $(( $1 + 5 )) ]; then READY=1; break; fi
        DISPLAY=$DISPLAY_NAME python3 "$XTEST" key Return >/dev/null; sleep 3
    done
    [ -n "$READY" ] || die "the game's UI loop is not running (startup dialog still up?)"
    echo "original_session: game on $DISPLAY_NAME ($SIZE), flatpak instance $INST, frida-server 127.0.0.1:$PORT"
}

stop() {
    # frida-server can wedge after a client is killed mid-RPC; SIGKILL it.
    for p in $(pgrep -x frida-server 2>/dev/null); do kill -9 "$p" 2>/dev/null || true; done
    flatpak kill org.winehq.Wine >/dev/null 2>&1 || true
    if [ -f "$RUN/xephyr.pid" ]; then kill "$(cat "$RUN/xephyr.pid")" 2>/dev/null || true; rm -f "$RUN/xephyr.pid"; fi
    if [ -f "$BACKUP" ]; then cp -p "$BACKUP" "$GAMEDIR/ra2.cfg" && rm -f "$BACKUP"; fi
    echo "original_session: stopped"
}

status() {
    echo "flatpak instance: $(instance)"
    pgrep -a -x frida-server || echo "frida-server: not running"
    DISPLAY=$DISPLAY_NAME xdpyinfo >/dev/null 2>&1 && echo "display $DISPLAY_NAME: live" || echo "display $DISPLAY_NAME: down"
}

case ${1:-} in
    start) start ;;
    stop) stop ;;
    status) status ;;
    *) die "usage: $0 start|stop|status" ;;
esac
