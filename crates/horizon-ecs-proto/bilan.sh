#!/usr/bin/env bash
# RULE: build the benches with CARGO_INCREMENTAL=0 (INC=0 ./dev-rel.sh cargo build -p horizon-ecs-proto --profile fast --bins, a PERSISTENT target
# directory: only the prototype crate recompiles). An incremental build is 5 to 8 % higher on every line: never compare two build modes.
# bilan.sh : the reference table of the integrated branch, exact instruction counter (src/instr.rs, one counter per process: the figures do
# not depend on the load of the machine), every scene in ITS OWN process, all at once (as many as cores). The `Mob` columns come from the
# dated file `bilan.mob` (the Mob does not change while Horizon does not): `MOB=1 ./bilan.sh` measures them again and writes it.
# HORIZON_TEST_RESOURCES: the prepared vanilla data; D: the directory of the benches (cargo build --profile fast --bins).
: "${HORIZON_TEST_RESOURCES:?set HORIZON_TEST_RESOURCES}"
export HORIZON_TEST_RESOURCES
D=${D:?set D to the directory of the benches (target/fast)}
cd "$(dirname "$0")"
export WARM=${WARM:-200}
T=${TICKS:-300}
export NO_XP=1   # a bench player never collects an orb (see `orb::NoXp`)
J=$(mktemp -d); trap 'rm -rf "$J"' EXIT
JOBS=${JOBS:-$(nproc)}
g() { grep -o 'instr_per_entity_tick=[0-9]*' | head -n1 | cut -d= -f2; }
job() { local key=$1; shift; ( eval "$*" > "$J/$key" 2>/dev/null ) & while [ "$(jobs -rp | wc -l)" -ge "$JOBS" ]; do wait -n; done; }
# NOTE: the held item is the bench default (wheat) for every species, as it always was in this table (the old script declared an array
# `ITEM` that hid the variable, so the pig, the chicken and the zombie never got their food): changing it changes every solicited line.
SPECIES="cow pig sheep chicken zombie"
for sp in $SPECIES; do
  E="HORIZON_SPECIES=minecraft:$sp"
  for n in 500 2000; do
    job ${sp}_idle_$n "env $E $D/goal_bench $n $T 0 0 0 0 | g"
    job ${sp}_eager_$n "env $E EAGER=1 $D/goal_bench $n $T 0 0 0 0 | g"
  done
  job ${sp}_active "env $E $D/goal_bench 2000 $T 1000 0 0 0 | g"
  job ${sp}_active_eager "env $E EAGER=1 $D/goal_bench 2000 $T 1000 0 0 0 | g"
  [ $sp != zombie ] && job ${sp}_solicited "env $E $D/goal_bench 2000 $T 50 30 20 20 | g"
done
Z="HORIZON_SPECIES=minecraft:zombie HUNT=1"
job chase1 "env $Z $D/cow_bench sleep 1024 $T 1 6 1024 | g"
job chase2 "env $Z $D/cow_bench sleep 2000 $T 1 2 1000 | g"
C="HORIZON_SPECIES=minecraft:cow"
for s in 0.5 0.8; do
  job crowd${s}_batch "env $C $D/cow_bench sleep 2000 $T 1 $s 0 | g"
  job crowd${s}_ordered "env $C ORDERED_PUSH=1 $D/cow_bench sleep 2000 $T 1 $s 0 | g"
done
job zone "env -u WARM $C $D/zone_bench rings 6 10 | awk '/--- zone on/{on=1} on && /^(near|mid|far)/{ for(i=1;i<=NF;i++) if (\$i ~ /^marginal\\/creature\\/tick=/) { v=\$i; sub(/.*=/,\"\",v); if (v==\"\") v=\$(i+1); print v } }'"
it() { echo "env $C $D/prof_bench items $1 400 0 200 | head -n1 | sed -n 's/.*whole Schedule::run)=\\([0-9]*\\).*/\\1/p'"; }
job items_500 "$(it 500)"; job items_2000 "$(it 2000)"; job empty "$(it 0)"
# The Mob (only when asked): its figures are kept in bilan.mob.
if [ -n "${MOB:-}" ]; then
  for sp in $SPECIES; do
    E="HORIZON_SPECIES=minecraft:$sp"; S=""; [ $sp = zombie ] && S="SCOPE=1"
    job mob_${sp}_idle "env $E $S $D/mob_goal_bench 2000 $T 0 0 0 0 | g"
    job mob_${sp}_active "env $E $S $D/mob_goal_bench 2000 $T 1000 0 0 0 | g"
    [ $sp != zombie ] && job mob_${sp}_solicited "env $E $S $D/mob_goal_bench 2000 $T 50 30 20 20 | g"
  done
  job mob_chase1 "env $Z SCOPE=1 $D/mob_bench 1024 $T 1024 6 | g"
  job mob_chase2 "env $Z SCOPE=1 $D/mob_bench 2000 $T 1000 2 | g"
  for s in 0.5 0.8; do job mob_crowd$s "env $C $D/mob_bench 2000 $T 0 $s | g"; done
  job mob_zone "env -u WARM $C $D/mob_zone_bench rings 6 10 | sed -n 's/.*per mob: near *\\([0-9]*\\).*rest-of-zone *\\([0-9]*\\).*/\\1 \\2/p;'"
fi
wait
if [ -n "${MOB:-}" ]; then
  { echo "# the Mob's figures (instr per creature and tick), measured $(date +%F) with Horizon at $(git -C ../.. rev-parse --short HEAD) ($(git -C ../.. log -1 --format=%h -- crates/horizon-client crates/horizon-world crates/horizon-core 2>/dev/null)): MOB=1 ./bilan.sh to measure again"
    for f in "$J"/mob_*; do echo "$(basename "$f") $(tr '\n' ' ' < "$f")"; done; } > bilan.mob
fi
v() { cat "$J/$1" 2>/dev/null | head -n1 | tr -d ' \n'; }
mob() { grep -m1 "^mob_$1 " bilan.mob 2>/dev/null | cut -d' ' -f2; }
sl() { echo $(( (2000 * $(v $1_2000) - 500 * $(v $1_500)) / 1500 )); }
for sp in $SPECIES; do
  echo "$sp idle: ecs $(sl ${sp}_idle) ecs-eager $(sl ${sp}_eager) mob $(mob ${sp}_idle)"
  echo "$sp active(1000 players, no solicitation): ecs $(v ${sp}_active) ecs-eager $(v ${sp}_active_eager) mob $(mob ${sp}_active)"
  [ $sp != zombie ] && echo "$sp solicited(50 players, 30% hold, 20 hurt, 20 love): ecs $(v ${sp}_solicited) mob $(mob ${sp}_solicited)"
done
echo "zombie chase clear (1024, spacing 6, 1024 players): ecs $(v chase1) mob $(mob chase1)"
echo "zombie chase crowded (2000, spacing 2, 1000 players): ecs $(v chase2) mob $(mob chase2)"
for s in 0.5 0.8; do echo "crowd $s (cow, 2000, 0 player): batch $(v crowd${s}_batch) ordered $(v crowd${s}_ordered) mob $(mob crowd$s)"; done
set -- $(tr '\n' ' ' < "$J/zone"); mn=$(grep -m1 '^mob_zone ' bilan.mob 2>/dev/null | cut -d' ' -f2-)
echo "zone near ring (cow, 6, 10): ecs ${1:-0} mob $(echo $mn | cut -d' ' -f1)"
echo "zone mid ring (cow, 6, 10): ecs ${2:-0} mob $(echo $mn | cut -d' ' -f2)"
echo "zone far ring (cow, 6, 10): ecs ${3:-0} mob 0"
echo "items at rest (slope per stack, 500 to 2000): ecs $(( ($(v items_2000) - $(v items_500)) / 1500 ))"
echo "schedule at rest (empty world, per tick): ecs $(v empty)"
