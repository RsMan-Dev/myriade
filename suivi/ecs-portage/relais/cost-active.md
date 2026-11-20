# ecs/cost-active
Batch 1: ECS path_type_static/adjacent_reset in spots/long_jump/ram/cow; Online::for_near sparse bins; IFloor (no libm floor); of_box clamped columns.
Per creature-tick before -> after: sniffer 5422->4306, creaking 10278->9904, frog 60108->51335, cow 92, villager 91, zoglin 131->130.
Open: ghast occupancy::collect/get_or_build, push closure, reindex, silverfish pathfind, wither apply_deferred.

Batch 2 (callgrind per creature-tick, like for like; base binary -> batch 2): ghast 5093->3188, silverfish 3199->2291, sniffer 5914->3389 (after batch 1).
sc.sh: frog 60108->42356, ghast 4440->4051 (batch1->2), sniffer 4285, creaking 9878.
Batch 2 = frog long jump: Occupancy::no_collision in Finder (with_occ) and jump_arc Footprint.occ; float_around::watch one kept-query pass; push reads is_object(e) lazily.
Tests: goat/frog/ghast/silverfish/sniffer species runs green (389 each).
Open: frog Chunk::get 15% (Finder::fluid/state, 8.2M reads, ~220 instr each: per-section cache), dig_position, silverfish Finder (Horizon), arm()/Clock::resource 172 (arm called ~1 per ghast per tick), apply_deferred wither, look_at_target_sink, alloc/archetype churn.
NOTE: scratchpad cg.sh was replaced mid-session by a file targeting ecs-port-dragon/.tgt-dragon; the first test run of batch 1 ran on the dragon tree, rerun on cgm.sh (cerveaux) is green.

Lot 3: BUG corrigé: layer_changed n'avait aucun appelant (couche Walk périmée après changement de bloc); LAYER_EPOCH (global) incrémenté dans block_changes, chaque couche par thread se vide en voyant une nouvelle époque. Test layer_follows_block_changes.
Lot 3 frog: BlockBatch (table directe des états de bloc pendant un `pick`) + Finder garde types/statics/collisions entre recherches d'un même lot (sig = profil+évaluateur+position); Leap.total tenu à jour; silverfish: USE_OWN par défaut + with_occ (Finder propre sur le chemin de marche).
Lot 3 ghast: Hazards::of_box_kept (clé + version dans Cell), Index::column sans floor.
Écart candidat NON appliqué: flot unique par pick (23 % des « atteint » diffèrent) -> suivi/ecs-ecarts.md.
Non fait: dig_position, wither apply_deferred, look_at_target_sink, compteur d'allocations, jump_arc trig/Mutex, draw indexé.

Suite lot 3 (non commitée): jump_arc::angle_trig (sin/cos des 4 angles mis en table). Flot unique: cause trouvée (Node.walkedDistance écrasé sans condition, confirmé au jar PathFinder.findPath putfield avant la comparaison): la recherche avec cible est la référence; flot non conforme par construction. Test A/B: layer_follows_block_changes échoue sans layer_bump.

## Passe coût du saut (2026-11-20, rien commité)
- Correction : une recherche coûte ~170 k instr. (4,76 G / 28 056 recherches), le « 383 » divisait par des appels internes.
  Profil : `find_in` 67 %, dont `find_accepted_slow` 49 % (5,8 M évaluations, ~207 par recherche, ~580 instr. chacune ; ~21 nœuds ouverts par recherche
  d'après le tas : 583 k down_heap / 28 k) ; `jump_vector` 12,7 % (`no_collision` 9,6 %) ; coût fixe par recherche (alloc, tampons) ~1,5 k.
- Fait : couche statique pour l'amphibie (clé de section bit 63, profil XOR constante, invalidation des deux variantes ; test
  `layer_follows_block_changes_amphibious`) ; octets type/sol de la grille gardés entre recherches d'une même série (`kstamp`) ;
  cache tag par état de bloc sur un tirage (`Tags`, 3,7 % : `has_tag` hachait le nom en SipHash) ; réutilisation des cases de chemin (écart accepté).
- `sc.sh` grenouille 24 849 -> 18 421 ; chèvre 13 947 -> 12 373. Tests d'espèces verts.
- Taille couche amphibie : non mesurée (seconde série de sections par chunk, même loi que la couche marche, plafond LAYER_CAP 256 Mo).
- Reste : mémoïser le résultat de `find_accepted_slow` entre recherches d'une série (indépendant de la cible, dépend de node_height/jump/dir/current ;
  difficulté : nœuds intermédiaires de `try_jump_on`), chemin rapide amphibie, paresse interne (voisins évalués à l'ouverture),
  tests A/B de `futile` (au jar : sample(cooldown)/2 = 50-70 ticks grenouille ; nos 600 = écart), `jump_vector` partagé, Mutex clear_transition.
- Chemin rapide `find_accepted_p` ouvert à l'amphibie (cellule Walkable, sol statique ; équivalent à `find_accepted_slow` : un type Walkable exclut l'eau dans la case).
  Preuve : `own_static_equal` (Finder propre = Finder d'Horizon, marche et amphibie) vert ; tests d'espèces verts.
  `sc.sh` grenouille 18 421 -> 15 015 (24 849 au départ, -40 %) ; chèvre 12 374 (13 947 au départ).
- Pas encore fait : mémo des évaluations entre recherches d'une série (taux de répétition non mesuré), curseur de section, nombre de nœuds d'un A* idéal,
  heuristique/arrêt vs jar, chiffre cible par recherche, A/B `futile`, sens du désaccord unique.
- Banc d'UNE recherche (test ignoré `one_search_bench` : `-- --ignored --nocapture`, mêmes 400 couples départ/cible <= 8 cases, meilleur de 5, hors lot de blocs) :
  marche (vache) ~2,2 µs/recherche ; amphibie (grenouille) plat 16,6 µs, accidenté 28,4 µs, soit ~8 à 13 x la marche ; sans chemin rapide amphibie 20,9 / 33,2 µs
  (gain réel du chemin rapide : -21 % plat, -14 % accidenté). Grenouille : 141/400 cibles atteintes contre 254/400 pour la vache (les échecs vont jusqu'au plafond de nœuds).
  `NO_AMPH_FAST` (bench seulement) coupe le chemin rapide dans le même binaire.
- Suite d'espèces après chemin rapide : sortie « checked », aucune ligne d'échec.
- Reste (non fait) : décomposition des ~76 k propres de `find_in`, mémo des voisins par série, paresse interne, compteur par section (Lithium), nombre de nœuds ouverts.
- Banc d'une recherche complété (`ONE_DENSITY=0.0|0.22`, un niveau par processus ; `LAST_VISITED` = nœuds ouverts ; plafond `follow_range*16` = 256) :
  plat : vache 2,2 µs (5,4 nœuds atteinte / 28,6 non atteinte), grenouille 16,6 µs (24 / 42) ; accidenté : vache 11,5 µs (72/400 atteintes), grenouille 21,2 µs (94/400).
  Zombie absent de la table d'espèces du banc. Les recherches non atteintes s'arrêtent à ~42 nœuds (longueur de chemin 8), jamais au plafond 256.
- Jar (javap) : `AmphibiousNodeEvaluator.prepare` pose WATER 0, WALKABLE 6, WATER_BORDER 4 : mêmes valeurs que `mt`. Malus 6 par case ouverte + h x 1,5 = A* proche d'un Dijkstra (~42 nœuds dans un rayon de 8) ; conforme au jeu.
- Où va le coût grenouille : l'amphibie évalue 10 voisins par nœud (8 + haut/bas, `walk_neighbors`) et n'a pas le chemin fusionné `expand_walk` de la marche. Mesure : sans haut/bas, 16,4 -> 5,5 µs (plat, mêmes nœuds) et 22,4 -> 16,0 µs (accidenté, résultat légèrement différent : les nœuds créés par haut/bas ont des effets de bord). Piste identique : fast path du haut/bas (type Open -> `first_ground_below`), à prouver par `own_static_equal`. Non faite.

## Passe 3 (coûts)
- `cached`/`static_floor` scindés en `cached_at`/`static_floor_at` : le fast path de `find_accepted_p` réutilise le `slot` déjà calculé (plus de second `slot`+`fresh` par voisin). Identique (tests `own_` verts). Gain : frog plat 7,4 -> 7,1 us, cow plat 2,1 -> 1,9 us ; rough dans le bruit (13-14 us frog, 9-10 us cow).
- Profil par ligne (frog plat) : le coût est réparti (slot/fresh, lecture type/sol, relaxation, tas ~3 %). Pas de point chaud unique ; plancher ~250 instr par voisin sans refonte de la grille.
- Non fait : boîte de `try_jump_on` par la couche (formes partielles), tampon de colonnes pour `static_floor`/`first_ground_below` : pas de gain démontré dans ce passage.
