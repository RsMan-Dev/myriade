# Cahier pour `codeur` : le lapin (`minecraft:rabbit`) comme vanilla 26.3

Contexte : la grille `suivi/fidelite.md` (mesure 68) donne le lapin rouge sur la sauvegarde, l'eau et la lave.
Faits relevés au jar (`runtime/vanilla-26.3/server-26.3.jar`, `javap -p -c -cp <jar> net.minecraft.world.entity.animal.rabbit.Rabbit`, ses classes internes `Rabbit$RabbitMoveControl` et `Rabbit$RabbitJumpControl`, puis `LivingEntity.aiStep` et `Mob.serverAiStep`).

Fichiers :
- crates/horizon-ecs-proto/src/species/rabbit.rs
- crates/horizon-ecs-proto/src/nbt_fields/creatures_more.rs (modèle d'un champ NBT)
- crates/horizon-ecs-proto/src/behaviours/hop.rs (locomotion `Hop`, `Params::RABBIT`)

Modèle :
- Champ NBT à propriétaire : `nbt_fields/creatures_more.rs:29-31` (`Sheared`, `Pumpkin`) ; le composant propriétaire est créé par la règle de l'espèce.
- Locomotion de saut : `behaviours/hop.rs` entier, déclarée par `species/rabbit.rs` ligne `locomotion: &[(Medium::Ground, ...)]`.

Interfaces (mesuré, puis cause à lire au jar) :
1. Sauvegarde : vanilla écrit `MoreCarrotTicks` (Int, 0 pour un lapin neuf) dans `addAdditionalSaveData` ; Myriade n'écrit rien. Lire `readAdditionalSaveData` : la clé est relue et alimente le délai des carottes (`DEFAULT_MORE_CARROT_TICKS`).
2. Sauvegarde : `RabbitType` est absent du `/data get entity` d'un lapin invoqué avec des étiquettes NBT (`summon ... {NoAI:1b,...}`) et n'apparaît qu'après rechargement ; vanilla l'a dès l'invocation. Cause à trouver : le tirage de variante (`species/rabbit.rs` VARIANT, `Pick::Rows`) n'est pas fait quand le `summon` porte un NBT ; au jar `summon` appelle `finalizeSpawn` seulement sans NBT autre que l'identifiant, donc le type vient de `Rabbit` (constructeur/valeur par défaut 0). Aligner sur ce que l'entité vanilla a dans ce cas.
3. Attributs : vanilla porte `attack_damage` 3,0 (`Rabbit.createAttributes`, instruction 21) et l'écrit dans `attributes` ; Myriade seulement `movement_speed`. Ajouter la valeur de base à la table d'attributs de l'espèce.
4. Eau : un lapin invoqué dans 3 blocs d'eau émet 23 sons (saut, ambiance, éclaboussure) en 100 ticks chez vanilla contre 1 chez Myriade. Lire `Rabbit.customServerAiStep` (appelle `setJumping`, `checkLandingDelay`) et la branche `jumping` de `LivingEntity.aiStep` (saut en fluide) : le lapin vanilla continue de sauter dans l'eau, `Medium::Ground` seul de Myriade ne le fait pas.
5. Lave : 79 sons `entity.rabbit.jump` et 20 `entity.rabbit.ambient` en 100 ticks chez vanilla, 0 et 0 chez Myriade (4 sons en tout, dont 3 `generic.burn` contre 1) : le lapin vanilla survit et saute dans la lave plus longtemps ; mesurer la vie après 100 ticks des deux côtés (`data get entity @e[tag=l,limit=1] Health` du banc) avant de corriger.

Tests (banc du scratchpad, `s_fid.py`, aucun test unitaire `it`) :
- `SP=rabbit BOXES=env,savea,saveb` côté Myriade et côté vanilla, analyseurs `fsnd.py rabbit <binaire>` et `fsave.py rabbit <binaire>` : attendu, plus aucune ligne `MoreCarrotTicks`, `RabbitType`, `attributes` autre que l'ordre des éléments ; `env-water` : 20 à 25 sons des deux côtés ; `env-lava` : même ordre de grandeur de sons de saut.

Fin : arrêter dès que `fsave.py rabbit` n'affiche plus de ligne `ours-vs-van` hors `Tags`, rapport de 3 lignes.

Interdits : ne toucher que les fichiers listés (`LEVER :` pour tout autre) ; aucune dépendance ; aucun dossier `target` dans le worktree ; aucun test `it` ; aucun nom d'espèce codé en dur dans un système ; aucune mémoire par entité entre ticks ; 10 % de commentaires au plus ; au plus 300 lignes d'un coup.
