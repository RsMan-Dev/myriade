# Écarts de comportement invisibles — à valider par le propriétaire

Rien n'est appliqué ici tant que le propriétaire n'a pas validé.

## Saut long (grenouille, chèvre) : un seul flot par `pick` au lieu d'une recherche par candidat
- Mesuré (`tests/it/own_static_equal.rs::flood_is_the_search_per_target`, 19 200 couples grenouille/chèvre, flots tous complets) : la recherche par cible dit « atteint » 1 890 fois ; le flot sans cible est d'accord 1 459 fois, dit non 431 fois (23 %), et dit oui 138 fois où la recherche dit non. Cause : `walked` est réécrit à chaque voisin examiné et dépend de l'ordre d'expansion, donc de l'heuristique vers la cible ; l'ensemble atteint n'est pas indépendant de la cible.
- Gain attendu : une recherche par départ au lieu d'environ 40 ; non appliqué. `Finder::flood` existe (inutilisé).
- Ce que le joueur pourrait voir : une grenouille qui saute là où l'original marche (138/19 200) ou qui ne saute pas là où l'original la fait sauter (431/1 890).

## Flot par départ : effet visible (mesuré, à valider par le propriétaire)
Interrupteur `long_jump::LJ_FLOOD` (défaut off ; `LJ_FLOOD=1` dans `prof_bench`). `prof_bench idle 2000 1200` :
- grenouille : sans flot 8770 départs, 6 sauts choisis, 3928 « rien trouvé », distance moyenne 4,04 ; avec flot 20099 départs, 11441 sauts choisis, distance 4,61.
- chèvre : sans 1998 départs, 42 choisis, dist 5,08 ; avec 2000 départs, 2000 choisis, dist 5,47.
Le flot change donc fortement la fréquence des sauts (beaucoup plus de cibles jugées « non atteignables à pied »). Non identique : ne pas activer sans accord.

Mémo par (départ, cible) : essayé puis RETIRÉ (décision du propriétaire : pas de mémoire qui croît avec les mobs). Il évitait 45 % des recherches des grenouilles immobiles et 0,008 % en mouvement ; la position exacte entre dans la recherche (balayage de collision depuis la position fine), donc une clé par case n'est pas identique.
Réutilisation « case d'un chemin trouvé = atteignable » : désaccord 0/5410 (plat), 1/4325 (accidenté, amphibie) : non identique, non branchée.

## Saut long : réutilisation des cases de chemin d'une série (écart ACCEPTÉ par le propriétaire, 2026-11-20)
Une case d'un chemin trouvé par une recherche de la série est tenue pour atteignable (la recherche de cette case est sautée).
Ensemble limité à la série (`SEEN` dans `long_jump.rs`, par fil, vidé à la fin du tirage, quelques centaines de cases au plus).
Mesure (`path_cells_are_reached_by_their_own_search`) : désaccord 0/5410 en terrain plat, 1/4325 en terrain accidenté (marche et amphibie).
Sens du désaccord unique : non relevé (à mesurer si le propriétaire le demande). Banc grenouille : 346 508 -> 180 237 recherches
(48 % évitées, 166 271 cases déjà sur un chemin), mêmes 8770 départs et mêmes 6 sauts choisis. Propriétaire : « un écart sur 4300 ... acceptable ».
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

## Piglin : cause trouvee de l'ecart de fuite (jar : SetWalkTargetAwayFrom)
Trace sur une graine, 1 piglin : meme depart (tick 22), `walk_away::start` tirait UN seul `land_random_pos_away` ; le jar boucle
`for (i < 10) getPosAway(mob, 16, 7, pos)` et prend le premier non nul, avec `WalkTarget(pos, vitesse, 0)`. Sur 5 graines sur 39 l'ECS
ne partait pas du tout (distance 6,8 = depart). Corriges d'apres le jar : dix essais, portee 16 (l'ECS avait 12), arret a 0 bloc
(l'ECS 1). Moyenne sur 39 graines, 20 piglins : Mob 14,63 ; ECS 11,26 -> 13,3. Les 1,15 bloc restants, deux causes (mesures sur 80 graines) :
1. Phase du capteur : le jar tire `timeToTick = random.nextInt(scanRate)` a la creation du capteur (`Sensor.<init>`), puis `--timeToTick <= 0`
   (premier balayage entre 1 et 19 ticks, moyenne ~10) ; l'ECS balayait a 20 ticks fixes (10 ticks de retard moyens = 1,6 bloc a 0,16 bloc/tick).
   Corrige : phase tiree a la creation. L'ecart « 1 piglin 14,65 / 14,0 » n'etait que cela : le Mob du banc avait la meme phase sur toutes
   les graines (`id % 20 = 1`) ; avec des ids de phases variees, 1 piglin : Mob 15,25 / ECS 15,37.
2. Relance : le Mob (`PiglinAvoidRepellent`, `SetWalkTargetAwayFrom.pos(.., 8, ..)`) ne part que si le repoussoir est a moins de 8 blocs
   (`distanceSqr >= 64` refuse) ; l'ECS relancait jusqu'a la fin de la memoire (120 ticks) meme loin. Corrige : `walk_away::Params.within`.
   Phase seule : ECS 15,25 / Mob 14,76 ; avec `within` : Mob 14,76, ECS 14,70 (20 piglins, 80 graines).
Ecartes par un test : poussee entre piglins (mode `ordered_push` : 13,53 contre 13,42 avant les corrections ; et espaces de 1,5 bloc l'ecart
n'etait deja plus que 0,29) ; vitesse (1.0), seuil de depart (8), rayon vertical (7), portee 16 : conformes au jar. Reste non porte :
les deux restes sont corriges : relance a l'arrivee (`walk_away::stop` redemande le but tout de suite tant que le point est memorise ; le
capteur efface la memoire quand il ne trouve plus rien, comme `eraseMemory`) et point fui au bas-centre du bloc (`atBottomCenterOf`).
Mesure finale (20 graines, 20 piglins) : Mob 14,57 (ecart-type 0,47), ECS 14,58.
Recensement des capteurs a phase fixe : seul le repoussoir du piglin en avait une ; les capteurs de cerveau (`brain::arm`) et celui de la
grenouille (`shoot_tongue::arm`) tirent deja `nextInt(periode)` a la creation. Ordre des tirages : le piglin fait un tirage de plus sur son
flux a la creation (`next_int(20)`, au moment ou son module s'arme) ; les tirages suivants du piglin sont decales d'un rang (meme loi).
