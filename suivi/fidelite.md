# Fidélité des espèces contre vanilla 26.3

**Espèces 100 % vertes : 0 sur 6** (mesure 51). Vache, mouton, cochon et poule : l'apparition naturelle à la génération du chunk est
ROUGE (Myriade en fait nettement moins que vanilla, cause non trouvée : les tables de biome sont identiques au jar, voir plus bas) ; cochon : la monte à main vide est corrigée (mesure 55) ; mouton : le broutage du bébé est corrigé (mesure 55). Morue : nage corrigée (mesure 55) ; éclaboussures de la chute : petites éclaboussures encore absentes. Chauve-souris :
toutes les cases du cahier vertes, restes listés plus bas. Cahiers : `suivi/cahiers/cod.md`, `suivi/cahiers/bat.md`.

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
| apparition naturelle | R | à la génération (mêmes graine et zone) : zone 2 vanilla vache 14, mouton 6, cochon 4, poule 8 contre Myriade 4/4/4/0 à 1 ; zone 0 vanilla 7/8/6/4 contre vache 2, cochon 2, rien d'autre ; cause non cherchée ; UUID présent (V) ; le tirage continu (règle des 400 ticks) n'a rien donné des deux côtés en 2 400 à 4 000 ticks : non concluant |

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
| apparition naturelle | R | voir la vache (même banc, mêmes chiffres) ; UUID présent |

## Mouton
| Case | État | Note |
|---|---|---|
| apparition, eau, mort, sauvegarde | V | |
| déplacement, sons | V | déplacement N = 76 : 2,4/7,9 contre 2,7/7,7 |
| interactions | V | blé comme la vache ; tonte : 1 à 3 laines, N = 24 (10/11/3) contre 15 tontes vanilla non nulles (9/5/1) ; teinture : identique (V) ; herbe broutée adulte 10 sur 24 contre 10 sur 24 (V) ; herbe broutée du bébé V (mesure 55) : bébés 24 sur 24 ayant brouté en 300 ticks contre 23 sur 24 en vanilla (avant : 3 sur 24) ; adulte en 1500 ticks 16 sur 24 contre 13 sur 24 |
| croissance | V | |
| apparition naturelle | R | voir la vache (même banc, mêmes chiffres) ; UUID présent |

## Poule
| Case | État | Note |
|---|---|---|
| apparition | V | `IsChickenJockey` ajouté, 43 apparitions identiques |
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
| déplacement | R | 100 ticks : 0,1 contre 3,7 : à 40 puis 140 ticks la position (y 103,0 exact) ne bouge pas du tout, même pas la dérive de -0,005 ; à 600 ticks 6,3 contre 7,1 (V) ; cause non trouvée (but `fish_swim` armé à la naissance, `travel` ne tourne que sous `Moving` : piste, non vérifiée) |
| eau, chute | R | volume de `playSwimSound` corrigé (vitesse, pas `min(0,1, 25 f)` qui est celui du cheval squelette ; tortue 1,5 f) ; `fish.swim` 0,00 à 0,01 comme vanilla ; nage V (mesure 55 : déplacement à 100 et 600 ticks 3,6 et 4,8 blocs contre 4,8 et 5,2) ; éclaboussures de chute : 1 contre 6, les petites (volume 0,02) manquent encore (cause lue, non prouvée : `move_events.rs` 93-95 recrée `StepState` avec `was_water = in_water`, l'entrée dans l'eau d'un poisson immobile ou d'un poisson qui reste à la surface n'est donc jamais vue) |
| sons | V | ambiance (intervalle 120) 16 contre 20 ; blessure et mort présentes |
| dégâts, mort | V | PV 2,0 identiques ; XP dans la loi ; un os de farine à 5 % : la table vanilla `cod` le prévoit, pas un écart |
| interactions | V | seau d'eau : `cod_bucket` avec `bucket_entity_data {Health: 3.0f, PersistenceRequired: 1b, NoAI: 1b}` identique |
| croissance | - | pas d'âge |
| sauvegarde | V | `Motion` identique après rechargement (seul l'ordre des étiquettes diffère) |
| apparition naturelle | ? | non mesurée (poisson d'eau) |

## Chauve-souris (N = 30 déplacements, 10 dégâts, 24 morts)
| Case | État | Note |
|---|---|---|
| apparition | V | `BatFlags` 0 à l'apparition, comme vanilla (43 apparitions) |
| déplacement | V | 100 ticks : 5,6 contre 5,4 ; 600 ticks : 8,7 contre 8,7 ; au sol 0 à 1 sur 30 contre 0 |
| sons | V | ambiance 23 contre 22, volume 0,1, hauteur x0,95 au repos ; blessure et mort présentes (hauteurs dans la loi) |
| dégâts, mort | V | PV 5,0, XP 0 identiques |
| sauvegarde | V | `Motion` et `Rotation` identiques après rechargement (étiquettes dans un autre ordre seulement) |
| réveil par bloc retiré | V | toit retiré : 5 suspendues avant, 0 après (vanilla 8 avant, 0 après) ; l'événement 1025 est émis, non relevé au paquet |
| suspension spontanée | ? | 5 sur 12 contre 8 sur 12 après 1500 ticks (une exécution : non concluant) |
| interactions, croissance | - | sans objet |

## Écarts connus non corrigés
- Apparition naturelle à la génération : rouge pour les quatre espèces d'élevage (Myriade bien en dessous de vanilla), cause non trouvée : les tables de biome (`resources/worldgen/biomes.json`, 67 biomes) sont identiques au jar, la génération du code reprend `spawnMobsForChunkGeneration`, 289 chunks donnent bien vache 17, mouton 10, cochon 8, poule 8 ; banc des positions joué (mesure 55) : mêmes coordonnées que vanilla pour les groupes (17 vaches, 10 contre 6 moutons, 8 contre 5 cochons, 8 poules des deux côtés, N = 1 zone), donc pas de manque constaté à la génération dans ce banc, l'écart des mesures précédentes reste inexpliqué ; tirage continu non concluant (rien des deux côtés) ; UUID posé par le flux propre de l'hôte (`bridge/host.rs`, `persist.rs`), vérifié sur des mobs apparus seuls.
- Cochon (mesure 59, N=10 contre vanilla) : monté à main vide après `/summon` avec selle 10/10 comme vanilla, après `/item replace ... saddle` 10/10 (0/10 avant : `equip_stack::set` ne posait pas `Mount.saddled`, corrigé), accroupi refusé 0/10 comme vanilla (verbe `interact` du bot à drapeau secondaire). `data get entity <joueur> RootVehicle` en cours de monte rend le cochon (cause : `sync_level` passait `None` à `ridden_by_others`, le véhicule du joueur invité comptait donc comme partagé ; corrigé) et plus rien après `/ride <joueur> dismount` comme vanilla (reste périmé effacé quand aucune monte n'est en attente). Doublons au rechargement des chunks (mesure 60) : corrigés. Cause : `ecs_load_chunk` (`game/ecs/chunks.rs`), chunk chargé sous le niveau FULL, passait les entités du fichier à `hold_chunk` alors que le monde gardait déjà celles du déchargement de la session (`EcsLevel.chunks`) : elles s'additionnaient (10 cochons posés, 28 après un rechargement, 52 après deux ; vanilla 10). Le défaut est antérieur à 5f3ffd5e7a (code de `d8f3fe4e37`, 2026-11-25, inchangé depuis). Après correctif : chaque cochon étiqueté 1 fois, 14 cochons au total (4 non étiquetés, apparus pendant la mesure, origine non élucidée), 13 au deuxième rechargement.
- Morue (mesures 59 et 60, N=36 contre vanilla) : `entity.cod.ambient` dans les 60 ticks après l'apparition 28/36 des deux côtés (corrigé : `Mob.ambientSoundTime` vaut 0 à l'apparition, seul un son joué le remet à `-intervalle` ; Myriade armait le premier son à `-120`). Petites éclaboussures (0,01 à 0,03) : 0/36 chez Myriade contre 10/36 chez vanilla, cause non trouvée. Éclaboussure principale : 0,15 chez Myriade contre 0,16 chez vanilla dans 36/36 essais (écart systématique de vitesse d'entrée, non tracé) ; apparition naturelle des poissons non mesurée.
- Chauve-souris : le réveil avant un coup mortel n'est pas reproduit ; suspension spontanée à confirmer sur plus de répétitions.
- Seau : données propres au saumon, au poisson tropical, à l'axolotl et au têtard non écrites (seule la morue est vérifiée).
- Mesure 61 : les cochons sans tag du rechargement (14 contre 10) venaient de `spawn_chunk_packs`, qui ignorait `spawn_mobs` (le jar : `spawnMobsForChunkGeneration` retourne si la règle est fausse) ; corrigé, 10/10/10 avant, pendant et après le rechargement comme vanilla. `Mob.playHurtSound` appelle `resetAmbientSoundTime` : une bête blessée repart à `-interval` ; corrigé (`reset_on_hurt`), son d'ambiance après chute vache/cochon/mouton 1/2/2 sur 12 contre 0/1/3 vanilla (avant : 11/8/11). Non vérifié : petites éclaboussures (piste vy non suivie), tests `regles_e_ambient` / `regles_lot6_a2` non compilés.
