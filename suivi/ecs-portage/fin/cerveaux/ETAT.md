# Lot cerveaux : état (2026-11-20, [non compilé] sur ordre du propriétaire)
Branche ecs/fin-cerveaux. Aucune compilation depuis l'ordre ; dossier cible supprimé.

## Fait et mesuré (avant l'ordre)
- Test hoglin `hoglins_hunt_a_still_player_as_a_mob_does` : cause = variance de graine de la statistique de foule (ECS 41,0 à 50,2 sur 7 graines ; Mob déterministe) ; cadence par hoglin 41 des deux côtés ; test poolé sur 7 graines, seuils intacts, vert. Oubli d'une cible inatteignable : conforme au jar (StopAttackingIfTargetInvalid 200), identique avec ou sans.
- Test lib `living_near_equals_sorted_candidates` avec frog : rouge aussi sur la base (fichiers identiques) ; cause : la grenouille n'a pas de FollowRange ; le test en donne un à chaque créature ; vert.
- Passes `piglin`, `hoglin`, `zombified_piglin` avant les derniers ajouts : tout vert sauf swim_controls_vs_mob::the_fluid_height_is_the_mobs_at_the_same_positions avec hoglin et piglin (le Mob garde water=1.3889 constant à y 3.55 à 4.05 : sa hauteur de fluide n'est pas recalculée pour ces espèces ; hors lot, Mob intouchable : à trancher). zombified_piglin : 508 verts.

## Écrit non compilé
- species/mod.rs : sniffer, creaking, breeze remis dans BUILTIN + lignes RESOLVED (float 0 ; creaking float 0 + stare_freeze 0) ; faute `] ),` de zombified_piglin corrigée ; ligne piglin : interact_with_door 0, spear_use 3.
- piglin.rs : InteractWithDoor (prio 0, params `&()`), PiglinSpear = `spear_use::PIGLIN` prio 3 (priorité lue dans le Mob piglin.rs:1346, jar non relu).
- brain/behaviours/walk_away.rs : mémoire Block (centre bas du bloc) ; hoglin.rs : entrée idle HoglinAvoidRepellent (vitesse 1.0, désir 8, NEAREST_REPELLENT).

## Cahiers à lancer (Haiku) : cahier-back-up.md, cahier-admire-seen.md, cahier-tests-ajouts.md
## Alias (à ajouter à la table avancement.py)
- LookAtLoved -> look_at_held ; GoToWantedItem -> go_to_wanted ; HuntHoglin -> hunt_prey ; PiglinAvoidRepellent -> flee_repellent ; HoglinAvoidRepellent -> walk_away (brain, mémoire Block) ; piglin InteractWithDoor -> interact_with_door ; PiglinSpear -> spear_use.
## À mesurer après fusion
- Coût au repos de sniffer, creaking, breeze (9 847 et 12 484 relevés avant `brain-sleep`) : sans dérogation ; rapporter les chiffres.
- LookAtLoved ECS : le regard se pose au démarrage seulement (le Mob l'entretient chaque tick) : à vérifier.
