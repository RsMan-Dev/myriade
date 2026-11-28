# Cahier pour `codeur` : écarts des bancs du lot 9 (armadillo, allay, tortue)

Contexte : mesure 85 (`suivi/fidelite.md`), bancs `snd` à 5 créatures, 2 passes par côté, 3000 ticks. Jar : `runtime/vanilla-26.3/server-26.3.jar`. Chemins relatifs à `crates/horizon-ecs-proto/src/`. Quatre lots indépendants, fichiers disjoints.

## Lot 1 : armadillo, `state` en NBT et son de blessure réduite
Écart : avec `{state:"scared"}` ou `{state:"rolling"}` vanilla joue 0 ambiance et `entity.armadillo.hurt_reduced` (2 sur 2 passes) ; chez nous 190 et 192 `entity.armadillo.ambient` et `entity.armadillo.hurt`.
Cause au jar : `Armadillo.addAdditionalSaveData` écrit `state` (codec `ArmadilloState`) et `scute_time` ; `readAdditionalSaveData` relit `state` ; `isScared` = état différent de `IDLE` (donc `ROLLING`, `SCARED`, `UNROLLING`) ; `getHurtSound` : `isScared` donne `ARMADILLO_HURT_REDUCED`, sinon `ARMADILLO_HURT`. Chez nous `nbt_fields/creatures.rs:892` ne porte que `scute_time` : `state` n'est ni lu ni écrit.
Fichiers : `nbt_fields/creatures.rs` (champ `state`, valeurs `idle`, `rolling`, `scared`, `unrolling`, d'après `behaviours/armadillo_state.rs:5-17`) ; `species/armadillo.rs` (son de blessure `hurt_reduced` si l'état n'est pas `IDLE`, par le mécanisme des sons déclarés par espèce ou `sound_slots.rs:44-49`) ; `species/armadillo.rs` ambiance : `isScared` = état différent de `IDLE`, vérifier le `choose` du lot A (il compare à `SCARED` seul ?).
Test : `SP=armadillo BAB=',NoAI:1b,state:"scared"' BOXES=snd IDLE=3000` puis `state:"rolling"` : 0 ambiance et `hurt_reduced` 2 des deux côtés.

## Lot 2 : allay, main chargée par NBT
Écart : `equipment:{mainhand:{id:"minecraft:apple",count:1}}` : vanilla 251 `entity.allay.ambient_with_item`, chez nous 252 `ambient_without_item`. (`HandItems` n'est plus lu par vanilla 26.3 : 0 des deux côtés, sans écart.)
Cause : `nbt_fields/living_state.rs:198-225` (`read_equipment`) remplit `Equipment` mais pas `HandStack`, que `goals.rs:428` (`main_hand`) lit seul ; `HandStack` n'est écrit que par `set_main_hand` (ramassage au sol, renard).
Fichiers : `species/allay.rs` (`allay_ambient`, ligne 15) ou `goals.rs:428` ; choisir le plus petit : lire aussi l'emplacement principal de `Equipment` (`crate::equipment`), sans cache.
Test : `SP=allay BAB=',NoAI:1b,equipment:{mainhand:{id:"minecraft:apple",count:1}}' BOXES=snd` : `ambient_with_item` 251 environ.

## Lot 3 : tortue bébé, sons de blessure et de mort
Écart : bébé blessé et tué : vanilla `entity.turtle.hurt_baby` et `entity.turtle.death_baby` (2 et 2), chez nous `entity.turtle.hurt` et `death`.
Cause au jar : `Turtle.getHurtSound` et `getDeathSound` : `isBaby()` donne `TURTLE_HURT_BABY` / `TURTLE_DEATH_BABY`.
Fichiers : `entity_voice.rs` (`baby_voice`, comme le lot 1 du cahier `ecarts-sons-lot8.md` pour le chat : renommer `entity.turtle.hurt` et `entity.turtle.death` d'un bébé ; `shamble_baby` existe déjà).
Test : `SP=turtle BAB=',Age:-24000' BOXES=snd` : `hurt_baby` 2 et `death_baby` 2.

## Lot 4 : tortue, trop de sons de pas (cause à trouver)
Écart : `entity.turtle.shamble` adulte 1614 chez nous contre 1062 (+52 %) ; bébé `shamble_baby` 1074 contre 768 (+40 %). La nage `entity.turtle.swim` est présente : 8267 contre 8545 (conforme). Ambiance 127 contre 121 (conforme).
Cause au jar relevée : `Turtle.nextStep` = `moveDist + 0,15` (un pas toutes les 0,15 unité), `playStepSound` à 0,15 de volume. Cause chez nous non relevée : soit la cadence du pas (`StepCarry`, loi `Entity.applyMovementEmissionAndPlaySound`) n'applique pas `nextStep` = 0,15 à la tortue, soit les tortues marchent plus loin (buts `stroll`, `turtle_go_to_water`, `turtle_go_home`).
À faire : mesurer au banc la distance parcourue par tortue des deux côtés (positions toutes les 20 ticks) avant de toucher au code ; si la distance est égale, corriger la cadence ; sinon le but fautif.
Fichiers : à décider après la mesure (`entity_voice.rs`, `species/turtle.rs`, buts de la tortue).
Test : `SP=turtle BAB='' BOXES=snd IDLE=3000` : `shamble` environ 530 par passe.

Fin : chaque écart vert au banc ; compilation sans erreur ni avertissement nouveau ; rapport de 10 lignes au plus.

Interdits :
- toucher un autre fichier que ceux de son lot (besoin : `LEVER :`), en particulier `species/mod.rs`, `myriade-engine` ;
- ajouter une dépendance ; laisser un dossier `target` dans le worktree ; lancer les tests unitaires `it` de `horizon-ecs-proto` ;
- nom d'espèce codé en dur dans un système ; cache par mob entre ticks ; plus de 10 % de commentaires ; plus de 300 lignes ajoutées d'un coup.
