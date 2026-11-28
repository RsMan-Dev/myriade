# Écarts du lot 11 (mesure 89)

Binaire a12167fc2e, 2 passes par côté. Chaque lot touche des fichiers disjoints de `crates/horizon-ecs-proto/src/`. Faits : « jar » = vérifié par javap/liste du jar `runtime/vanilla-26.3/server-26.3.jar` ; « banc » = mesuré ; « À VÉRIFIER » = pas encore lu au jar, le codeur ou le relecteur le lit avant d'écrire.

## Lot A : villageois (`species/villager.rs` seul)
Fichiers : `species/villager.rs`.
Modèle : (1) jar : `Villager` écrit les clés `Gossips`, `LastGossipDecay`, `VillagerData`, `FoodLevel`, `LastRestock`, `RestocksToday`, `Xp`, `VillagerDataFinalized` ; banc : nous n'écrivons pas `Gossips` (liste vide `[]`), `Inventory` (`[]`), `LastGossipDecay` (long, tick de jeu), `VillagerDataFinalized`. (2) banc : marche à 600 ticks 8,9 et 9,4 contre 4,2 et 3,6, `block.stone.step` 63 et 77 contre 41 et 36 : cause À VÉRIFIER (`RandomStrollGoal` villageois, intervalle ou vitesse du cerveau `VillagerGoalPackages`). (3) banc : `entity.generic.splash` 3 chez nous contre 0 (À VÉRIFIER).
Interfaces : table `nbt_fields/*` existante ; ne pas créer de nouveau module.
Tests : relancer le banc `spawn,move,snd,savea,saveb` du villageois, 2 passes ; critère : plus aucune clé « only vanilla », distance à 600 ticks dans l'intervalle vanilla ±30 %.
Fin : rapport avec le chiffre de marche et l'espace libre.
Interdits : tout autre fichier d'espèce ; `nbt_fields/common.rs` (lot B) ; pas de compilation à froid.

## Lot B : noyé, husk (`species/drowned.rs`, `species/husk.rs`, `species/zombie.rs`)
Fichiers : ces trois seulement.
Modèle : banc : `PersistenceRequired` à 1b chez nous sur 11 à 12 créatures sur 14, 0b chez vanilla (nous le posons à la création, cause À VÉRIFIER : sans doute le ramassage ou l'équipement par défaut) ; clé `mainhand` en trop chez nous (vanilla n'écrit pas de `equipment` vide). Jar : table de butin du noyé contient `copper_ingot` (vanilla 2 sur 14 aux deux passes, nous 0) : ajouter l'entrée avec sa condition lue dans `data/minecraft/loot_table/entities/drowned.json` (chance et `killed_by_player` À RELIRE avant d'écrire).
Interfaces : règles de butin existantes de `drowned.rs`.
Tests : banc `spawn,death` du noyé et du husk ; critère : `PersistenceRequired` 0b partout, lingot de cuivre vu au moins une fois en 28 morts.
Fin : rapport court + espace libre.
Interdits : `nbt_fields/common.rs`, `species/slime.rs`.

## Lot C : slime (`species/slime.rs` seul)
Modèle : banc : attribut `attack_damage` base 1.0 absent chez nous (vanilla le porte à 14 sur 14) ; clé `wasOnGround` absente ; `slime_ball` 5 et 6 contre 1 sur 14 (nous dépassons : la table du jar impose `cube_mob size 1` et un compte uniforme ; lire `entities/slime.json` en entier, À VÉRIFIER) ; `entity.generic.splash` 0 contre 3 (À VÉRIFIER : son d'éclaboussure dans l'eau).
Tests : banc `spawn,env,death,savea,saveb` du slime. Critère : attributs identiques, clé `wasOnGround`, `slime_ball` ≤ 1 par mort de taille 1.
Interdits : autres espèces ; `nbt_fields/common.rs`.

## Lot D : pillard (`species/pillager.rs` seul)
Modèle : jar : le constructeur ou `finalizeSpawn` du `Pillager` appelle `setCanPickUpLoot(true)` ; banc : nous écrivons `CanPickUpLoot` 0b contre 1b, et pas la clé `Inventory` (vanilla l'écrit, `Pillager` a un `SimpleContainer` de 5 cases : À VÉRIFIER). Banc : `block.stone.step` 41 et 58 contre 20 et 19 : déplacement trop fréquent (À VÉRIFIER, but d'approche ou de patrouille).
Tests : banc `spawn,move,snd,savea,saveb`. Critère : `CanPickUpLoot` 1b, `Inventory` présent, pas de marche en trop.
Interdits : `species/witch.rs`, `nbt_fields/common.rs`.

## Lot E : sorcière (`species/witch.rs`, `behaviours/drink_potion.rs`)
Modèle : banc : `entity.witch.drink` 0 contre 3 aux deux passes (vanilla boit ; nous jamais) ; jar : `Witch.aiStep` joue le son à chaque boisson et pose le modificateur `SPEED_MODIFIER_DRINKING` (champ du jar) avec l'événement `GameEvent.DRINK`. Lire l'ordre exact dans `aiStep` avant d'écrire.
Tests : banc `snd,hurt` ; critère : au moins un `drink` par passe quand la sorcière est blessée.
Interdits : `species/pillager.rs`.

## Lot F : enderman (`species/enderman.rs`, champ commun `nbt_fields/common.rs` : `anger_end_time`)
Modèle : banc : `anger_end_time` chez nous 0L, chez vanilla -1L pour l'enderman (le défaut commun est 0L : `nbt_fields/common.rs:28` et `:150`) ; il faut une valeur par espèce, À VÉRIFIER dans `EnderMan` du jar (`persistentAngerEndTime`). Attention : le fichier commun touche toutes les espèces, donc ce lot passe seul, après les autres.
Tests : banc `savea,saveb` de l'enderman et d'un loup (le champ y est utilisé).
Interdits : tout fichier d'espèce autre que `enderman.rs`.

## Non rouge, non tranché
`inter` du villageois et du pillard : un champ `cow` n'apparaît que de notre côté, probablement la boîte de banc ; à vérifier dans `s_fid.py` avant d'en faire un lot. Marche à 600 ticks du husk, slime, pillard et sons d'enderman (`entity.enderman.hurt` 4 et 8 contre 1) : variance, à rejouer avec 3 passes.

## Mesure 90 (binaire du commit 5a1b036957, 3 passes)
Restent rouges, avec la cause relevée :
- Lot B (noyé, husk, et tout zombie sans tirage) : cause trouvée. Fichiers : `behaviours/pick_up_gear.rs` (fonction `look`, ligne ~108) et/ou `nbt_fields/common.rs` (`load_pick_up`, ligne ~51). Modèle : au jar `Mob.aiStep` exige `canPickUpLoot()` avant de chercher des objets ; `look` ne regarde que `spawn_gear::NoLoot`, que seul le tirage de naissance pose ; un `/summon` avec NBT n'a pas de tirage, `load_pick_up` met `Pickup.picks_up` à faux (un composant que `Gear` ne lit pas) et le zombie ramasse la chair pourrie du cadavre précédent (`equipment.mainhand` = `rotten_flesh`, `CanPickUpLoot:0b` relevé), puis `item::pick_up` pose `despawn::keep` (`PersistenceRequired` 1b). Correctif : que `look` refuse quand le drapeau de ramassage est faux (lire `Pickup.picks_up` ou poser `NoLoot` dans `load_pick_up`). Test : banc `spawn` du husk et du noyé, `PersistenceRequired` 0b et pas de `mainhand`. Le `copper_ingot` du noyé n'est plus un écart (9 sur 80 contre 10 sur 80).
- Lot A (villageois, `species/villager.rs` ou son champ de sauvegarde) : `LastGossipDecay` doit valoir le temps de jeu du premier appel (au jar : `lastGossipDecayTime` à 0, puis `Level.getGameTime()`), nous écrivons 18 à 22L.
- Lot F (enderman, `behaviours/ender_teleport.rs` et lave) : dans la lave, vanilla téléporte 9 fois sur 10 au premier coup (`Enderman.hurtServer`), nous gardons 18 sons de lave contre 2 aux passes 2 et 3. Cause non cherchée (le banc est une boîte fermée de verre : à voir si `repeatedly_try_to_teleport` y échoue).
- Non tranchés (variance) : marche de l'enderman et du husk (ours plus bas), `block.stone.step` du villageois, du pillard, de la sorcière et `drowned.step` (1,5 à 2 fois chez nous dans certaines passes).
