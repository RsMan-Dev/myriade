# Cahier : back_up_if_too_close (piglin à l'arbalète)
Racine : crates/horizon-ecs-proto. NE COMPILE PAS, ne lance aucun test.
Fichiers :
- NOUVEAU src/behaviours/back_up_if_too_close.rs (< 90 lignes).
- src/behaviours/mod.rs : `pub mod back_up_if_too_close;` (près de `pub mod spear_use;`) et `&back_up_if_too_close::MODULE,` dans la liste des modules (près de `&spear_use::MODULE,`).
- src/species/piglin.rs : après l'entrée `ranged_bow`, `Entry::new(&back_up_if_too_close::MODULE, 3, &back_up_if_too_close::Params::DEFAULT).when(&HOLDS_CROSSBOW),` + import.
- src/species/mod.rs : dans la ligne RESOLVED de minecraft:piglin, `("back_up_if_too_close", 3)` juste après `("ranged_bow", 3)`.
- tests/it/piglin_vs_mob.rs : un test différentiel (voir Tests).
Modèle :
- Mob : crates/myriade-engine/src/mobs/piglin.rs:1287-1327 (`BackUpIfTooClose`) : drapeaux 0, can_use = can_continue = trop près, tick chaque tick : regarder l'oeil de la cible, reculer en strafe -0.75, orienter le corps.
- Forme du module : src/behaviours/look_at_held.rs (Params/DEFAULT/Component/MODULE/equip/can_use) ; module à tick chaque tick : copier la façon dont src/behaviours/ranged_bow.rs applique la marche (`Drive`, avant/strafe), la visée (`goals::look_at`) et lit la cible (`Target`, `targeting::info`).
Interfaces :
Params { range: f64 (5.0), strafe: f32 (-0.75) } avec `DEFAULT`. Module `back_up_if_too_close`, flags 0, requires Body, Goals, Head, Locomotion, Targets. Trop près : navigation terminée, cible visible (même test de vue que ranged_bow) à moins de `range` blocs. Pas de Vec par entité ; pas de tick qui tourne hors des entités actives (utilise le mécanisme `every tick` de ranged_bow, pas une requête globale si ranged_bow n'en utilise pas).
Tests : aucune commande à lancer (ne compile pas, ne lance pas cargo). Tests à ÉCRIRE :
Dans piglin_vs_mob.rs, sur le modèle de `a_piglin_shoots_its_crossbow_as_a_mob_does` : piglin à l'arbalète, joueur immobile à 3 blocs ; sur 40 ticks, la distance joueur-piglin augmente des deux côtés (Mob et ECS) et le piglin regarde le joueur ; comparer la distance finale à 0,5 bloc près.
Fin :
Les fichiers ci-dessus écrits, aucun nom d'espèce dans le module. Rapport : fichiers touchés, doutes.
Interdits :
Compiler, lancer cargo, toucher au Mob (myriade-engine), à d'autres lignes de species/mod.rs, créer d'autres fichiers.
