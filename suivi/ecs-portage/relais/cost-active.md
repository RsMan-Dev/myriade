# ecs/cost-active
Batch 1: ECS path_type_static/adjacent_reset in spots/long_jump/ram/cow; Online::for_near sparse bins; IFloor (no libm floor); of_box clamped columns.
Per creature-tick before -> after: sniffer 5422->4306, creaking 10278->9904, frog 60108->51335, cow 92, villager 91, zoglin 131->130.
Open: ghast occupancy::collect/get_or_build, push closure, reindex, silverfish pathfind, wither apply_deferred.

Batch 2 (callgrind per creature-tick, like for like; base binary -> batch 2): ghast 5093->3188, silverfish 3199->2291, sniffer 5914->3389 (after batch 1).
sc.sh: frog 60108->42356, ghast 4440->4051 (batch1->2), sniffer 4285, creaking 9878.
Batch 2 = frog long jump: Occupancy::no_collision in Finder (with_occ) and jump_arc Footprint.occ; float_around::watch one kept-query pass; push reads is_object(e) lazily.
Tests: goat/frog/ghast/silverfish/sniffer species runs green (389 each).
Open: frog Chunk::get 15% (Finder::fluid/state, 8.2M reads, ~220 instr each: per-section cache), dig_position, silverfish Finder (Horizon), arm()/Clock::resource 172 (arm called ~1 per ghast per tick), apply_deferred wither, look_at_target_sink, alloc/archetype churn.
NOTE: scratchpad cg.sh was replaced mid-session by a file targeting ecs-port-dragon/.tgt-dragon; the first test run of batch 1 ran on the dragon tree, rerun on cgm.sh (cerveaux) is green.
