# Cahier pour `codeur` : le renard (`minecraft:fox`) comme vanilla 26.3

Contexte : la grille `suivi/fidelite.md` (mesure 68) donne le renard rouge sur la sauvegarde. Classes au jar : `net.minecraft.world.entity.animal.fox.Fox`, `Fox$DefendTrustedTargetGoal`.

Fichiers :
- crates/horizon-ecs-proto/src/species/fox.rs
- crates/horizon-ecs-proto/src/nbt_fields/creatures_more.rs
- crates/horizon-ecs-proto/src/behaviours/fox_state.rs

Modèle :
- Champ NBT lié à `FoxFlags` : `nbt_fields/creatures_more.rs:49-62` (clé `Sleeping`).
- Etat du renard : `behaviours/fox_state.rs`.

Interfaces :
1. `Trusted` : `Fox.addAdditionalSaveData` écrit toujours la clé `Trusted` (liste, vide pour un renard neuf), puis `Sleeping`, `Type`, `Sitting`, `Crouching` ; `readAdditionalSaveData` appelle `clearTrusted` puis relit la liste. Myriade n'écrit pas `Trusted` : la confiance envers des joueurs (`addTrustedEntity`, `getTrustedEntities`, but `DefendTrustedTargetGoal`) n'a pas de composant. Créer un composant de renard portant la liste d'entités de confiance (références d'entité, comme `Owner`), l'écrire (liste vide comprise) et la relire ; le but de défense des fiables existe-t-il dans `species/fox.rs` : à vérifier avant d'ajouter.
2. `CanPickUpLoot` : après `/summon minecraft:fox ... {NoAI:1b,PersistenceRequired:1b,...}` vanilla donne `0b` avant et après rechargement, Myriade `1b`. Au jar `summon` ne passe par `finalizeSpawn` (qui pose `setCanPickUpLoot` pour le renard) que sans NBT ajouté. Faire de même : poser le drapeau à `finalizeSpawn`, pas à la création de l'entité.
3. Rien d'autre n'a divergé : `Sleeping`, `Type`, `Sitting`, `Crouching` identiques ; déplacement N = 10 : 14,2 contre 12,7 blocs à 600 ticks (non significatif à N = 10), sons de chute/eau : écarts d'un son isolé non concluants.

Tests (banc du scratchpad, pas de test unitaire `it`) :
- `SP=fox BOXES=savea,saveb` des deux côtés puis `fsave.py fox <binaire>` : attendu, plus aucune ligne `Trusted` ni `CanPickUpLoot` (hors `Tags` et ordre de `attributes`).

Fin : arrêter dès que `fsave.py fox` est vide hors `Tags`, rapport de 3 lignes.

Interdits : ne toucher que les fichiers listés (`LEVER :` pour tout autre) ; aucune dépendance ; aucun dossier `target` dans le worktree ; aucun test `it` ; aucun nom d'espèce codé en dur dans un système ; aucune mémoire par entité entre ticks ; 10 % de commentaires au plus ; au plus 300 lignes d'un coup.
