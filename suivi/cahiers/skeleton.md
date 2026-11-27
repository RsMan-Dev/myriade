# Cahier pour `codeur` : le squelette (`minecraft:skeleton`) comme vanilla 26.3

Contexte : la grille `suivi/fidelite.md` (mesure 68) donne le squelette rouge sur la sauvegarde. Classes au jar : `net.minecraft.world.entity.monster.skeleton.Skeleton`, `AbstractSkeleton`, `net.minecraft.world.entity.ConversionTracker`.

Fichiers :
- crates/horizon-ecs-proto/src/species/skeleton.rs
- crates/horizon-ecs-proto/src/behaviours/convert_in_water.rs (modèle d'un compteur de conversion)
- crates/horizon-ecs-proto/src/nbt_fields/common.rs (clés de conversion)

Modèle :
- Conversion par compteur : `behaviours/convert_in_water.rs` entier (`Soaking`) ; clés `nbt_fields/common.rs:38`, `:109` (`DrownedConversionTime`, défaut -1) et `:257-265` (écriture).

Interfaces :
1. `Skeleton.addAdditionalSaveData` (instructions 0-10) écrit `ConversionTracker.addAdditionalSaveData` : les clés `FreezingTime` (Int, -1 hors conversion) et `StrayConversionTime` (Int, -1 hors conversion) ; `readAdditionalSaveData` les relit. Myriade n'écrit ni l'une ni l'autre. Lire au jar (`javap -p -c` de `Skeleton`, `AbstractSkeleton`, `ConversionTracker`) le déclencheur, la durée et la conversion vers le `stray` (non relevés ici). Ajouter, comme `convert_in_water`, un compteur de gel pour le squelette, l'écrire à -1 par défaut et le relire.
2. Mêmes clés à vérifier sur le `stray`, qui n'a pas de fichier d'espèce (`species/skeleton.rs:4` : « leurs fichiers n'existent pas ») : hors de ce lot.
3. Autres cases du squelette vertes à N = 10 à 24 : déplacement 14,4 contre 11,4 blocs à 600 ticks (non significatif), PV, expérience 5, butin os et flèches, sons de blessure et de mort.

Tests (banc du scratchpad, pas de test unitaire `it`) :
- `SP=skeleton DIFF=normal TIME=18000 BOXES=savea,saveb` des deux côtés puis `fsave.py skeleton <binaire>` : attendu, plus aucune ligne `FreezingTime` ni `StrayConversionTime`.
- Vérification de la conversion : un squelette posé dans de la neige poudreuse devient un stray après la même durée des deux côtés (durée lue au jar) (`/data get entity`, type).

Fin : arrêter dès que `fsave.py skeleton` est vide hors `Tags`, rapport de 3 lignes.

Interdits : ne toucher que les fichiers listés (`LEVER :` pour tout autre) ; aucune dépendance ; aucun dossier `target` dans le worktree ; aucun test `it` ; aucun nom d'espèce codé en dur dans un système ; aucune mémoire par entité entre ticks ; 10 % de commentaires au plus ; au plus 300 lignes d'un coup.
