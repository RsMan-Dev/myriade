# Ajouter un comportement (but) ECS : un fichier, une ligne, une entrée d'espèce

Le sélecteur (`goals.rs`) ne connaît aucun but. Un comportement est un `Module` (points d'entrée en `fn` simples, drapeaux
MOVE/LOOK/JUMP, 0 pour un sélecteur de cibles) ; une espèce est une liste `Entry { module, priorité, paramètres }`
(`species.rs`), le même module pouvant y figurer plusieurs fois (les deux tempt du cochon : `Inst.nth` 0 et 1).

1. Copier `behaviours/example.rs` en `behaviours/<nom>.rs`. Y écrire : `Params` (+ `DEFAULT`, valeurs vanilla, noms
   stables : prêts pour un fichier de données), l'état (composants, données seules ; marqueur SparseSet pour « court »), et
   les points d'entrée : `equip` (une fois, crée l'état depuis SES paramètres ; ne connaît pas d'espèce), `arm` (premières
   échéances), `fired` (échéance de la roue tirée : true = demander `can_use`), `can_use` (oui/non ; le choix va dans son
   état), `start`, `stop`, `systems` (requêtes filtrées par SON marqueur + événements réveilleurs, `.in_set(slot)`).
2. Enregistrer : `pub mod <nom>;` et UNE ligne `&<nom>::MODULE,` dans `TABLE` (`behaviours/mod.rs`). L'ordre de la table
   est l'ordre des systèmes ; tous tournent avant `arbitrate` et `pathfind` (un ordre plus fin : `.before/.after` dans `systems`).
3. Donner le but aux espèces : une ligne `Entry::new(&<nom>::MODULE, priorité, &<nom>::Params { ..DEFAULT })` dans la liste de
   l'espèce (`species.rs`, ordre d'enregistrement vanilla).
4. Le cadre fournit : arbitrage par priorité et drapeaux, phase du sélecteur (un tick sur deux : `phase_of`), roue
   (`arm(w, e, slot, attente)`), réveils (`wake(w, e, &MODULE)`, `Req::Start/End(e, slot)`, `Req::Call`), hasard imposé
   (`Forced`, `force_spot`, `fire` pour les tests), `go_to`, `stop_nav`, `aim`, `with_rng`, `skip_unless/skip_if`.
5. Tester : un test dans le fichier du module, et la comparaison à `Mob` dans `tests/*_vs_mob.rs` (garder le test avec
   `skip_unless(&<nom>::MODULE, "...")`). Commande : `HORIZON_TEST_RESOURCES=... HORIZON_SPECIES=minecraft:cow cargo test
   -p horizon-ecs-proto` (cinq espèces : cow, pig, sheep, chicken, zombie). Coût : `prof_bench idle 3000 500 0 300`.

Cible d'attaque (socle `targeting.rs`) : un module d'attaque (mêlée, bond, tir) lit le composant `Target { who: Who::Player(id) | Who::Entity(e), seen, .. }`
du cadre, sans connaître le module qui l'a posé ; `targeting::info(w, who)` ou le paramètre système `Targets` donnent position, yeux, boîte, bébé ;
pour être prévenu d'une acquisition, ajouter une fonction à `TargetWatchers` (dans `resources`) ; pour frapper une créature :
`DamageQueue::add_by_entity(cible, dégâts, (x, z), attaquant)` (la victime peut en faire sa cible : module `hurt_by`). Les modules qui
CHOISISSENT la cible sont `nearest_target` (joueur, types, tag, prédicat nommé ; plusieurs instances) et `hurt_by` (frappeur, alerte).

Règles : pas de système qui parcourt tous les animaux (une requête filtrée par le marqueur du module) ; une échéance
plutôt qu'une scrutation ; le module n'écrit pas dans l'état des autres (il leur demande par `Req`).
Points d'entrée = `&mut World` (système exclusif, comme `arbitrate`) ; une forme à requêtes reste possible plus tard.

Autres composants lourds (déplacement/navigation, dangers, règles propres d'espèce) : la forme (liste d'entrées
{module, paramètres} résolue une fois à la création) s'y prête quand le composant est une liste de comportements
indépendants (dangers : oui ; règles d'espèce : oui, `farm::FarmDef` est déjà ce cas à la main). Elle ne s'y prête pas telle
quelle pour le déplacement/navigation : un seul mode par animal (choix d'un module, pas une liste) et un chemin chaud
(`travel`, `pathfind`) qui ne passe pas par un sélecteur.
