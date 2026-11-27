# Fidélité des espèces contre vanilla 26.3

**Espèces 100 % vertes : 3 sur 6** (mesure 64 : vache, poule, morue). Ce qui empêche chacune : mouton : `apparition naturelle` à remesurer après la mesure 65 (Myriade 26, 26, 30 contre vanilla 21 à 28 sur 3 zones ; avant correction 27 à 31) ; cochon : `apparition naturelle` à trancher (totaux égaux ; vanilla perd au hasard des membres d'un paquet : 4, 4 puis 1 cochons sur le même chunk en 3 exécutions) ; chauve-souris : `suspension spontanée` non tranchée (13 sur 72 contre 21 sur 72, N = 36 de chaque côté, écart non significatif) ; toutes les autres cases vertes. Morue : toutes les cases vertes (mesure 63). Hors espèce : ligne `objet au sol dans l'eau` ouverte. Apparition naturelle des quatre espèces d'élevage : 3 zones d'océan (graine 12345, 300 ticks, rayon 150), 4 exécutions de Myriade contre 5 de vanilla ; la ligne de chaque espèce donne les totaux.
Cahiers : `suivi/cahiers/cod.md`, `suivi/cahiers/bat.md`.

Légende : V vert (identique, ou dans la loi à N répétitions), R rouge (écart prouvé), - sans objet (au jar), ? non mesuré ou non concluant.
Banc : `drv3.py` + `s_fid.py` (serveur Myriade `fid48` contre serveur vanilla, même monde plat, mêmes commandes, bot Bot0 ; analyseurs
`fid.py`, `fsnd.py`, `fidcmp.py`, `fdeath.py`, `fint.py`, `fsave.py`). N = répétitions par case ; déplacement N = 76 par durée (mesure 49 ; 30 avant).
Les scripts de banc vivent dans le scratchpad de la session ; ils ne sont pas versionnés.

Correctifs de cette mesure (tous petits, < 100 lignes chacun) : voix d'ambiance par la loi `Mob.baseTick` (volume 1,0, variantes
de sons vérifiées au jar) ; brûlure en lave (`entity.generic.burn` 0,4) ; clés NBT par défaut (`Invulnerable`, `attributes`
avec `movement_speed`, `Type`/`variant`, `IsBaby` seulement pour les espèces à drapeau bébé, `IsChickenJockey`, `ForcedAge`) ;
UUID posé par `/summon` et `/data get` ; porc, poule, mouton : l'interaction d'un aliment n'est plus avalée par la règle de monture
de l'hôte (porc non sellé, poule porteuse seulement) ; alimentation d'un bébé en secondes entières avec `ForcedAge`.
Mesure 51 : morue (intervalle d'ambiance 120, volume de nage de `playSwimSound`, seau), chauve-souris (vol dès l'apparition, repos, tête, son d'ambiance à 0,95, réveil par bloc retiré et son 1025) ; `/summon painting` paniquait (`variant` déclaré deux fois depuis la mesure 49) : un champ avec propriétaire l'emporte sur le champ général de même clé (`nbt.rs`). Non-régression : g21b 5 contre 5, 0 différence ; g23 5 contre 5, une seule (`vex`, vol aléatoire).
Les tests unitaires de `horizon-ecs-proto` (cible `it`) ne compilent pas à ce HEAD (56 erreurs préexistantes dans
`regles_mecanismes_vivant.rs`, `regles_lot6_a2.rs`, `regles_lot10_creatures.rs`) : aucun test ajouté, la preuve est le banc.

## Vache (N = 43 apparitions, 30 déplacements, 10 dégâts, 24 morts, 24 interactions)
| Case | État | Note |
|---|---|---|
| apparition, NBT complet | V | 43 apparitions, clés et valeurs identiques |
| déplacement libre 100/600 | V | N = 76, mêmes mondes : 2,3/7,3 contre 2,6/7,8 (écarts sous 1 écart-type de la moyenne, signe inversé entre les durées) |
| eau, lave, chute | V | éclaboussures et brûlure dans le bruit |
| sons | V | ambiance 21 contre 20 ; blessure et mort dans la loi de hauteur ; pas de `cow.step` à 0,15 corrigé |
| dégâts reçus, panique | V | PV identiques ; la panique n'est pas analysée à part |
| mort (butin, XP) | V | N = 24, lois identiques |
| interactions du jar | V | blé : InLove 580 contre 579 (un tick de lecture) ; bébé : ForcedAge 2380 identique après correctif |
| croissance | V | même suite d'Age |
| sauvegarde, rechargement | V | NBT identique (UUID conservé après correctif) |
| apparition naturelle | V | mesure 64 : cause corrigée (garde unique `Index.unplaced`, voir Mesure 64) ; 4 exécutions de Myriade contre 5 de vanilla, 3 zones d'océan, 300 ticks, rayon 150, comptes un à un (`data get entity`) ; total des 3 zones : Myriade 32, 31, 32, 31 ; vanilla 29, 30, 30, 31, 30 ; par zone Myriade 7/8/16-17, vanilla 7/6-9/14-17 ; UUID présent (V) |

## Cochon
| Case | État | Note |
|---|---|---|
| apparition | V | 43 |
| déplacement | V | N = 75/76 : 3,3/7,7 contre 2,5/8,1 (signe inversé entre 100 et 600 ticks : pas de dérive) |
| eau, lave, chute | V | |
| sons | V | pas : `StepCarry` garde `moveDist`/`nextStep` entre les arrêts (loi `Entity.applyMovementEmissionAndPlaySound`) : `pig.step` 42 et 75 contre 63 et 58 (deux relevés, avant : 32 contre 55) ; ambiance 24 contre 20 ; bébé : `baby_pig.ambient` 21 contre 20, `baby_pig.step` 75 contre 58, blessure et mort dans la loi de hauteur (base 1,5) |
| dégâts, panique | V | |
| mort | V | dans la loi |
| interactions | V | carotte, patate, betterave : InLove 580 contre 579, ForcedAge identique ; selle d'un adulte V ; monte V (mesure 55 : la monte à main vide d'un cochon sellé prend, 10 sur 10 comme vanilla ; un cochon sellé par commande ou NBT ne monte pas encore, `Mount.saddled` ne suit pas `read_equipment`, `nbt_fields/living_state.rs` 198-220) ; bébé non sellé, identique |
| croissance | V | |
| sauvegarde | V | |
| apparition naturelle | ? | mesure 64 : total des 3 zones 29, 29, 29, 29 contre 28, 28, 30, 31, 26 (égal), mais les zones divergent en sens contraires : zone 1 Myriade 15 contre 15 à 20 (moyenne 17), zone 2 Myriade 8 (quatre fois) contre 5 (quatre fois sur cinq) et 8 une fois ; le total ne dit pas que chaque zone est juste ; cause non cherchée ; UUID présent (V) |

## Mouton
| Case | État | Note |
|---|---|---|
| apparition, eau, mort, sauvegarde | V | |
| déplacement, sons | V | déplacement N = 76 : 2,4/7,9 contre 2,7/7,7 |
| interactions | V | blé comme la vache ; tonte : 1 à 3 laines, N = 24 (10/11/3) contre 15 tontes vanilla non nulles (9/5/1) ; teinture : identique (V) ; herbe broutée adulte 10 sur 24 contre 10 sur 24 (V) ; herbe broutée du bébé V (mesure 55) : bébés 24 sur 24 ayant brouté en 300 ticks contre 23 sur 24 en vanilla (avant : 3 sur 24) ; adulte en 1500 ticks 16 sur 24 contre 13 sur 24 |
| croissance | V | |
| apparition naturelle | R | mesure 64 : total des 3 zones Myriade 30, 27, 30, 31 contre vanilla 24, 21, 27, 27, 25 (4 contre 5 exécutions) ; zone 1 : 13, 9, 13, 13 contre 9, 7, 9, 9, 8 ; zone 2 : 10 contre 6 à 10 ; la zone 0 est égale (7 à 8) ; Myriade en fait plus, constant d'une exécution à l'autre alors que vanilla varie ; cause non établie (piste non vérifiée : vanilla ne montre pas les paquets des chunks encore en dessous du statut final au moment du relevé) ; UUID présent (V) |

## Poule
| Case | État | Note |
|---|---|---|
| apparition | V | `IsChickenJockey` ajouté, 43 apparitions identiques |
| apparition naturelle | V | mesure 64 : total des 3 zones Myriade 16, 16, 16, 16 contre vanilla 16, 16, 19, 16, 16 ; zones 4/4/8 des deux côtés |
| déplacement | V | N = 75/76 : 2,5/8,4 contre 3,0/8,3 |
| eau, lave, chute | V | pas de dégâts de chute |
| sons | V | `chicken.step` 50 et 43 contre 55 et 50 (avant : 35 contre 68) ; ambiance 22 contre 20 ; bébé : `baby_chicken.ambient` 23 contre 19, `baby_chicken.step` 43 contre 50, blessure et mort dans la loi |
| dégâts, mort | V | |
| interactions | V | graines : InLove 580 contre 579 (la règle de monture avalait la graine, corrigé) ; ForcedAge identique |
| croissance | V | |
| sauvegarde | V | |
| ponte | V | EggLayTime identique à l'apparition ; la ponte observée n'est pas comptée |

## Morue (poisson, eau) (N = 43 apparitions, 30 déplacements, 10 dégâts, 24 morts)
| Case | État | Note |
|---|---|---|
| apparition | V | 43, clés identiques |
| déplacement | V | mesure 55 : 100 et 600 ticks 3,6 et 4,8 blocs contre 4,8 et 5,2 (l'ancien 0,1 contre 3,7 venait du but `fish_swim` non armé, corrigé) |
| eau, chute | V | éclaboussure principale de l'entrée dans l'eau 0,16 contre 0,16 en 24/24 (mesure 62 ; avant 0,15) : `checkFallDamage` de vanilla relit l'eau à la fin de `move`, avec la vitesse que le déplacement a utilisée ; Myriade la relit au début du tick suivant, `StepState` garde donc la vitesse de l'avant-dernière ligne (`v2`) ; volume de `playSwimSound` corrigé ; `fish.swim` 0,00 à 0,01 comme vanilla |
| sons de surface de l'éclaboussure | V | mesure 63 : les `entity.generic.splash` de source ambiance (8), volume 0,007 à 0,026, paquet positionnel au niveau de l'eau (y 106,8), viennent d'un `ItemEntity` (le butin de la morue tuée par `/kill` entre deux essais ; `ItemEntity` et `ExperienceOrb` sont les seuls `getSoundSource` AMBIENT du jar) : 5 essais sur 12 chez vanilla, 0 sur 12 chez Myriade. Ce n'est pas la morue : voir `Écarts connus`, objet au sol dans l'eau |
| sons | V | ambiance (intervalle 120) 28 sur 36 des deux côtés ; blessure et mort présentes |
| dégâts, mort | V | PV 2,0 identiques ; XP dans la loi ; un os de farine à 5 % : la table vanilla `cod` le prévoit, pas un écart |
| interactions | V | seau d'eau : `cod_bucket` avec `bucket_entity_data {Health: 3.0f, PersistenceRequired: 1b, NoAI: 1b}` identique |
| croissance | - | pas d'âge |
| sauvegarde | V | `Motion` identique après rechargement (seul l'ordre des étiquettes diffère) |
| apparition naturelle | V | mesure 64, 3 répétitions par zone d'océan (graine 12345, `spawn_mobs` vrai des deux côtés, 1 200 ticks, rayon 128, comptes lus un à un par `data get entity`) : zone 0 Myriade 20, 20, 26 (moyenne 22,0) contre vanilla 21, 22, 24 (22,3) ; zone 1 10, 16, 27 (17,7) contre 11, 17, 17 (15,0) ; zone 2 10, 5, 7 (7,3) contre 12, 16, 7 (11,7) ; l'écart entre répétitions d'un même côté (10 à 27 en zone 1) dépasse celui entre les côtés : l'écart de +40 % de la mesure 63 était du bruit, pas de différence décelable (zone 2 plutôt plus bas chez nous) ; taille des groupes non relue |

## Chauve-souris (N = 30 déplacements, 10 dégâts, 24 morts)
| Case | État | Note |
|---|---|---|
| apparition | V | `BatFlags` 0 à l'apparition, comme vanilla (43 apparitions) |
| déplacement | V | 100 ticks : 5,6 contre 5,4 ; 600 ticks : 8,7 contre 8,7 ; au sol 0 à 1 sur 30 contre 0 |
| sons | V | ambiance 23 contre 22, volume 0,1, hauteur x0,95 au repos ; blessure et mort présentes (hauteurs dans la loi) |
| dégâts, mort | V | PV 5,0, XP 0 identiques |
| sauvegarde | V | `Motion` et `Rotation` identiques après rechargement (étiquettes dans un autre ordre seulement) |
| réveil par bloc retiré | V | toit retiré : 5 suspendues avant, 0 après (vanilla 8 avant, 0 après) ; l'événement 1025 est émis, non relevé au paquet |
| suspension spontanée | ? | non tranché (mesure 63) : 1 500 ticks, 18 chauves-souris par exécution, 2 exécutions par méthode : 13 sur 72 (Myriade) contre 21 sur 72 (vanilla) ; variance entre exécutions très forte (vanilla 10 puis 2 sur 18) ; décision lue au jar identique ; tendance plus basse chez Myriade, non significative |
| interactions, croissance | - | sans objet |

## Écarts connus non corrigés
- Objet au sol dans l'eau (`ItemEntity`, mesure 63) : vanilla joue 5 fois sur 12 un `entity.generic.splash` de source ambiance, volume 0,007 à 0,026, à la surface, pour le butin d'une morue ; Myriade 0 sur 12. Cause à trouver : l'objet flotte-t-il vers la surface (`ItemEntity.setUnderwaterMovement`), sort-il et rentre-t-il dans l'eau, `doWaterSplashEffect` est-il appelé pour lui. Code de l'objet côté Myriade non lu.
- Apparition naturelle (mesure 63 ; vache, mouton, cochon, poule ; graine 12345, `spawn_mobs` vrai des deux côtés ; zones (-3000,5000), (8000,1000), (-6000,-6000)) : à 20 ticks, zone 0 identique groupe par groupe ; les paquets présents ont les mêmes coordonnées des deux côtés. Plus tard Myriade en compte plus (94 contre 83 à 140 blocs) par des paquets entiers visibles d'un seul côté. Trace Myriade (journal temporaire, retiré) : `generate_chunk_packs` place les paquets des chunks forcés sans joueur (chunk -382 -376 : 3 bêtes), puis le niveau passe de 34 à 31 sans `ecs_load_chunk`. Vanilla : le même paquet (-6100, 84, -6008) manque au relevé du forceload seul et apparaît quand le joueur s'en approche. Non prouvé : pourquoi les bêtes forcées de Myriade restent hors du sélecteur (hypothèse : créées avant le niveau complet) ; non comparé ligne à ligne au jar (`spawnMobsForChunkGeneration`, étape SPAWN).
- Mesure 64, chunks forcés sans joueur : `spawn_fresh_chunks` place les bêtes de génération avant que le chunk ait son enregistrement dans l'index spatial (`chunk_hazards`, appelé dans `spawn_loaded_entities`) ; 21 bêtes sur 28 naissaient avec `Cell` = `u32::MAX`, hors de toute colonne : `@e[distance=..]` ne les voyait pas (7 candidats sur 28 dans le rayon), et la zone ne les comptait pas (`simulated_cell` faux). Corrigé par une garde unique au point d'insertion : `enter_index` note dans `Index.unplaced` toute entité qui reste sans colonne (`Cell` = `u32::MAX`), et `adopt_unplaced` (`bridge/host.rs`, appelé par `chunk_hazards` et `chunk_loaded`) la place quand le chunk reçoit son enregistrement ; aucun appel par chemin d'apparition. Chemins d'apparition relus : summon, spawner, reproduction, apparition continue, chargement par fichier, génération ; toutes les insertions (`spawn_species`, `spawn_like`, bateaux, wagonnets, objets, orbes, téléportation, mort) passent par `enter_index` ; seule la génération naissait avant l'enregistrement du chunk ; non vérifié : une bête qui marche vers un chunk sans enregistrement (`reindex` ne note pas ce cas dans `unplaced`) ; `Cell` = `u32::MAX` reste possible un temps pour une entité dont le chunk n'est pas chargé, la garde la place à son chargement. Deuxième cause (mouton 9 contre 13 au banc de forceload) : le biome du paquet était lu dans les chunks chargés, et au bord d'une zone forcée le voisin manque, donc le premier biome du tableau (badlands) : vache à la place du mouton dans le chunk (-385,-381). Le biome est maintenant demandé au générateur à la cellule zoomée du coin (`generate_chunk_packs`), comme le fait la région de génération du jar. Banc de forceload : mouton 13 contre 13 sur 3 exécutions de vanilla (la 4e exécution à 8 compte : moyenne vanilla 11,75 ou 12 contre 13), 20 et 22 contre 20 et 22. Simulation de ces bêtes : chute identique bit à bit dans un chunk forcé sans joueur (Myriade et vanilla) ; au repos 1 sur 19 bougent chez Myriade, 0 sur 12 chez vanilla (vanilla : le jar tient `noActionTime` à 100 ou plus sans joueur ; non relu cette session en octets). Relevé du banc de forceload après correction : mouton 9 contre 9, tout dans 80 blocs 20 et 22 contre 20 et 22 (avant : 2 et 3 à 5). 
- Apparition naturelle à la génération : rouge pour les quatre espèces d'élevage (Myriade bien en dessous de vanilla), cause non trouvée : les tables de biome (`resources/worldgen/biomes.json`, 67 biomes) sont identiques au jar, la génération du code reprend `spawnMobsForChunkGeneration`, 289 chunks donnent bien vache 17, mouton 10, cochon 8, poule 8 ; banc des positions joué (mesure 55) : mêmes coordonnées que vanilla pour les groupes (17 vaches, 10 contre 6 moutons, 8 contre 5 cochons, 8 poules des deux côtés, N = 1 zone), donc pas de manque constaté à la génération dans ce banc, l'écart des mesures précédentes reste inexpliqué ; tirage continu non concluant (rien des deux côtés) ; UUID posé par le flux propre de l'hôte (`bridge/host.rs`, `persist.rs`), vérifié sur des mobs apparus seuls.
- Cochon (mesure 59, N=10 contre vanilla) : monté à main vide après `/summon` avec selle 10/10 comme vanilla, après `/item replace ... saddle` 10/10 (0/10 avant : `equip_stack::set` ne posait pas `Mount.saddled`, corrigé), accroupi refusé 0/10 comme vanilla (verbe `interact` du bot à drapeau secondaire). `data get entity <joueur> RootVehicle` en cours de monte rend le cochon (cause : `sync_level` passait `None` à `ridden_by_others`, le véhicule du joueur invité comptait donc comme partagé ; corrigé) et plus rien après `/ride <joueur> dismount` comme vanilla (reste périmé effacé quand aucune monte n'est en attente). Doublons au rechargement des chunks (mesure 60) : corrigés. Cause : `ecs_load_chunk` (`game/ecs/chunks.rs`), chunk chargé sous le niveau FULL, passait les entités du fichier à `hold_chunk` alors que le monde gardait déjà celles du déchargement de la session (`EcsLevel.chunks`) : elles s'additionnaient (10 cochons posés, 28 après un rechargement, 52 après deux ; vanilla 10). Le défaut est antérieur à 5f3ffd5e7a (code de `d8f3fe4e37`, 2026-11-25, inchangé depuis). Après correctif : chaque cochon étiqueté 1 fois, 14 cochons au total (4 non étiquetés, apparus pendant la mesure, origine non élucidée), 13 au deuxième rechargement.
- Morue (mesures 59, 60 et 62) : premier `entity.cod.ambient` 28/36 des deux côtés (corrigé : `Mob.ambientSoundTime` vaut 0 à l'apparition). Éclaboussure principale : 0,16 des deux côtés, 24/24 (corrigé mesure 62, cause lue au jar : `LivingEntity.checkFallDamage` appelle `updateFluidInteraction` à la fin de `move`, avant la mise à jour de la vitesse). Mesure 63 : sons de surface = butin (ItemEntity), apparition naturelle mesurée ; la morue est verte.
- Chauve-souris : le réveil avant un coup mortel n'est pas reproduit ; suspension spontanée non tranchée (13 sur 72 contre 21 sur 72).
- Seau : données propres au saumon, au poisson tropical, à l'axolotl et au têtard non écrites (seule la morue est vérifiée).
- Mesure 61 et 62 : les cochons sans tag du rechargement (14 contre 10) venaient de `spawn_chunk_packs`, qui ignorait `spawn_mobs` (le jar : `spawnMobsForChunkGeneration` retourne si la règle est fausse) ; corrigé. Recompte par requête numérique (`execute if entity` par étiquette r0 à r9, hors étiquettes, total) : chaque étiquette 1, aucun cochon hors étiquette, total 10, avant, pendant et après deux rechargements, comme vanilla. `Mob.playHurtSound` appelle `resetAmbientSoundTime` ; corrigé (`reset_on_hurt`). Premier son d'ambiance sans blessure, bêtes posées au sol, 12 essais, 60 ticks (Myriade contre vanilla) : vache 11 contre 10, cochon 11 contre 10, renard 8 contre 8, loup 8 contre 9 (pant compris, 3 contre 5 sur 24 sons ensemble des deux côtés). Rappel : l'écart d'apparition naturelle « Myriade en fait moins » a été mesuré avant la découverte que le banc met `spawn_mobs false` : à remesurer avec la même règle des deux côtés. Non vérifié : tests `regles_e_ambient` / `regles_lot6_a2` non compilés ; non-régression g21b et g23 non rejouée pour ces trois correctifs.
- Mesure 65, loups loin d'un joueur : `nearest_target::fired` ne posait la question qu'avant 100 ticks (`ActiveUntil`, la copie de `noActionTime`) ; au jar seule la promenade dépend de `noActionTime` (`RandomStrollGoal.canUse` : refus à 100 ou plus ; remise à 0 dans `Mob.checkDespawn` à moins de 32 blocs d'un joueur, et dans `LivingEntity.hurtServer`). Trace d'un loup en chunk forcé à 400 blocs : questions aux ticks 28, 75, 83, 99 puis plus aucune. Corrigé (`fired` répond toujours ; `sensor_due` du cerveau sans la porte non plus). 1 loup et 4 moutons, 600 ticks, 10 exécutions par cas, moutons restants : loin du joueur avant 1,9 (2, 1, 2...), après 0,2 puis 0 sur 10 avec le binaire final, vanilla 0 sur 10 ; près du joueur 0 sur 10 des trois côtés. Zones d'apparition après correction (3 exécutions) : moutons 26, 26, 30 contre 31 avant et 28 vanilla. `reindex` : une bête qui change de colonne vers un chunk sans enregistrement entre dans `Index.unplaced` (code seul ; le test de poussée ne franchit pas la frontière, ni chez vanilla ; le test par `/tp` perd l'entité dans les trois cas, non élucidé). Vache : zones 1 et 2 de Myriade 8 et 17 contre 5 et 14 de vanilla, non expliqué.
- Mesure 66 : `random_stand.rs` ne lit plus `ActiveUntil` (au jar, `RandomStandGoal` ne lit pas `noActionTime` ; seuls `RandomStrollGoal.canUse` et ses sous-classes `RandomSwimmingGoal`, `WaterAvoidingRandomStrollGoal`/`WaterAvoidingRandomFlyingGoal`, `TurtleRandomStrollGoal`, `GolemRandomStrollInVillageGoal`, plus `Squid$SquidRandomMovementGoal.tick`, le lisent : les portes de `stroll`, `fish_swim`, `fly_stroll`, `turtle_stroll` restent ; la porte du calamar n'a pas été vérifiée). `spawn_chunk_packs` ne teste plus `obstructed` : `WorldGenRegion.getEntities` renvoie une liste vide au jar, aucune bête du paquet ne gêne la suivante pendant la génération (mouton zone 0 : 7 puis 8 comme vanilla, à 20 ticks 4/4). Pertes de cochons vanilla (-383,-369), joueur à (-6000,-6000), relevés à 60/200/600 ticks, N=5 : nous 4/4/4/4/4, vanilla 4/1/4/4/4 (le 1 reste 1 aux trois relevés : la perte ne vient pas d'un chargement différé des sections d'entités). Moutons, 5 exécutions de plus (binaire avant correction du paquet) : 26 chaque fois contre vanilla 22/22/22/22/27 ; avec le correctif : 31 chaque fois ; zone 1 : 13 contre 8-9, car un paquet de 4 en (7953,64,918) que le vanilla perd (santé 8 puis 4 puis mort en 80 à 220 ticks, y qui monte à 64,8) reste vivant chez nous : cause de ces dégâts vanilla non trouvée (bloc en (7954,64,917) : air chez vanilla, autre bloc chez nous). Vaches zones 1 et 2 : 8 et 17 contre vanilla 10/9/9/13/7 et 17/14/15/17/17 : dans la plage. g21b et g23, 1 répétition : différences seulement sur des espèces au mouvement aléatoire (vex, shulker, ghast, allay) et sur le poisson d'argent, qui disparaît aussi au hasard chez vanilla (2 sur 5 contre 1 sur 5 chez nous).
