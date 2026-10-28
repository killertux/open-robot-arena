#!/bin/sh
# A scratch install for the Bot Lab side-by-sides: Teams/ and Robot Designs/ copied from
# local/game, everything else linked. Team 0's cell 5 is made empty (scrapped), as in the live
# runs. usage: tools/labgame.sh DIR
set -eu
SRC=$(cd "$(dirname "$0")/../local/game" && pwd -P)
DST=$1
rm -rf "$DST"; mkdir -p "$DST/AI"
for e in "$SRC"/*; do
  n=$(basename "$e")
  case $n in
    Teams|"Robot Designs") cp -r "$e" "$DST/$n";;
    AI) for c in "$e"/*; do
          b=$(basename "$c")
          if [ "$b" = Teams.txt ]; then cp "$c" "$DST/AI/$b"; else ln -s "$c" "$DST/AI/$b"; fi
        done;;
    *) ln -s "$e" "$DST/$n";;
  esac
done
sed -i '0,/^Robots: 0 1 2 3 4 5$/s//Robots: 0 1 2 3 4/' "$DST/Teams/Teams.txt"
rm -f "$DST/Teams/Team0/Bot5.bot"
