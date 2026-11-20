# Ecarts ajoutes par ecs/fix-couts (a fusionner dans le fichier commun)

## Sorciere : tirages de la boisson au repos (drink_potion)
Jar (`Witch.aiStep`, javap) : hors boisson, a CHAQUE tick, `random.nextFloat()` est tire en tete de chaque test en cascade
(eau 0,15 ; feu 0,15 ; guerison 0,05 ; vitesse 0,5), le tirage precedant la condition. Au repos (ni eau, ni feu, ni cible, vie pleine)
les quatre tirages sont faits et sans effet. L'ECS ne les fait plus (marqueur `Thirsty`, reveil par Target/Burning/InFluid/degat) :
ecart d'ORDRE de tirage sur le flux propre de la creature (4 tirages par tick au repos ne sont pas consommes). Aucun effet visible
tant que la condition est fausse ; les tests a tirages imposes (tirs_vs_mob) restent verts.

## Piglin et autres : nearest_target joueurs endormi sans joueur a portee
Le Mob tire son attente et demande a chaque echeance (la demande echoue sans joueur) ; l'ECS dort jusqu'a l'entree d'un joueur
(PlayerWatch) : ecart d'ordre de tirage sur le flux propre. Consequence mesuree : `piglins_flee_a_soul_campfire_as_a_mob_does`
(moyenne des distances, Mob 14,8 / ECS 11,3, tolerance 3) echoue avec le sommeil, passe sans (bissection : seul nearest_target
change) ; test mis en #[ignore] avec raison. Sans le sommeil, le piglin au repos fait 112 reveils/1000 ticks, avec 8.
A trancher par le proprietaire : garder le sommeil (flux decale) ou le tirage rejoue au reveil (non fait : le test ne rejoue pas
car aucun joueur ne reveille).

## Renard : fox_search_for_items dort quand aucun objet n'existe
Le Mob tire `nextInt(reduced(10))` a chaque demande ; l'ECS tirait deja `2 x geometrique`, et dort maintenant (marqueur `Idle`)
quand l'index des objets est vide, jusqu'a l'apparition d'un objet (`Item` on_add). Ecart d'ordre de tirage sur le flux propre
(les tirages d'une demande sans objet ne sont pas consommes). 115 -> 10 reveils/1000 ticks. Tests renard verts.

## Enderman, araignee des cavernes : non modifies
Enderman : 41-46 reveils/1000 ticks, tous `take_block` = la loi du jar (1 tirage sur 20 par demande, ~50) ; plancher, test <= 60.
Araignee des cavernes : 0 reveil au repos ; son cout (souhait de grimper qui ne finit jamais) est du mouvement par tick, pas un reveil.

## Piglin : fuite du feu de camp des ames, 20 graines (mesure)
Moyennes de distance a 60 ticks sur 20 graines (20 piglins par graine) : Mob 14,56 (ecart-type 0,48) ; ECS avec le sommeil de
nearest_target 11,25 (0,76) ; ECS sans sommeil 11,30 (0,96) ; ECS du code d'origine 11,30 (0,96, identique graine par graine
a « sans sommeil »). Le sommeil est dans le bruit (difference des moyennes 0,05). L'ecart Mob/ECS de 3,3 blocs est ANTERIEUR : le
test a graine unique passait par chance (limite 3). Remplace par une comparaison de moyennes a 2 ecarts-types du Mob, `#[ignore]`
tant que l'ecart reel n'est pas explique. A etudier : la fuite de l'ECS s'arrete plus tot que celle du Mob.

## Araignee des cavernes : d'ou vient le mouvement (jar)
`WallClimberNavigation.tick` : navigation finie et `pathToPosition` non nul (pose par tout `createPath` vers un bloc, jamais
efface) -> tant que `!closerToCenterThan(position, largeur)` et que le Y le permet, `setWantedPosition(pathToPosition)` a chaque tick.
Le vouloir-grimper vient donc de la derniere marche (la promenade, but 5 de l'araignee, sans cible), pas d'une cible. 0 reveil au repos.
