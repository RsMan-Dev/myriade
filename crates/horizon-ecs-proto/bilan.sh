#!/usr/bin/env bash
# bilan.sh : the reference table of the integrated branch, exact instruction counter (src/instr.rs), 1 thread
export HORIZON_TEST_RESOURCES=/tmp/claude-1000/-home-rsman-projects-horizon-server-tests/f80a876d-5501-4517-a3ca-22948ac8b0bb/scratchpad/new-main-perf/resources
export WARM=200
D=/home/rsman/projects/horizon-server-tests/wt/.tgt-ecs/fast
g() { grep -o 'instr_per_entity_tick=[0-9]*' | head -n1 | cut -d= -f2; }
declare -A ITEM=( [cow]=minecraft:wheat [pig]=minecraft:carrot [sheep]=minecraft:wheat [chicken]=minecraft:wheat_seeds [zombie]=minecraft:wheat )
for sp in cow pig sheep chicken zombie; do
  export HORIZON_SPECIES=minecraft:$sp ITEM=${ITEM[$sp]}
  S=""; [ $sp = zombie ] && S=1
  e() { "$D/goal_bench" "$@" 2>/dev/null | g; }
  m() { if [ -n "$S" ]; then SCOPE=1 "$D/mob_goal_bench" "$@" 2>/dev/null | g; else "$D/mob_goal_bench" "$@" 2>/dev/null | g; fi; }
  echo "$sp idle: ecs $(e 2000 300 0 0 0 0) ecs-eager $(EAGER=1 e 2000 300 0 0 0 0) mob $(m 2000 300 0 0 0 0)"
  echo "$sp active(1000 players, no solicitation): ecs $(e 2000 300 1000 0 0 0) ecs-eager $(EAGER=1 e 2000 300 1000 0 0 0) mob $(m 2000 300 1000 0 0 0)"
  if [ $sp != zombie ]; then echo "$sp solicited(50 players, 30% hold, 20 hurt, 20 love): ecs $(e 2000 300 50 30 20 20) mob $(m 2000 300 50 30 20 20)"; fi
done
export HORIZON_SPECIES=minecraft:zombie HUNT=1
echo "zombie chase clear (1024, spacing 6, 1024 players): ecs $("$D/cow_bench" sleep 1024 300 1 6 1024 2>/dev/null | g) mob $(SCOPE=1 "$D/mob_bench" 1024 300 1024 6 2>/dev/null | g)"
echo "zombie chase crowded (2000, spacing 2, 1000 players): ecs $("$D/cow_bench" sleep 2000 300 1 2 1000 2>/dev/null | g) mob $(SCOPE=1 "$D/mob_bench" 2000 300 1000 2 2>/dev/null | g)"
unset HUNT HORIZON_SPECIES
export HORIZON_SPECIES=minecraft:cow
for sp in 0.5 0.8; do
  echo "crowd $sp (cow, 2000, 0 player): batch $("$D/cow_bench" sleep 2000 300 1 $sp 0 2>/dev/null | g) ordered $(ORDERED_PUSH=1 "$D/cow_bench" sleep 2000 300 1 $sp 0 2>/dev/null | g) mob $("$D/mob_bench" 2000 300 0 $sp 2>/dev/null | g)"
done
