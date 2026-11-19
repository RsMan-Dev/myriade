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

Projectile (`projectile.rs`, capacité, pas un but) : une entité `Pos`+`Vel`+`Flight`+`Projectile` (+`OwnTime`), sans vie ni buts ni boîte dans l'index ;
`projectile::launch(w, Flight::ARROW | SNOWBALL, pos, vitesse, propriétaire)` ; le tir d'un but : `behaviours/ranged_bow.rs`.

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

## Capacités à la création

Une espèce déclare ses capacités (`Def.caps`, héritées du gabarit, toutes par défaut) : `Body`, `Living`, `Locomotion`, `Targets`,
`Goals`. `spawn_species` / `spawn_like` ne posent que les composants de ces capacités, le corps d'abord (`body_bundle`) :
`Body` : Pos, Vel, Body, Bounds, Cell, Contact, FluidState, FluidParams, Eye, SpeciesId, Uid, Moving, OwnTime ; `Living` :
Sizes, Health, HurtAt, Frozen, Air, Resist, Immune, TypeName ; `Locomotion` : Yaw, Drive, MoveTo, Nav, Stroll, NavParams ;
`Targets` : FollowRange, AttackDamage ; `Goals` : GoalSet, Look, Head, Jump, Phase, ActiveUntil, Rng. Un module déclare dans
`requires` ce qu'il suppose ; le registre refuse l'espèce qui a le module sans la capacité. `spawn_bundle` (tests, bancs) pose tout.
Ne jamais supposer un composant d'une capacité que `requires` ne cite pas : `world.get::<X>(e)` rend `None` pour un objet.

## Service : positions aléatoires (`spots.rs`)

Un module ne tire JAMAIS sa propre position. `spots::pick_for(w, e, Pick::...) -> Option<[f64; 3]>` : `Pick::Default{h,v}`, `DefaultAway{h,v,from}`,
`DefaultTowards{h,v,to,max_rad}`, `Land{h,v}`, `LandAway{min_h,max_h,v,from}`, `LandTowards{h,v,to}` = `DefaultRandomPos` / `LandRandomPos` du `Mob` :
décalage arrondi en bloc AVANT d'être ajouté à la position, hauteur, destination stable (bloc solide dessous), malus de chemin nul, meilleur de dix
par `walk_target_value` de l'espèce (composant `WalkValue` : `Animal` herbe 10 sinon luminosité - 0,5 ; `Monster` l'ombre ; `Mooshroom`, `Bee`, `Zero`).
Le résultat est (x + 0,5, y, z + 0,5) ; la rangée y est celle du bloc : l'écrire dans la cible du chemin. Stroll (`WaterAvoiding`) et Panic l'emploient.
Limites : navigation au sol seule, pas de `home` (`restricted` toujours faux), le `Mob` a douze cas d'espèce de plus (gardien, strider, tortue...).
Contrôle : `tests/spots_vs_mob.rs` (30 000 tirages par picker contre un vrai `Mob` : part d'herbe, moyennes, taux d'échec).

## Capacités : le corps est en trois

`Transform` (Pos, Vel, SpeciesId, Uid, OwnTime : tout ce qu'a un projectile), `Solid` (Bounds, Cell : dans l'index, poussé, cherché, ciblé), `Body` (Body, Contact, FluidParams, Eye, FluidState, Recheck, Moving : `travel`, chute, fluides ; exige Transform et Solid). `Living` et `Locomotion` exigent `Body` ; les dépendances sont déclarées (`species::NEEDS`) et vérifiées au gel. Test : `no_system_of_the_framework_visits_an_entity_with_only_a_position_and_a_velocity`.

## Condition d'espèce : `Entry::when` (prédicats nommés)

`Entry::new(&m::MODULE, prio, &params).when(&Cond::Is("horizon:is_baby"))` : le CADRE évalue la condition avant le `can_use` du module
(`start_goal`) ; refusée, le but ne démarre pas et le `blocked` du module (« redemander plus tard ») est appelé s'il en a un. `Cond` est une
donnée : `Is(nom)`, `Not`, `All`, `Any` (composables, chargeables depuis des données). Prédicats de base : `horizon:is_baby`, `is_adult`, `is_day`,
`is_night`, `in_water`, `on_ground`, `on_fire`. Un prédicat est une `fn(&World, Entity) -> bool` qui lit des composants de capacité, jamais une
espèce : `species/predicates.rs`. Un mod ou un module en ajoute un avec `Builder::predicate("monmod:nom", f)` ; un nom inconnu dans une
configuration est une erreur de démarrage qui nomme l'espèce et le module. Limite : la condition est évaluée au DÉMARRAGE, pas pendant le but
(un but en cours ne s'arrête pas quand elle devient fausse) ; un module dont la condition peut changer sans événement fournit un `blocked`.
Test : `a_named_condition_gates_an_entry_before_its_module`. Non vérifié contre le `Mob` : les modules de `PORTABLES.md` qui l'emploient (TamePanic...) n'existent pas encore.

## Coût : un module que personne ne porte ne coûte rien

`Registry::carried(i)` : les systèmes et les ressources d'un module ne sont inscrits au schedule que si une espèce résolue le porte (`REGISTER_ALL=1` pour tout inscrire, mesure). Mesure (`goal_bench`, 2 000 vaches au repos, `DUMMY=n` modules factices de 2 systèmes) : 0 module 59, 20 modules 59 (garde) / 65 (tout inscrit), 200 modules 59 / 123 instr par animal et par tick : environ 640 instr par tick et par module inscrit sans garde. Un module porté coûte ses propres systèmes : garde dans le CORPS (sortie immédiate sans porteur vivant), `ProfExec` ignore les `run_if`.

## Services de marche

`goals::go_to_at(w, e, [x, y, z], vitesse, portée)` : chemin vers (x, rangée `floor(y)`, z), terminé à `portée` blocs ; ne rien écrire dans `Stroll` ni `Nav.reach` ; `stop_nav` remet la portée à 1. `move_to_block::MoveToBlock` (composant de l'état du module) : `find_nearest(base, mw, valid)` (anneaux vers l'extérieur, hauteurs alternées : le `Mob` à l'identique, `tests/spots_vs_mob.rs`), `zone`/`invalid_after_change(base, blocs)` (la recherche est une échéance ; un `BlockChanges` dans la zone invalide le bloc), `start(rng)` (les deux tirages du séjour), `tick(pos, distance)` (vrai = redemander le chemin, toutes les 40 ticks), `can_continue`, `target`.

## Barrière de régression

`bilan_check.sh` (dans `crates/horizon-ecs-proto`) : `HORIZON_TEST_RESOURCES=... D=<target>/fast ./bilan_check.sh` lance `bilan.sh` et échoue (sortie 1) si une ligne ECS dépasse sa référence `bilan.ref` de plus de `THRESHOLD` % (1 par défaut) ; une dérogation écrite dans `bilan.waivers` (`ligne|colonne ## pourcent ## raison`) excuse un surcoût qui achète de la fidélité au `Mob` ; `./bilan_check.sh --update "pourquoi"` change la référence et écrit la raison dans `bilan.ref.log`. À lancer à chaque fusion.

## Services du cadre pour les buts (go_to, vitesse, regard, règles, événements)

- Marche : `goals::go_to_at(&mut World, e, [x, y, z], vitesse, portée)` (système exclusif / point d'entrée de module) ; `goals::go_to_entity(w, e, cible, vitesse, portée)` (la position de la cible à cet instant : redemander pour suivre) ; `goals::go_to(&mut Commands, e, x, z, vitesse, but)` est la forme de SYSTÈME (rangée 1, `but` < 32 pour `ASKS`) : le GUIDE qui la dit utilisable depuis un système exclusif se trompe, un système exclusif prend `go_to_at`.
- Vitesse écrite hors de la physique : `goals::set_velocity(w, e, v)` / `goals::impulse(w, e, dv)` posent `Moving` ; ne jamais écrire `Vel` seul.
- Regard : `goals::aim_clamped(cmd, e, look, looking, at, y_speed, x_max)` (borne verticale de ce regard ; `None` = `Look.x_default`).
- Règle de partie : `goals::mob_griefing(w)`.
- `Module.interruptable` (défaut vrai) : un but en cours non interruptible garde ses drapeaux contre toute priorité.
- Événements (lus une fois, au tick suivant) : `Events.hurt` (blessé), `hurt_by_entity` (blessé par une entité ; l'attaquant par `HurtBy`), `entered_powder_snow`, `left_powder_snow`, en plus des précédents.

## État du joueur, occupation, équipement, hasard imposé en 3D

- Joueur : `hostile::PlayerState` est LA source (une seule) : `creative`, `spectator`, `visibility`, `sneaking`, `helmet`, `gaze` (vecteur de regard), `team`, `peaceful` (difficulté) et la règle `targetable(id)` = vivant, ni créatif ni spectateur, pas Peaceful (le ciblage l'emploie). Le `Mob` ne fuit ni ne cible un joueur non ciblable : tout but qui fuit ou vise un joueur appelle `targetable`.
- Occupation : `goals::occupy(w, e, OCC_*)` / `release` / `is_occupied(w, e, masque)` (`OCC_GRAZING`, `MOUNTED`, `SITTING`, `REARING`, `SLEEPING`, `ROLLED`) : le module qui immobilise pose son bit (le broutage le fait), les buts consultent sans connaître ce module.
- Équipement : `goals::equipped(w, e, Slot::Head)` rend l'objet (0 si vide ou si l'entité n'a pas de composant `Equipment`).
- Hasard imposé en 3D : `Forced::impose_spot(e, slot, [x, y, z])` et, dans le module, `Forced::take_spot(e, slot, y_par_défaut)`.

## Règles, réactions, interactions, tête (conformité : aucun nom d'espèce dans le cadre)

- RÈGLES (hors buts) : `rules::Rule { name, equip, attrs, interact }` + `RuleEntry::new(&RULE, &params)` dans la section `rules` de `Def` (héritée du gabarit, la première d'abord). Les règles de la base : `farm::WOOL`, `farm::LAY_EGG` (`EggDef`), `farm::WING_FLAP` (`WingParams` : glissade et immunité à la chute), `hostile::SUN_BURN`. `equip` pose l'état une fois ; `attrs` donne les attributs du corps lus quand les paquets de composants sont faits (`Body.fall_factor`, `Immune`) ; `interact` répond à un objet de joueur (la laine : cisailles). `FarmDef`, `farm_def(nom)` et `burns_in_day` n'existent plus.
- RÉACTIONS : le dégât appelle le point d'entrée `damaged(w, e, inst, &Damaged)` de chaque module du sélecteur (`goals::dispatch_damaged`) : `breed` finit l'amour, `stroll` remet `noActionTime` à zéro. Le cadre ne sait plus ce qu'elles font. Ordre inchangé : dégât, réactions (au point de synchronisation), recul, physique.
- INTERACTIONS : `goals::interact(w, e, &Interaction)` : le premier module qui répond vrai gagne (`breed` : les aliments `Params.foods` mettent en amour), puis les règles (cisailles). 
- TÊTE : `Cap::Head` (`Look`, `Head`) est une capacité à part ; tout but qui regarde ou vise la déclare dans `requires`.
- GARDE-FOU : `tests/no_species_names.rs` : aucun nom d'espèce (`"minecraft:..."` de `species::names()`) dans `src/` hors `species/`, `bin/` et les modules `#[cfg(test)]` ; la tolérance `ALLOWANCE` est vide.

## Les capacités sont indépendantes (règle du propriétaire)

Aucune capacité n'en exclut une autre et aucune n'en suppose une autre au-delà de ce que `requires` / `NEEDS` déclarent. Un système ne demande que les composants qu'il traite : pas de `Without<...>` pour écarter une autre capacité, pas de lecture de `Health` pour un objet. Un objet qui marche, qui a une vie ou des buts est un objet. Les endroits du cadre qui supposaient « toute entité qui a une boîte est une créature » et ont été corrigés pour les objets au sol : `push` (un objet ne pousse pas et n'est pas poussé : `Entity.isPushable`, lu dans `Body.object`) ; `apply_damage` (un objet encaisse chaque coup en entier : `Immune` porte `NO_HURT_COOLDOWN`, `ItemEntity.hurtServer` n'a pas de fenêtre d'invulnérabilité).

## Objets au sol (`item.rs`, `Cap::Item`)

Un objet au sol de vanilla = `Transform` + `Solid` + `Body` (avec `Body.object`) + `Item`, et ses 5 points de vie (`Living` : la lave, le feu et le cactus du cadre le blessent comme une créature). Rien d'autre : ni buts, ni marche.
- `Item { stack, born, age_base, ready_at, target, cell, id }` : la pile, son âge, son délai de ramassage sont des TEMPS PROPRES (`age = now - age_base`), personne ne les compte par tick. `drop_item(w, pile, position, vitesse, délai)` crée tout. La fusion de deux piles et la fin de vie à 6 000 ticks sont celles de Horizon (`merge_with_neighbours`).
- Mouvement : `Body.object` met `travel` en mode objet (`ItemEntity.tick` : gravité AVANT le déplacement, eau 0,99 / lave 0,95 avec poussée de 5,0E-4, frottement de l'air 0,98 et du bloc sous l'objet, rebond -0,5). Un objet posé n'a plus `Moving` : coût nul, il se réveille par les événements des corps (un bloc change, un fluide arrive).
- Fusion par événement : (a) un objet qui bouge (`Moving`) la demande à son tick (tous les 2 ticks tant que son bloc change, tous les 40 sinon, comme `ItemEntity.tick`) ; (b) une échéance de la roue à son multiple de 40 propre, réarmée SEULEMENT tant qu'une pile fusionnable est à portée (un objet seul n'est plus regardé). Vanilla regarde chaque objet tous les 40 ticks même seul : l'écart est la scrutation, pas le résultat (même tick de fusion, mesuré).
- Fin de vie : une échéance (`Due::ItemExpire`), réarmée quand une fusion change l'âge. Elle retire l'objet au début du tick où l'âge atteint 6 000 (vanilla le retire à la fin du même tick : il ne fait pas son dernier déplacement ; invisible pour un objet posé).
- Non porté (restent à `ItemEntity`) : dalle de slime, miel, toile d'araignée, neige poudreuse, colonne de bulles, escape hors d'un bloc posé sur l'objet (`no_physics`), friandises de mod, `native_item_expire/despawn`.
- Contrôle : `tests/item_vs_original.rs` (`ItemEntity.tick` de Horizon tourné sur chaque objet à chaque tick, dans le même monde, `item_reference.rs`). Coût : `prof_bench items|itemfall|itemsref N TICKS 0`.
