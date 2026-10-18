#!/bin/sh
# Record ONLY the original game's audio stream (not the host mix) to a WAV.
#
#   tools/capture/record_audio.sh OUT.wav [SECONDS]      # default 15
#
# The flatpak Wine game shows up as a PipeWire/PulseAudio sink input named
# "Robot Arena 2.exe"; `parec --monitor-stream=<index>` records that stream's
# output alone (48 kHz stereo s16le).  Needs the game running with sound
# playing (tools/capture/original_session.sh start).  Verified 2026-10-18: main
# menu music and a match both record with non-zero signal.
set -eu
OUT=${1:?usage: record_audio.sh OUT.wav [SECONDS]}
SECS=${2:-15}
IDX=$(pactl list sink-inputs | awk '/^Sink Input #/{id=substr($3,2)} /application.name = "Robot Arena 2.exe"/{print id; exit}')
[ -n "$IDX" ] || { echo "record_audio: no 'Robot Arena 2.exe' sink input (is it playing?)" >&2; exit 2; }
timeout "$SECS" parec --monitor-stream="$IDX" --file-format=wav --format=s16le --rate=48000 --channels=2 "$OUT" || true
echo "$OUT"
