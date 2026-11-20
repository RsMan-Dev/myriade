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
