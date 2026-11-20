# Lot effets (2026-11-20) : état
Écrit, NON compilé après la consigne du propriétaire (un état vert partiel avait été mesuré : les 4 tests `thrown_ends_vs_original` verts avant l'enclume).
- `OnHit::{Hatch,Teleport,Orbs}` (projectile.rs), `projectile_more::ends` + `ThrownRng`, `egg_hit` (config `species/lists.rs` : chicken), `HostEvent::Pearl`. Mesuré vert : 4 tests thrown_ends.
- Enclume : `falling_block.rs` (`fell`, `anvil_hurts`), `Cause::FallingBlock` (cow/health.rs), test `an_anvil_hurts_the_creature_under_it_and_wears_as_the_jar_says` : jamais compilé ni lancé.
- Reste : compiler, lancer `thrown_ends`, `falling_block`, `no_species_names`; vérifier les `match` sur `Cause`; fusion avec l'autre agent de `falling_block.rs`.
- Écarts : `suivi/ecs-ecarts.md`, section « Effets à l'impact ».
