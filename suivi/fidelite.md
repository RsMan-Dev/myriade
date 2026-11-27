# Fidélité des espèces contre vanilla 26.3

**Espèces 100 % vertes : 0 sur 6.** Aucune n'a toutes ses cases vertes : chaque espèce garde au moins une case rouge,
non concluante ou non mesurée (voir les sections). Vache, mouton et poule n'ont plus de rouge prouvé dans les cases mesurées
(il reste « déplacement non concluant » et « apparition naturelle non mesurée »).

Légende : V vert (identique, ou dans la loi à N répétitions), R rouge (écart prouvé), - sans objet (au jar), ? non mesuré ou non concluant.
Banc : `drv3.py` + `s_fid.py` (serveur Myriade `fid48` contre serveur vanilla, même monde plat, mêmes commandes, bot Bot0 ; analyseurs
`fid.py`, `fsnd.py`, `fidcmp.py`, `fdeath.py`, `fint.py`, `fsave.py`). N = répétitions par case ; déplacement N = 30 par durée.
Les scripts de banc vivent dans le scratchpad de la session ; ils ne sont pas versionnés.

Correctifs de cette mesure (tous petits, < 100 lignes chacun) : voix d'ambiance par la loi `Mob.baseTick` (volume 1,0, variantes
de sons vérifiées au jar) ; brûlure en lave (`entity.generic.burn` 0,4) ; clés NBT par défaut (`Invulnerable`, `attributes`
avec `movement_speed`, `Type`/`variant`, `IsBaby` seulement pour les espèces à drapeau bébé, `IsChickenJockey`, `ForcedAge`) ;
UUID posé par `/summon` et `/data get` ; porc, poule, mouton : l'interaction d'un aliment n'est plus avalée par la règle de monture
de l'hôte (porc non sellé, poule porteuse seulement) ; alimentation d'un bébé en secondes entières avec `ForcedAge`.
Les tests unitaires de `horizon-ecs-proto` (cible `it`) ne compilent pas à ce HEAD (56 erreurs préexistantes dans
`regles_mecanismes_vivant.rs`, `regles_lot6_a2.rs`, `regles_lot10_creatures.rs`) : aucun test ajouté, la preuve est le banc.

## Vache (N = 43 apparitions, 30 déplacements, 10 dégâts, 24 morts, 24 interactions)
| Case | État | Note |
|---|---|---|
| apparition, NBT complet | V | 43 apparitions, clés et valeurs identiques |
| déplacement libre 100/600 | ? | N = 30 : 1,8/7,3 contre 3,4/9,4 ; N = 10 inversé : non concluant |
| eau, lave, chute | V | éclaboussures et brûlure dans le bruit |
| sons | V | ambiance 21 contre 20 ; blessure et mort dans la loi de hauteur ; pas de `cow.step` à 0,15 corrigé |
| dégâts reçus, panique | V | PV identiques ; la panique n'est pas analysée à part |
| mort (butin, XP) | V | N = 24, lois identiques |
| interactions du jar | V | blé : InLove 580 contre 579 (un tick de lecture) ; bébé : ForcedAge 2380 identique après correctif |
| croissance | V | même suite d'Age |
| sauvegarde, rechargement | V | NBT identique (UUID conservé après correctif) |
| apparition naturelle | ? | non mesurée |

## Cochon
| Case | État | Note |
|---|---|---|
| apparition | V | 43 |
| déplacement | ? | 2,9/8,0 contre 3,5/7,2, N = 30 : non concluant |
| eau, lave, chute | V | |
| sons | R | `pig.step` 32 contre 55, 25 contre 57 pour la poule : environ la moitié des pas, reproduit deux fois ; ambiance 22 contre 20 V |
| dégâts, panique | V | |
| mort | V | dans la loi |
| interactions | V | carotte, patate, betterave : InLove 580 contre 579, ForcedAge identique ; selle et monte non mesurées |
| croissance | V | |
| sauvegarde | V | |
| apparition naturelle | ? | |

## Mouton
| Case | État | Note |
|---|---|---|
| apparition, eau, mort, sauvegarde | V | |
| déplacement, sons | V | N = 30 |
| interactions | V | blé comme la vache ; tonte : 1 à 3 laines, N = 24 (10/11/3) contre 15 tontes vanilla non nulles (9/5/1) ; teinture et herbe broutée non mesurées (?) |
| croissance | V | |
| apparition naturelle | ? | |

## Poule
| Case | État | Note |
|---|---|---|
| apparition | V | `IsChickenJockey` ajouté, 43 apparitions identiques |
| déplacement | V | 3,5/10,8 contre 3,7/8,7 (N = 30) |
| eau, lave, chute | V | pas de dégâts de chute |
| sons | R | `chicken.step` 35 contre 68 (même demi-cadence que le cochon) ; ambiance 23 contre 20 V |
| dégâts, mort | V | |
| interactions | V | graines : InLove 580 contre 579 (la règle de monture avalait la graine, corrigé) ; ForcedAge identique |
| croissance | V | |
| sauvegarde | V | |
| ponte | V | EggLayTime identique à l'apparition ; la ponte observée n'est pas comptée |

## Morue (poisson, eau)
| Case | État | Note |
|---|---|---|
| apparition | V | 43, clés identiques |
| déplacement | R | 100 ticks : 0,1 contre 3,2 (elle bouge à peine au début) |
| eau, chute | R | `entity.cod.ambient`, `entity.fish.swim` et éclaboussures absents (ours 0 contre 11 à 51) ; ours émet `fish.swim` à 0,1 en rafale |
| sons | R | ambiance absente (0 contre 20) |
| dégâts, mort | V | PV et XP dans la loi |
| interactions | R | seau d'eau : vanilla donne le seau de morue ; ours donne un autre objet |
| croissance | - | pas d'âge |
| sauvegarde | R | `Motion` change après rechargement |
| apparition naturelle | ? | |

Écart supérieur à 100 lignes (voix, nage, seau, banc de poissons) : noté, non corrigé.

## Chauve-souris
| Case | État | Note |
|---|---|---|
| apparition | R | `BatFlags` 1 (suspendue) contre 0 (en vol) |
| déplacement | R | ours 26/30 au sol ; vanilla vole (0/30) : modèle « suspendue jusqu'à un joueur à 4 blocs » différent de vanilla |
| sons | R | ambiance (`bat.ambient` 0,1) absente : 0 contre 25 |
| dégâts, mort | V | PV 5,0, XP 0 identiques |
| sauvegarde | R | `Motion` et `Rotation` changent |
| interactions, croissance | - | sans objet |

Écart supérieur à 100 lignes (modèle de vol et de repos) : noté, non corrigé.

## Écarts connus non corrigés
- Pas des animaux (`*.step`) : demi-cadence chez le cochon et la poule.
- Sons des bébés (`entity.baby_pig.ambient`, `entity.baby_chicken.ambient`) non gérés.
- Les mobs apparus naturellement n'ont un UUID que s'ils passent par `/data get` ou `/summon`.
- Morue et chauve-souris : voir leurs sections.
