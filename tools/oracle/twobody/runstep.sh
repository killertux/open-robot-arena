#!/bin/bash
# runstep.sh <dir> [binary] [steps]: examples/stepprobe on every tNN.state with its tNN.poses (mkposes.py), output tNN.sp
D=$(realpath $1); E=${2:-$(cd $(dirname $0)/../../.. && pwd)/engine/target/release/examples/stepprobe}; N=${3:-180}
G=${RA2_GAME:-$(cd $(dirname $0)/../../.. && pwd)/local/game}
ls $D/t*.state | sed 's/.state$//' | xargs -P ${J:-6} -I{} sh -c "$E $G/Arenas/box.py $G/AI/Team0/Bot0.bot $G/AI/Team0/Bot1.bot {}.state {}.poses $N > {}.sp 2>{}.sperr"
