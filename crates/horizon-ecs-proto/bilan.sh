#!/usr/bin/env bash
# RULE: build the benches with CARGO_INCREMENTAL=0 (e.g. INC=0 ./dev.sh cargo build -p horizon-ecs-proto --profile fast --bins, its own target
# directory). An incremental build is 5 to 8 % higher on every line, the Mob's included: never compare two build modes.
# bilan.sh : the reference table of the integrated branch, exact instruction counter (src/instr.rs), 1 thread
# HORIZON_TEST_RESOURCES: the prepared vanilla data; D: the directory of the built benches (cargo build --profile fast --bins).
: "${HORIZON_TEST_RESOURCES:?set HORIZON_TEST_RESOURCES}"
export HORIZON_TEST_RESOURCES
export WARM=200
D=${D:?set D to the directory of the benches (target/fast)}
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
# The three rings of the simulated chunks (cow, spacing 6, view = simulation = 10): instructions per creature and per tick, the
# marginal cost of the creature (ECS) and of the mob (the Mob's pass), near (< 30 blocks), mid (in the zone, > 36 blocks), far.
unset HUNT WARM
export HORIZON_SPECIES=minecraft:cow
zr=$("$D/zone_bench" rings 6 10 2>/dev/null | awk '/--- zone on/{on=1} on && /^(near|mid|far)/{ for(i=1;i<=NF;i++) if ($i ~ /^marginal\/creature\/tick=/) { v=$i; sub(/.*=/,"",v); if (v=="") v=$(i+1); print v } }')
mn=$("$D/mob_zone_bench" rings 6 10 2>/dev/null | sed -n 's/.*per mob: near *\([0-9]*\).*rest-of-zone *\([0-9]*\).*/\1 \2/p;')
set -- $zr
echo "zone near ring (cow, 6, 10): ecs ${1:-0} mob $(echo "$mn" | sed -n 1p | cut -d' ' -f1)"
echo "zone mid ring (cow, 6, 10): ecs ${2:-0} mob $(echo "$mn" | sed -n 2p | cut -d' ' -f2)"
echo "zone far ring (cow, 6, 10): ecs ${3:-0} mob 0"
# Items lying on the ground (2 000 stacks of stone at rest, no creature): instructions per item and per tick (the whole schedule).
items=$("$D/prof_bench" items 2000 400 0 200 2>/dev/null | head -n1 | sed -n 's/.*whole Schedule::run)=\([0-9]*\).*/\1/p')
echo "items at rest (2000 stacks): ecs $(( ${items:-0} / 2000 ))"
# The fixed cost of the schedule: no creature, no item, no player (instructions per tick of the whole Schedule::run). Every module
# adds systems, run conditions and sync points to it; the per-creature lines above carry it divided by 2 000.
empty=$("$D/prof_bench" items 0 400 0 200 2>/dev/null | head -n1 | sed -n 's/.*whole Schedule::run)=\([0-9]*\).*/\1/p')
echo "schedule at rest (empty world, per tick): ecs ${empty:-0}"
