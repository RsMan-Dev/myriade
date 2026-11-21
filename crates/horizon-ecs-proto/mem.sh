#!/usr/bin/env bash
# mem.sh : bytes per entity at a FIXED terrain (the barrier's memory line): the slope of the allocator between N = 4 000 and 8 000 animals in
# the same 400 x 400 block world, after 300 ticks (what the tables, the heaps and the retained capacities cost per entity), for a cow, a
# zombie and a wolf; and the tables that are alive and the ones that are left empty (retained capacity). D: the directory of the benches.
: "${HORIZON_TEST_RESOURCES:?set HORIZON_TEST_RESOURCES}"
export HORIZON_TEST_RESOURCES NO_XP=1
D=${D:?set D to the directory of the benches}
for sp in cow zombie wolf; do
  a=$(HORIZON_SPECIES=minecraft:$sp "$D/mem_bench" 200 4000 300 2>/dev/null | sed -n 's/^RAW n=4000 bytes=//p')
  out=$(HORIZON_SPECIES=minecraft:$sp "$D/mem_bench" 200 8000 300 2>/dev/null)
  b=$(echo "$out" | sed -n 's/^RAW n=8000 bytes=//p')
  inline=$(echo "$out" | sed -n 's/.*inline component bytes .* = \([0-9]*\) per entity/\1/p')
  rows=$(echo "$out" | sed -n 's/^tables with capacity >= 100: entities \([0-9]*\), capacity \([0-9]*\) rows.*/\1 \2/p')
  echo "bytes per entity $sp: slope $(( (b - a) / 4000 )) (at 8 000: $(( b / 8000 )); components in tables: $inline; rows allocated for entities: $rows)"
done
