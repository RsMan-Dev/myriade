#!/usr/bin/env bash
# sweep.sh FORM K : the calibration of the batch push for one (form, k); prints one line of measures
export HORIZON_TEST_RESOURCES=/tmp/claude-1000/-home-rsman-projects-horizon-server-tests/f80a876d-5501-4517-a3ca-22948ac8b0bb/scratchpad/new-main-perf/resources
export PUSH_FORM=$1 PUSH_K=$2
D=/home/rsman/projects/horizon-server-tests/wt/.tgt-ecs/fast
T=$(ls -t $D/deps/crowd_vs_mob-* | grep -v '\.d$' | head -n1)
line=""
for sp in 0.8 0.5; do
  r=$($D/crowd_trace ecs 2000 $sp 600 2>&1 | grep "^ecs t=" | awk '/t= *(100|300|600) /{print $NF}' | tr '\n' ' ')
  line="$line crowd$sp: $r|"
done
for s in cow chicken zombie; do
  out=$(HORIZON_SPECIES=minecraft:$s $T --nocapture two_overlapping 2>&1 | grep "^t=10 ")
  sep=$(echo "$out" | sed -E 's/.*mob \[([-0-9.]+), ([-0-9.]+)\] \| ecs \[([-0-9.]+), ([-0-9.]+)\].*/\2 \1 \4 \3/' | awk '{printf "mob %.3f ecs %.3f", $1-$2, $3-$4}')
  herd=$(HORIZON_SPECIES=minecraft:$s $T --nocapture tight_herd 2>&1 | grep "overlapping pairs at ticks" | sed -E 's/.*mob \[([0-9]+), ([0-9]+)\], ecs batch \[([0-9]+), ([0-9]+)\].*/mob \1 \2 ecs \3 \4/')
  line="$line $s: sep $sep herd $herd |"
done
echo "form=$1 k=$2 $line"
