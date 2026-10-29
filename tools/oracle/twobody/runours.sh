#!/bin/bash
# runours.sh <statedir> [binary] [steps]: examples/twoprobe on every t*.state of mkours.py, RA2_H17_DUMP alongside
D=$1; E=${2:-$(cd $(dirname $0)/../../.. && pwd)/engine/target/release/examples/twoprobe}; N=${3:-180}
G=${RA2_GAME:-$(cd $(dirname $0)/../../.. && pwd)/local/game}
ls $D/t*.state | xargs -P ${J:-4} -I{} sh -c "RA2_H17_DUMP={}.dump $E $G/Arenas/box.py $G/AI/Team0/Bot0.bot $G/AI/Team0/Bot1.bot {} $N > {}.out 2>&1"
