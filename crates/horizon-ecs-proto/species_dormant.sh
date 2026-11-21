#!/usr/bin/env bash
# Cost control of a SLEEPING creature of every species in configuration: the zone of simulation on (`ZONE=8`), no player, so every
# creature of a herd of 2 000 is asleep (its chunk does not tick: vanilla ticks only the entities of `ServerLevel.entityTickList`).
# The whole Schedule::run per creature and tick, compared with the sleeping cow. FAILS for a species above 1.5 x the sleeping cow.
# No exception file: a sleeping creature that costs more is a defect of its sleep (something wakes it or visits it), to fix at the cause.
# HORIZON_TEST_RESOURCES and D (the directory of the benches) set; the species are those of the registry table (species/mod.rs).
set -u
cd "$(dirname "$0")"
: "${HORIZON_TEST_RESOURCES:?}"; : "${D:?}"
export HORIZON_TEST_RESOURCES SPACING=6 ZONE=8
J=$(mktemp -d); trap 'rm -rf "$J"' EXIT
per() { local v; v=$(HORIZON_SPECIES=minecraft:$1 "$D/prof_bench" idle 2000 1200 0 400 2>/dev/null | head -n1 | sed -n 's/.*whole Schedule::run)=\([0-9]*\).*/\1/p'); echo $(( ${v:-0} / 2000 )); }
SPECIES=$(grep -o '("minecraft:[a-z_]*", &\[' src/species/mod.rs | sed 's/("minecraft://;s/", &\[//' | sort -u | grep -v '^nope$')
for s in cow $SPECIES; do ( per "$s" > "$J/$s" ) & while [ "$(jobs -rp | wc -l)" -ge "$(nproc)" ]; do wait -n; done; done; wait
cow=$(cat "$J/cow"); fail=0
[ "$cow" -gt 0 ] || { echo "FAIL the sleeping cow was not measured"; exit 1; }
for s in $SPECIES; do
  v=$(cat "$J/$s")
  if [ $((v * 2)) -gt $((cow * 3)) ] || [ "$v" -eq 0 ]; then echo "FAIL $s $v instr per sleeping creature-tick (sleeping cow $cow, limit 1.5 x)"; fail=1; else echo "ok $s $v"; fi
done
exit $fail
