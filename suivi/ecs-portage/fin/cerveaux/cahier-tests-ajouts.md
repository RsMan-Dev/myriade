# Cahier : tests différentiels du répulsif de hoglin et de la porte du piglin
Racine : crates/horizon-ecs-proto/tests/it. NE COMPILE PAS, ne lance aucun test. Seuls fichiers touchés : hoglin_vs_mob.rs et piglin_vs_mob.rs (ajouts en fin de fichier, aucune ligne existante modifiée).
Fichiers : crates/horizon-ecs-proto/tests/it/hoglin_vs_mob.rs et piglin_vs_mob.rs (ajouts en fin de fichier).
Modèle : les tests cités ci-dessous (même fichier).
Interfaces : deux fonctions `#[test]` aux noms donnés, helpers existants seulement.
Tests : aucune commande à lancer (ne compile pas, ne lance pas cargo). Tests à ÉCRIRE :
1. hoglin_vs_mob.rs : `a_hoglin_walks_away_from_a_repellent_as_a_mob_does`. Modèle : le test `a_repellent_pacifies_a_hoglin_that_is_already_attacking_as_a_mob_does` (même fichier : pose du bloc répulsif, deux côtés, helper `ecs_of`). Ici aucun joueur : un hoglin adulte au repos à 5 blocs d'un bloc de champignon tordu (`minecraft:warped_fungus`), laisser 100 ticks ; des deux côtés la distance au bloc finale est > 8 (le hoglin s'éloigne : HoglinAvoidRepellent, `crates/horizon-client/src/mobs/piglin.rs:1592`, vitesse 1.0, désir 8). Comparer l'écart de distance finale à 3 blocs près (les tirages diffèrent).
2. piglin_vs_mob.rs : `a_piglin_opens_the_door_on_its_path_as_a_mob_does`. Modèle : un test de villageois/porte existant (cherche `door` dans tests/it/*.rs, p. ex. interact_with_door ou door_vs_mob) : porte en bois fermée entre le piglin et un point imposé à 6 blocs ; après le trajet, la porte est ouverte puis refermée des deux côtés (comparer l'état de la porte aux mêmes ticks, à 5 ticks près).
Fin :
Deux tests écrits avec les helpers déjà présents. Rapport : doutes.
Interdits :
Compiler, cargo, toucher au Mob, ajouter des helpers dans common/mod.rs.
