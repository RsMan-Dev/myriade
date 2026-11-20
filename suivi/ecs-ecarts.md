# Écarts de comportement invisibles — à valider par le propriétaire

Rien n'est appliqué ici tant que le propriétaire n'a pas validé.

## Saut long (grenouille, chèvre) : un seul flot par `pick` au lieu d'une recherche par candidat
- Mesuré (`tests/it/own_static_equal.rs::flood_is_the_search_per_target`, 19 200 couples grenouille/chèvre, flots tous complets) : la recherche par cible dit « atteint » 1 890 fois ; le flot sans cible est d'accord 1 459 fois, dit non 431 fois (23 %), et dit oui 138 fois où la recherche dit non. Cause : `walked` est réécrit à chaque voisin examiné et dépend de l'ordre d'expansion, donc de l'heuristique vers la cible ; l'ensemble atteint n'est pas indépendant de la cible.
- Gain attendu : une recherche par départ au lieu d'environ 40 ; non appliqué. `Finder::flood` existe (inutilisé).
- Ce que le joueur pourrait voir : une grenouille qui saute là où l'original marche (138/19 200) ou qui ne saute pas là où l'original la fait sauter (431/1 890).
