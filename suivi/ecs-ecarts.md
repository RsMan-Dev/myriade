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
