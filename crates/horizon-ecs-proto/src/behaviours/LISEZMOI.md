# Ajouter un comportement (but) ECS : un fichier, une ligne, une entrée d'espèce

Le sélecteur (`goals.rs`) ne connaît aucun but. Un comportement est un `Module` (points d'entrée en `fn` simples, drapeaux
MOVE/LOOK/JUMP, 0 pour un sélecteur de cibles) ; une espèce est une liste `Entry { module, priorité, paramètres }`
(`species/<nom>.rs`, un fichier par espèce, `species/templates/` pour les gabarits), le même module pouvant y figurer plusieurs fois (les deux tempt du cochon : `Inst.nth` 0 et 1).

1. Copier `behaviours/example.rs` en `behaviours/<nom>.rs`. Y écrire : `Params` (+ `DEFAULT`, valeurs vanilla, noms
   stables : prêts pour un fichier de données), l'état (composants, données seules ; marqueur SparseSet pour « court »), et
   les points d'entrée : `equip` (une fois, crée l'état depuis SES paramètres ; ne connaît pas d'espèce), `arm` (premières
   échéances), `fired` (échéance de la roue tirée : true = demander `can_use`), `can_use` (oui/non ; le choix va dans son
   état), `start`, `stop`, `systems` (requêtes filtrées par SON marqueur + événements réveilleurs, `.in_set(slot)`).
   Écrire le module en `Module { name, flags, <vos points d'entrée>, requires: &[Cap::..], ..Module::DEFAULT }` : tout champ
   omis a son défaut (`Module::DEFAULT`), donc un champ ajouté plus tard ne touche JAMAIS un module existant. `requires` = les
   capacités que le module suppose de l'entité (`Cap::Body`, `Living`, `Goals`, `Targets`, `Locomotion`) : une espèce qui a
   le module sans une de ces capacités est une erreur de démarrage qui nomme l'espèce et le module. Points d'entrée optionnels
   (défaut sans effet, pas encore appelés) : `damaged` (l'entité a pris des dégâts), `interact` (un joueur utilise un objet).
   Un module ne lit JAMAIS `SpeciesTable` ni un nom d'espèce : seulement ses `Params` et les composants de capacité.
2. Enregistrer : `pub mod <nom>;` et UNE ligne `&<nom>::MODULE,` dans `TABLE` (`behaviours/mod.rs`). L'ordre de la table
   est l'ordre des systèmes ; tous tournent avant `arbitrate` et `pathfind` (un ordre plus fin : `.before/.after` dans `systems`).
3. Donner le but aux espèces : une ligne `Entry::new(&<nom>::MODULE, priorité, &<nom>::Params { ..DEFAULT })` dans la liste de
   l'espèce (le gabarit, ou `Change::Add/InsertAfter/Set/Priority` dans `species/<nom>.rs` ; ordre d'enregistrement vanilla).
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

## Événements du cadre (un module ne scrute jamais un composant)

`goals::Events` (ressource) : une liste d'entités par événement, lue avec `ev.<nom>.iter()`. GARANTIE : `iter` rend les événements du tick
PRÉCÉDENT, tous et seulement ceux-là ; un système qui tourne à chaque tick voit donc chaque événement EXACTEMENT UNE FOIS, où que
soit sa place dans le schedule (`due` permute au début du tick) ; les événements du tick en cours ne sont visibles qu'au tick
suivant ; personne ne vide ; un système qui ne tourne pas à chaque tick peut en manquer (il lit l'événement dans le tick suivant
celui où il s'est produit, ou jamais) : `ignited` (le feu vient de prendre),
`extinguished`, `entered_fluid`, `left_fluid`, `landed` (les pieds touchent le sol après l'air), `grew` (un bébé a grandi),
`ate` (le broutage a mangé son bloc). Existants dans la même forme : `Hurts` (blessé), `Grown`, `LaidEggs`. Un événement
nouveau = un champ de `Events`, une ligne dans `swap`, un `push` au point où l'état change. À venir (agent du ciblage) : cible
acquise / perdue. Pour réveiller un module : un système `.in_set(slot)` qui lit l'événement et appelle `wake(w, e, &MODULE)`.

## But refusé en l'air

Le `Mob` fait comme le cadre : `GroundPathNavigation.canUpdatePath` est faux hors du sol (ni liquide, ni monture), `nav_move_to`
rend faux, `is_done()` est vrai sans chemin, donc `canContinueToUse` (`!nav.is_done()`) met fin au but au tick suivant ; le
sélecteur redemande `canUse` à chaque tick de sélecteur (un sur deux) tant que le but ne tourne pas. Le but se termine donc
bien en l'air, et c'est son `canUse` redemandé qui le relance à l'atterrissage. Dans le cadre, un but dont le `canUse`
vanilla n'a pas de minuterie (FleeSun, AvoidEntity) doit se faire redemander : `retry_on_end: true` (nouvel essai à la fin du but)
ou `arm(w, e, slot, 2)` dans `stop` tant que la condition de départ tient. Un but avec minuterie (stroll) n'y touche pas.

## Lumière du monde de test

Le monde fuité du prototype est éclairé par le vrai moteur de Horizon (`light_chunk` à la construction, `relight` après chaque
changement de bloc de `block_changes`) : `can_see_sky`, `brightness` et `precipitation_at` voient un toit posé ou retiré
(test : `a_roof_over_a_zombie_stops_the_sun_and_removing_it_lets_it_burn`).

## Le temps (horloge propre à l'entité)

Un module lit le temps par `cow::now_of(w, e)` (dans un système : `clock.of(e)`) et arme par `cow::arm_at(w, e, local, due)` ou
`goals::arm`, JAMAIS par la ressource d'horloge du monde (`Clock.0`) : le temps d'une créature s'arrête pendant son gel
(chunk non simulé). Restent en temps du monde : la roue elle-même, la grille de 20 ticks de `MeleeAttackGoal.canUse`, la phase du
sélecteur (un tick sur deux), le journal des coups, `Daylight`.
