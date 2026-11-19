#!/usr/bin/env bash
# Cost control of every species in configuration: the whole Schedule::run per creature and tick of a resting herd of 2 000 of the
# species, spaced 6 blocks (a body wider than the spacing pushes forever: the scene would measure the crowd, not the species), compared
# with the cow. FAILS for a species above 2.5 x the cow that is not in species_cost.allow ("name ## cause").
# HORIZON_TEST_RESOURCES and D (the directory of the benches) set; the species are those of the registry table (species/mod.rs).
set -u
cd "$(dirname "$0")"
: "${HORIZON_TEST_RESOURCES:?}"; : "${D:?}"
export HORIZON_TEST_RESOURCES SPACING=6
J=$(mktemp -d); trap 'rm -rf "$J"' EXIT
per() { local v; v=$(HORIZON_SPECIES=minecraft:$1 "$D/prof_bench" idle 2000 1200 0 400 2>/dev/null | head -n1 | sed -n 's/.*whole Schedule::run)=\([0-9]*\).*/\1/p'); echo $(( ${v:-0} / 2000 )); }
SPECIES=$(grep -o '("minecraft:[a-z_]*", &\[' src/species/mod.rs | sed 's/("minecraft://;s/", &\[//' | sort -u | grep -v '^nope$')
# Every species in its own process, all at once (the counter is per process: the figures do not depend on the load).
for s in cow $SPECIES; do ( per "$s" > "$J/$s" ) & while [ "$(jobs -rp | wc -l)" -ge "$(nproc)" ]; do wait -n; done; done; wait
cow=$(cat "$J/cow"); fail=0
for s in $SPECIES; do
  v=$(cat "$J/$s")
  if [ $((v * 2)) -gt $((cow * 5)) ]; then
    why=$(awk -F' ## ' -v k="$s" '$1==k{print $2}' species_cost.allow 2>/dev/null | head -n1)
    if [ -n "$why" ]; then echo "ok $s $v instr per creature-tick (cow $cow), allowed: $why"; else echo "FAIL $s $v instr per creature-tick (cow $cow, limit 2.5 x)"; fail=1; fi
  else echo "ok $s $v"; fi
done
exit $fail
