# couts (branche ecs/fix-couts)
Mesures `REST_WAKES` (tests/it/rest_wakes.rs, 1000 ticks au repos, hors gate) :
- sorciere : 0 (avant : un tirage par tick dans drink_potion::tick). Marqueur Thirsty + hooks (Target/Burning/InFluid), event damaged.
- piglin : 8 (avant : flee_repellent, nearest_target joueurs en scrutation ; analytique, non mesure avant). BlockWatch + Parked, PlayerWatch.
- wither : 175 -> 119. regen dort a pleine sante (reveil par damaged, meme phase). Reste : tetes laterales (volees oisives, observables).
  Test wither en #[ignore] avec raison ; entree `wither` gardee dans species_cost.allow.
- enderman, fox, cave_spider : non traites.
Ecart a consigner (suivi/ecs-ecarts.md) : sorciere, ordre de tirage sur son propre flux (tirages sans effet supprimes). Loi du jar (Witch.aiStep) non verifiee.
