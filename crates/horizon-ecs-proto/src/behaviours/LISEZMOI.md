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

## Les modes de locomotion (`locomotion/`)
Un mode de déplacement est un module de la même forme (`Module { name, requires, equip, systems, ..Module::DEFAULT }`, `Params`), inscrit dans `locomotion::TABLE` et déclaré par l'espèce : `Def { locomotion: &[(Medium::Ground, walk), (Medium::Water, swim), (Medium::Air, fly), (Medium::Lava, lava)], .. }` (vide : celle du modèle, ou la marche). Le mode actif est un marqueur (`WalkMode`, `SwimMode`, `FlyMode`) que filtrent les systèmes du mode ; on change de mode par `locomotion::switch(world, e, medium)` (événement `Events.mode_changed`) : `reindex` le fait aux entrées et sorties de fluide pour une espèce qui a un mode eau, un but peut le faire (décoller, se poser). Ajouter un mode : un fichier dans `locomotion/`, une ligne dans `TABLE`, un marqueur. `swim` : `needs_water` (air qui baisse hors de l'eau, saut). `fly` : `max_turn`, `hovers`, `glide` ; la gravité est coupée par le contrôle de mouvement tant qu'un chemin est suivi ; vol libre sans chemin : `control` (`Path`, `Float` = ghast, `None` = un but pousse, chauve-souris), `free` (`travelFlying`), `damp_y`. `swim` amphibie (tortue : `amphibious: Some(prefers_shallow)`, `turtle_control`, `push` 0,1) : la nage sert aussi à terre (chemin amphibie, nœud atteint à 1 en vertical, `TurtleMoveControl` dans les deux milieux) ; l'espèce ne déclare alors que ce mode. Le tirage de point (`spots::Spotter.stable`, `Stable::{Ground, Water, Amphibious}`) a la destination stable de la navigation en usage : `pick_for` donne celle de l'eau à un animal en `SwimMode` (un nageur ne tirait aucun point avec celle du marcheur : bloquait `SwimStroll`). vol direct : `Control::Direct` (vex, `noclip`) et `Control::Bank` (phantom), le point voulu est `MoveTo`, un but le pose par `fly::steer_to` ; `swim` `squid: true` (calamar : l'impulsion du cycle des tentacules, `SquidState.vector` posé par les buts `squid_*`). Les buts qui tickent chaque tick (`flier_goals`, `squid_goals`) n'ont pas de système : `arbitrate` appelle leur `tick_all` quand un porteur vit. `lava` : le strider marche sur la lave (carte de chemin, collision de surface, flottaison) ; le mode devient actif à l'entrée dans la lave. Un volant qui se pose se SUSPEND ou se perche : hors de toute requête, réveillé par événement (`PlayerWatch`, blessure) : voir `bat_flutter.rs`.

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

## Apprivoisement (`tame.rs`, `Cap::Tamable`)

Un propriétaire est un composant (`Owner(Option<Who>)`), pas un type d'animal : toute entité qui déclare `Cap::Tamable` peut en avoir un. Assis = l'occupation `OCC_SITTING` (`tame::order_sit`). L'objet qui apprivoise et la chance sont les paramètres du règle `tame::TAME` (`TameParams`), l'ordre de s'asseoir ou de se lever est la même interaction. Les modules (`sit_when_ordered`, `follow_owner`, `owner_target` pour `OwnerHurtBy` et `OwnerHurt`) ne lisent que `Owner`, `Occupied`, la position du joueur propriétaire (`Online`) et son état de combat (`PlayerState.hurt_by/last_hurt`). Conditions d'entrée : `horizon:is_tame`, `horizon:is_sitting` dans `Entry::when`. Un module qui suit un propriétaire demande `PlayerWatch` (un seul rayon par entité : une limite connue) et se réveille quand le propriétaire en sort ; il s'arme à l'entrée de zone pour le cas où le propriétaire est déjà loin. Le propriétaire est un joueur (`Who::Player`) ; un propriétaire créature n'est pas encore lu par les modules.

## Orbe d'expérience (`orb.rs`)

Entité à part comme l'objet au sol : `Pos`, `Vel`, une boîte, un corps d'objet (`Body.object` + `Body.orb` : `ExperienceOrb.tick`, gravité 0,03, frottement égal sur les trois axes, rebond 0,4) et `Orb { value, count, born, age_base, id, following }`, plus les cinq points de vie de `Living` pour la lave et le feu. Au repos : plus de `Moving`, coût nul (le système ne visite que les `OrbAwake`). Attirée par un joueur à moins de 8 blocs : `PlayerWatch(8)` prévient quand un joueur arrive (`Attracted`), le système suit le plus proche trouvé à son tick 1 modulo 20 et le lâche au-delà de 8. Fusion (`scanForMerges`) : au tick 1 modulo 20, même valeur, identifiants à 40 d'écart, rééchelonnée seulement tant qu'une orbe fusionnable est à portée. Fin de vie : échéance de la roue à 6 000 ticks. `orb::award(monde, position, montant, hasard)` coupe un montant en orbes (`getExperienceValue`) et fusionne avec celles qui sont là ; `loot::on_death` l'appelle pour les entités qui ont `XpReward` (règle `loot::XP`, `1 + nextInt(3)` pour un animal, 5 pour un zombie). `orb::collect` : ce qu'un joueur ramasse (le joueur n'a pas de points d'expérience dans le banc). Écarts : l'attraction commence au tick 1 pour une orbe créée à portée et un tick plus tard pour une orbe qui entre dans le rayon (l'événement du rayon est lu au tick suivant) ; le rejet hors de la lave (`velocity` tirée) et le dégagement d'un bloc ne sont pas portés.

## Montures et passagers (`mount.rs`, `Cap::Rideable`)

Une monture est un composant (`Mount { rider, input, jump_strength, max_temper, temper, saddled }`), pas un type d'animal : toute entité qui déclare `Cap::Rideable` peut porter un cavalier (un joueur du banc par son identifiant, ou une créature : elle reçoit alors `Riding(monture)`). Monter est l'interaction du règle `mount::RIDE` (`RideParams`) ou le service `mount::ride(monde, monture, qui)` ; descendre le service `mount::dismount` (le joueur est posé à côté). Être monté est l'occupation `OCC_MOUNTED` et le marqueur `Ridden` ; événements `Events.mounted` / `dismounted` (la monture), lus au tick suivant. Une monture SELLÉE (`Mount.saddled` : `getControllingPassenger`) prend la direction de son cavalier à chaque tick (`ridden_drive`, qui ne visite que les montures montées) : `Mount.input` (avant, côté, lacet, saut de 0 à 1) écrit par l'hôte ; vers l'arrière la moitié d'un quart, côté la moitié, le saut donne `jump_strength * saut` en vertical (et 0,4 en avant). Le joueur cavalier suit le siège dans `Online`. Prédicats nommés `horizon:has_rider` (la monture porte quelqu'un) et `horizon:is_mounted` (l'entité monte quelque chose) pour `Entry::when` (la panique d'une monture : `.when(Not(has_rider))`). `run_around_like_crazy` : une monture sauvage montée court (5 x 4 blocs, vitesse 1,2) et tire un sur 50 : `nextInt(maxTemper) < temper` l'apprivoise (le cavalier devient `Owner`), sinon +5 de tempérament, le cavalier est jeté. Le cheval est une configuration (`species/horse.rs`).

## Le mode « par bonds » (`locomotion/hop.rs`) : slime

Un mode du sol (`Medium::Ground`) à côté de `walk`, `swim`, `fly` : le corps attend sans entrée au sol, se tourne vers sa direction et saute après un délai, garde sa vitesse et son entrée avant en l'air, puis attend de nouveau (`CubeMobMoveControl`). Paramètres (`hop::Params`) : le délai `delay_base + nextInt(delay_extra)` (divisé par `aggressive_div` quand il chasse), le tirage d'une nouvelle direction toutes les `reduced(redirect_base + nextInt(redirect_extra))` ticks au sol, le modificateur de vitesse ; l'impulsion du saut est celle du corps (`travel`). L'attente est une échéance de la roue (`Due::Hop`), le vol est visité à chaque tick (`Hopping`) : un corps qui attend entre deux bonds ne coûte rien. Aucun nom d'espèce ; combinable avec les autres modes (le slime déclare `locomotion: [(Ground, hop)]`). Écarts : la direction tirée en fin d'attente est prise tout de suite (l'original tourne de 90 degrés au plus par tick) ; le premier bond après la naissance attend un tick de plus. Pas portés : le saut du lapin (délai 10 ou 3 selon la vitesse, hauteur 0,2 / 0,3 / 0,5 selon la vitesse et le relief, impulsion avant de 0,1 : il suit un chemin, `RabbitMoveControl`), le saut long de la grenouille et de la chèvre (cerveau), la division du slime à la mort, l'attaque par contact, la taille.
## Cerveaux : paramètres des capteurs, conditions d'une entrée

`Sensor { name, mems, scan, equip, requires }` s'écrit avec `..Sensor::DEFAULT` ; `scan` et `equip` reçoivent le `Ctx` de l'entrée (`ctx.get::<Params>()` : portée, aliments de l'espèce ; `ctx.nth` : rang parmi les entrées du même capteur) ; `SensorEntry::new(&SENSOR, période).with(&Params { .. })`. Le cadre appelle `equip` de chaque capteur (et de chaque comportement) UNE fois à la création (`brain::start`), personne d'autre. La condition d'une entrée de comportement se met dans sa configuration : `needs: &[(&WALK_TARGET, Need::Absent)]` (mémoire présente ou absente), pas dans la porte ni dans `can_start`.

## Points d'intérêt et village (`poi.rs`)

`PoiRegistry` (ressource) : les blocs que les villageois visitent (lits, postes de travail, cloche), leur type (`horizon_world::poi`, que le `Mob` lit aussi), leurs tickets (`acquire`, `release`, `max_tickets` du type : 1 pour un poste, 32 pour une cloche), la recherche des points proches (`in_range(centre, rayon, libres, filtre)` : le plus proche d'abord, égalités par bloc de 16 en z puis x, puis y, z, x), la distance d'un lieu au village en sections (`sections_to_village`, au plus 7 : le plus grand écart sur les trois axes à la section la plus proche qui a un point de village : maison, poste, point de rencontre) et `is_village` (à une section au plus). Le registre est écrit par ÉVÉNEMENTS : un bloc qui a changé (`ChangedBlocks`) est relu, un point apparaît ou part, `added()` et `removed()` donnent ceux du tick d'avant ; rien ne parcourt le monde chaque tick (le monde du banc est indexé une fois au premier tick ; un serveur indexerait chaque bloc de 16 à son chargement). Prédicat nommé `horizon:in_village` pour `Entry::when`. Le registre ne dit pas qui est villageois ni ce que sait un villageois de ses lits (c'est le cerveau).

## Les ticks des modules ne sont pas des systèmes (règle, lot 5)
Un but ou une règle dont le tick tourne à chaque tick (`flier_goals`, `squid_goals`, `locomotion/climb.rs`) n'a PAS de système : chaque système du cadre coûte au calendrier au repos (un système vide ~450 instructions, un système qui a des `Commands` ~850 de plus pour la synchronisation), et la ligne « schedule at rest » de la barrière le mesure. Le module expose `tick_all(world)`, appelé par `arbitrate` (`goals.rs`), qui ne fait rien tant qu'aucun porteur ne vit (le tableau `Carriers`, un `OnceLock` pour le rang du module, ou un registre `Climbers` vide). Les contrôles de vol (ghast, vex, phantom) sont UN système (`fly_controls`). Règle de dérive de la barrière : un pas à moins de 1 % ou de 10 instructions est consigné sans cause ; au-delà, la cause.
## Les locomotions de ce lot
- `walk::Params { climber: true }` (araignée) : `Drive.climber`/`climbing`, le contact horizontal du tick précédent fait grimper (vitesse bornée à 0,15, puis y = 0,2) ; `WallClimberNavigation.pathToPosition` : toute demande de chemin laisse un souhait (`Climbers`) que `climb::tick_all` poursuit en ligne droite quand le chemin est fini ou n'existe pas.
- `swim` amphibie (tortue) : `amphibious`, `turtle_control`, `push` ; `squid: true` (calamar). Les mouvements minuscules (< 0,003) sont coupés AVANT le contrôle, puis le contrôle pousse (ordre de `aiStep`).

## Golem de fer (`village_walk.rs`, `species/iron_golem.rs`, `spots::Home`)
- `Home{center,radius}` : la restriction vanilla (`isRestricted`) des tireurs de position, sans effet sans le composant.
- `village_walk` : `InVillage` (ailleurs 3 fois sur 10, sinon vers un point tenu d'une section de village à 2 sections) et `Back` (hors village seulement ; comme le `Mob`, aucune traction si le village est à plus de 2 sections : marche aléatoire par la promenade de 240 ticks).
- Manque : `OfferFlower`, la marche vers un villageois qui veut un golem (état du villageois), `DefendVillageTarget` (réputation : cerveau du villageois), cible hostile, colère, `MoveThroughVillage`.
- Contrôle `tests/village_vs_mob.rs` : temps de retour sur 40 graines, ECS (q1 1160, médiane 1830, q3 2720) contre `Mob` (1730, 2520, 3000+). Un village que personne ne tient : le registre le voit, le `Mob` (mémoires des villageois) non.

## Règle pour les modules nouveaux : pas de système de plus

Chaque module dont `systems` ajoute un système au planning coûte ~1 000 instructions par tick **même sans porteur** (sa condition d'exécution,
et sa place dans la chaîne des `GoalSlot`) : mesuré le 2026-11-19, le planning à vide (0 créature) coûte 124 266 instr/tick, et 46 608
sans aucun système de module (77 658 pour les modules, soit ~62 %). Un module nouveau n'ajoute donc PAS de système : ses ticks sont
appelés depuis un point existant (le champ `Module::tick`, appelé par `arbitrate` pour les modules que porte un animal vivant ; `goals::tick_state` reprend une fonction système telle quelle), sur les entités qui portent le marqueur de son
but en marche (`With<Diving>`...), et ne coûtent rien quand aucune ne le porte. Les événements réveillent le but (`Req::Start`), les
délais sont des échéances de la roue. Un module qui a vraiment besoin d'un ordre propre (avant `pathfind`, après `apply_damage`) le dit et
le justifie dans son en-tête ; la barrière (`bilan_check.sh`, ligne « schedule at rest ») refuse la dérive.

Le champ `tick` est LA façon : un `fn tick_all` libre que personne n'appelle rend le module inerte (`tests/no_tick_all.rs` le refuse). Les modules toujours actifs (mêlée, marche, regard, reproduction) restent au planning : coût mesuré en `tick`, +21 instr par créature dans la scène du mouton éveillé.

## Comment une espèce choisit son contrôle de déplacement (lot 5)
Par la CONFIGURATION de son entrée de locomotion (`Def.locomotion`), jamais par son nom. Nage (`swim::Params`) : `control` = `Fish` (`FishMoveControl`), `Turtle`, `Drowned` (marqueur `swim::WantsToSwim` posé par ses buts : sinon le contrôle de marche), `Smooth { max_turn_x, max_turn_y, in_water, outside, gravity }` (`SmoothSwimmingMoveControl` : dauphin, axolotl, têtard, grenouille) ; `push` = `Fixed(0,01)` (poisson), `Fixed(0,1)` (tortue) ou `Speed` (la vitesse du mob, avec l'entrée verticale : dauphin, axolotl) ; `sinks` : le `-0,005` d'une bête sans cible ; `amphibious: Some(prefers_shallow)` : chemin amphibie ; `breach` : navigation qui peut sortir de l'eau (dauphin) ; `squid: true`. Vol (`fly::Params`) : `control` = `Path`, `Float`, `Direct`, `Bank`, `None`. Marche (`walk::Params`) : `climber`. Les buts d'une espèce n'ont qu'à poser les marqueurs et les points voulus (`go_to`, `fly::steer_to`, `WantsToSwim`) : le contrôle est celui de l'entrée. Exemples complets : `species/{dolphin,axolotl,drowned,turtle,cod,squid,spider,vex,phantom}.rs`. `Drive.discard_friction` (`shouldDiscardFriction` : le saut long de la grenouille ou de la chèvre) : un but le pose pendant le bond et le retire à l'atterrissage.

## Éviter le soleil (`restrict_sun.rs`, `Nav.avoid_sun`)
`RestrictSunGoal` ne prend aucun drapeau et ne fait rien d'autre que `nav.avoid_sun` ; le module n'a ni échéance ni système : il pose la marque `AvoidSun`, et le chercheur de chemin (`cow::pathfind`) tronque le chemin à la première case que le ciel voit quand il fait jour et que la créature n'est pas déjà vue du ciel (`trimPath`). Non modélisé : le casque qui annule le but (pas d'équipement de mobs). Contrôle `tests/avoid_sun_vs_mob.rs` : squelette sous un toit envoyé en plein air, de jour il s'arrête à x = 6,28 des deux côtés, de nuit à 11,23 des deux côtés. Espèces : squelette ; stray, bogged, parched, wither : à venir avec leurs espèces.

## Projectiles, tirs à cadence (`projectile.rs`, `burst_shot.rs`, `charged_shot.rs`)
- `Flight` : `accel`, `max_age`, `hit` (`Nothing`, `Ignite(ticks)`, `Explode{power,fire}`), `Order::Hurting` (poussée le long de la vitesse puis inertie 0,95, avant le déplacement, sans gravité). Constantes : `ARROW`, `SNOWBALL`, `SMALL_FIREBALL`, `large_fireball(power)`, `SPIT`, `POTION`. Un coup qui explose pose `Events.exploded` (lu au tick suivant) : la capacité d'explosion n'existe pas encore.
- `burst_shot` (rafale : charge, `shots` tirs espacés de `gap`, repos) et `charged_shot` (charge qui monte quand la cible est vue, descend sinon, tir à `at`, puis attente) : paramètres, aucun nom d'espèce ; le projectile est un paramètre.
- Contrôle `tests/shots_vs_mob.rs` : vol identique au `Projectile::tick` du `Mob` (petite boule, grosse boule, crachat), rythme du blaze et du ghast identique, départ et vitesse du ghast identiques.
- Manque : mêlée du blaze, guidage de la balle de shulker, effets de potion, crachat qui traverse, explosion.

## Effets de statut (`effects.rs`, capacité)
- Composant `Effects` (type, niveau, fin, prochain acte) ; `effects::add` / `remove` par événement (`Events.effect_changed`). Aucun tick par porteur : poison (25 >> niveau), wither (40 >> niveau) et régénération (50 >> niveau) sont des échéances de la roue (`Due::Effect`, une par créature à sa plus proche) et les fins aussi. Le compte est celui du `Mob` : l'effet agit aux ticks où le temps restant est un multiple de la période, il est retiré à la fin de son dernier tick, avant le déplacement.
- Ce que chaque effet change : vitesse (`Body.speed`, remise à la fin), faiblesse (`AttackDamage`), résistance au feu (bit `IMMUNE_FIRE` lu par le brûlage) et respiration (bit `BREATHES_WATER`), marqueur `Invisible` (que le ciblage devra lire : pas lu encore), lévitation (champ `Drive.levitation` lu par le déplacement au sol : pas de système).
- `Brew` et `effects::splash` : potion lancée (`OnHit::Splash`), portée 4 x 2 x 4, échelle `1 - distance/4` (la créature touchée : 1), durée `(int)(échelle * durée * 0.75 + 0.5)`, nuisance 6 et soin 4 par niveau. `OnHit` (règle `on_hit_effects`) : effets posés par le coup au corps à corps.
- Sorcière par configuration : `ranged_attack::Params::WITCH` (choix de la potion selon la cible, visée avec l'avance de la cible, vitesse 0,45 ou 0,75). Manque : boire ses potions, soigner les pillards.
- Contrôle `tests/effects_vs_mob.rs` (santé tick par tick, facteurs de vitesse, hauteur en lévitation) et `tests/shots_vs_mob.rs` (choix de la potion, loi de la faiblesse). Manque : lecture de l'invisibilité par le ciblage, effets des joueurs (pas de `Effects`), mort-vivant insensible au poison, faim du husk sur un joueur, piqûres de l'abeille et du poisson-globe, grâce du dauphin, lévitation du shulker (clients à brancher).
- Rebranchement prévu (principe du propriétaire : un effet agit sur un composant autonome, pas sur un drapeau). `Drive.levitation` : la lévitation sera « retirer `Gravity` et poser une poussée verticale » ; le bit `IMMUNE_FIRE` : un composant d'immunité que le brûlage ne visite pas ; la vitesse et la lenteur : un modificateur d'attribut au lieu de l'écriture directe dans `Body.speed` (aujourd'hui `modifiers` recalcule `Body.speed` depuis la base à chaque changement : deux effets simultanés et la remise à la fin se composent, testé contre le `Mob` par `speed_and_slowness_together_compose_and_come_back_as_the_mob_s`). Les trois points d'entrée sont dans `effects::modifiers` et dans deux lignes de `cow::travel_one`.

## Explosion (`explosion.rs`, capacité) et amorçage du creeper
- `explosion::explode(w, Blast{at, radius, fire, owner, source})` : les créatures à moins du diamètre prennent `(pow² + pow) / 2 * 7 * diamètre + 1` avec `pow = (1 - d / diamètre) * exposition` (exposition : part des rayons d'une grille sur leur boîte qui atteignent le centre sans bloc de collision), sont poussées de `pow` à l'opposé, les joueurs prennent les dégâts (sans recul) ; les blocs : 1 352 rayons, puissance `rayon * (0,7 + 0,6 * tirage)`, moins `(résistance + 0,3) * 0,3` et 0,225 par pas de 0,3 ; casse seulement si `mobGriefing` ; avec `fire`, un tiers des cases cassées qui ont de l'air et un sol prennent le feu. Résultat : `BlockChanges` (appliqué au pas suivant), `Events.exploded` (lu le tick suivant), `Cause::Blast`. Manque : objets et projectiles dans le souffle, recul des joueurs, protections des enchantements, sons et particules, `onExplosionHit` propre à certains blocs.
- La logique de Horizon est une méthode privée de `Game` (`game/entities/explosion.rs`) : pas appelable d'un test de la bibliothèque. Contrôle : formules lues et portées, vérifiées par leurs chiffres (dégâts 20 / 16,75 / 5,08 à 1 / 3 / 5 blocs d'un souffle de rayon 3, boule de 80 blocs de terre), et contre le `Mob` pour l'amorçage.
- Amorçage : `Fuse` (valeur, sens, échéance) dans le module `swell` ; l'échéance `Due::Fuse` est armée quand le sens passe à +1, abandonnée quand il passe à -1 : aucun tick par creeper. `Charged` double le rayon. Les creepers explosent au même tick que ceux du `Mob` à un tick près : le départ du but `swell` a un tick de retard sur celui du `Mob`, le même que celui du tir du ghast (ordre de la chaîne : le système du but passe avant le traitement de la demande de départ). Manque : l'allumage au briquet (`Fuse.ignited` existe, rien ne le pose), la boule du ghast creuse déjà par `OnHit::Explode`.

## Téléportation (`behaviours/ender_teleport.rs`, service) et enderman
- Service : `ender_teleport::teleport_to(w, e, spot)` (`LivingEntity.randomTeleport` : le premier bloc `entities_can_teleport_to` sous le point est le sol, la boîte doit être libre, hors liquide et hors des blocs de la balise `Params.avoid`, pas sur un sol qui tient de l'eau si `dry_ground`), `random` (les tirages de `Enderman.teleportRandom`), `towards` (16 blocs vers un point), `repeatedly` (jusqu'à n essais). Module `ender_teleport` : paramètres (`Params::ENDERMAN`) et réaction à un coup qui ne vient d'aucune créature (9 fois sur 10). `Dodges` : un projectile qui touche la créature ne lui fait rien et la fait essayer 64 fois (`projectile.rs`).
- Appelants : `look_for_player` demande `random` quand le regard est à moins de 4 blocs et `towards` après `far_ticks` ticks à plus de 16 blocs de la cible.
- Contrôle `tests/teleport_vs_mob.rs` : 600 téléportations aléatoires de chaque côté, part de réussites 0,50 contre 0,505, écart horizontal moyen 16,7 / 15,4 contre 16,3 / 16,6, hauteur 1,0 des deux côtés ; la flèche esquivée (ECS seul). Manque : fuite de l'eau et de la pluie, départ de jour (`customServerAiStep`), tir des essais d'après la tenue en selle, téléportation du shulker, du loup et du chat vers un point sûr (service à rebrancher), balise `entities_can_teleport_to` et liquides non contrôlés contre le `Mob`, balle guidée du shulker.

## Clients des effets et de la téléportation (branchés)
- `effects::ON_HIT` (règle `on_hit_effects`) testée de bout en bout : un zombie qui porte `CAVE_SPIDER` empoisonne la vache qu'il frappe. Jeux de paramètres prêts pour les espèces à venir : `CAVE_SPIDER`, `BEE_STING`, `WITHER_SKELETON`, `HUSK`, `PUFFERFISH`. Joueurs : `PlayerEffects` (liste par joueur lue au tick demandé : le choix de la sorcière, la vue d'un joueur invisible, la faim du husk) ; les dégâts périodiques et la vitesse d'un joueur restent à son propre système.
- `Undead` (du drapeau du type du `Mob`) : poison et régénération ne font rien ; la nuisance soigne et le soin blesse. Testé contre le `Mob` (zombie empoisonné : même santé).
- Invisibilité lue par le ciblage : la portée de vue tombe à 7 % (`nearest_target`), pour une créature (`Invisible`) et pour un joueur (`PlayerEffects`). Testé : un zombie ne prend pas un joueur invisible à 10 blocs, il prend le visible.
- Flèches à effet : `ranged_bow::Params.effect` (lenteur du stray, poison du bogged, faiblesse du parched) via `OnHit::Effect` ; les espèces arrivent par leur lot de configuration.
- Enderman : l'eau le blesse (`WaterSensitive`, lu par `breathing`) et il s'en va (testé : hors de la mare au tick 4) ; départ de jour par la règle `sun_flee` (même tirage que le brûlage, `flee: true`), sans la porte des 600 ticks ; pluie non modélisée. Loup et chat : `tame::teleport_to_owner` passe par `ender_teleport::land` (le point d'entrée unique du déplacement).
## Montures, laisse, véhicule du joueur (`mount.rs`, `leash.rs`, `vehicle.rs`)
- **Conduite = paramètre** : `RideParams { steer: Steer::{Charged, Stick{..}, Dash{..}}, seats, seat_height, by_player }` dans la règle `RIDE` de l'espèce ; le système `ridden_drive` (avant `travel`) applique l'entrée du cavalier (`Mount.input`, écrite par l'hôte : `forward, strafe, yaw, jump, sprint, steer`), `ridden_seats` (après `travel`) pose les cavaliers. Un cavalier CRÉATURE (`mount::ride(w, monture, Who::Entity(c))`) mène la monture par la navigation : ses `go_to_at` / `stop_nav` vont à la monture (`mount::nav_owner`), les buts MOVE/LOOK/JUMP de la monture sont coupés (`goals::set_disabled_flags`).
- **Laisse** : `leash::attach(w, e, Holder::{Player, Entity, Knot})` / `detach` ; l'espèce déclare la règle `LEASH` (`LeashParams`) ; `Events.leashed` / `unleashed` ; `leash::held_by(holder)` (annuaire) ; prédicat `horizon:is_leashed` pour `Entry::when` ; un but qui doit s'arrêter tenu lit `leash::is_leashed(w, e)`.
- **Véhicule du joueur** : l'hôte écrit `vehicle::set(w, player, Some(Vehicle{..}))` à chaque tick où il change ; `Events.vehicle_moved` réveille les suiveurs (`follow_player_vehicle`).
- Modules : `follow_caravan` (état `Lined`/`Tail`), `defend_leash_holder` (lit `LastHurtBy` du titulaire), `follow_player_vehicle` (`Escorts`, `Escorting`).
## Apparition et disparition naturelles (`spawning.rs`, `despawn.rs`, services du cadre)
**Disparition** : une espèce déclare `despawn: Some(DespawnDef { .. })` dans son `Def` (hérité de la référence). Le cadre ne balaie jamais : `Despawn` (composant) est posé à la naissance par `despawn::admit`. Distance lointaine : mesurée seulement quand un joueur bouge (croissant `Index::crescent`), qu'une créature change de colonne, se réveille ou naît. Entre l'anneau de 32 blocs et la distance : `LeftRing` (écrit par `player_near`) arme `Due::Despawn` à `idle + 1 + geometric(1/800)` ticks du temps propre. `despawn::keep(w, e)` / `release` posent et retirent `Persistent` (nommer, apprivoiser, ramasser : l'appelant du module appelle `keep`) ; `despawn::touch` après une blessure ; `despawn::remove` retire sans butin. **Apparition** : `species/<nom>.rs` donne `spawn: Some(SpawnDef { category, place, rule, cluster, far, peaceful, despawn })` ; `Table::from_biome` résout les listes d'un biome vers les espèces portées (`unported` les liste) ; `Herd::enable_spawning(biome, graine)` allume le système (`spawning::step`, un seul, avant `player_near`) ; il lit les joueurs dans `Online`. Les comptes (global, par chunk, par joueur) sont tenus par les crochets du composant `Counted` : ne jamais recompter ; une créature qui doit être comptée est créée par `spawning::born` ; `uncount` / `recount` suivent `keep` / `release`. Contrôles : `tests/spawning_vs_mob.rs` (le `spawn_for_chunk` réel par `spawn_reference.rs`), `tests/despawn_vs_mob.rs`.
## Creaking : regard qui fige (`stare_freeze.rs`, `species/creaking.rs`)
- Module générique (`Params { wake_range_sqr, dot_min, reach, near, disguise }`) : `Roused` (réveillé : cible joueur posée) et `Stilled` (figé : `follow`, `move_control` et le saut de `travel` l'ignorent, knockback annulé par `Resist(1)` restauré, mémoire `horizon:stilled` qui retire l'activité de combat). Réévalué par ÉVÉNEMENT (`gaze_changed`, mouvement de joueur par l'index spatial) ; un creaking réveillé et libre est réévalué à son changement de cellule. Le `Mob` lit son joueur sur un capteur de 20 ticks : il se réveille en moyenne 8,75 ticks après l'ECS (contrôle `tests/creaking_vs_mob.rs`).
- Les paramètres `active` / `can_move` des comportements de combat (`start_attacking`, `melee_attack`) sont `stare_freeze::is_roused` / `can_move`.
- Non porté : cœur, sons, animations, déguisement (liste vide). Piège du laboratoire : les crates `horizon-ecs-proto` de tous les labos ont le même chemin relatif dans leur espace de travail, donc le même hash de cargo et se piétinent dans le dossier cible commun ; renommer le dossier de sa crate (`crates/proto-<nom>`) règle cela.

## Breeze (`species/breeze.rs`, `brain/behaviours/breeze.rs`, `brain/sensors/attackable.rs`)
- Capteur `attackable` (`BreezeAttackEntitySensor` : `nearest_attackable`, joueurs non créatifs et golems de fer par paramètre), `start_attacking` (finders `NearestAttackable` et `HurtBy`, règle `allowed` de l'espèce), `stop_attacking_if_target_invalid` (fenêtre de 100 ticks, lue aux pas du cerveau).
- Quatre comportements : `SHOOT` (charge de vent, `Flight::BREEZE_WIND_CHARGE`), `SHOOT_WHEN_STUCK`, `SLIDE` (+ `SLIDE_SINK`, le sink de marche qui remet la pose et demande un tir), `LONG_JUMP` (module à part de `behaviours/long_jump` : un but de grenouille et de chèvre avec une case tirée et les angles 65 à 80 ; ici un point derrière la cible, 10 ticks d'inspiration, les angles 40 à 80 sans test de collision de l'arc ; l'arithmétique de l'arc est la même : à factoriser en service quand lot7 sera fusionné).
- `OnHit::WindBurst { radius }` + `explosion::wind_burst` : poussée des créatures dans le diamètre par `(1 - d / diamètre) * exposition`, sans dégât ; manquent la poussée des joueurs et les blocs qui réagissent (portes, leviers).
- Contrôle `tests/it/breeze_vs_mob.rs` (12 runs par côté, un joueur à 10 blocs, 600 ticks) : cible prise au tick 10,75 contre 10,92 ; charges tirées 11,17 contre 10,83 ; premier tir au tick 59,17 des deux côtés ; sauts longs 11,58 contre 10,83 ; plus proche du joueur 3,28 contre 2,90.
## Index spatial, voisinages et commandes parallèles (socle `perf-index`)

- `Index::for_radius(x, z, r, f)` parcourt les blocs de 8 x 8 colonnes par leurs masques d'occupation : le coût suit les colonnes OCCUPÉES, pas la surface ; l'ordre de visite est celui d'une boucle sur toutes les colonnes (lignes, puis colonnes) : un contrôle contre le `Mob` qui dépend de l'ordre des candidats (`nearest_target`, cible la plus proche à égalité, `breed`, `follow_mob`) est inchangé (test `index_tests::for_radius_matches_column_loop`).
- Écouteurs de voisinage : un module qui réveille les écouteurs AUTOUR d'un mobile qui change de colonne (`CellMoves`) inscrit ses écouteurs dans l'index par un composant à crochets `on_insert = watch_in::<K>, on_discard = watch_out::<K>` (K = `WATCH_*` de `cow.rs`) ; le consommateur teste `idx.watched_near(K, cell_du_mobile)` avant toute recherche : coût nul quand personne n'écoute à moins de 32 colonnes. Portée maximale d'un écouteur : 32 colonnes. Branchés : `follow_parent` (les bébés : `GrowAt`), `avoid_entity` (`AvoidsAnimals`), `follow_mob` (`Follower`), `pufferfish_puff` (`Puff`). Les autres consommateurs de `CellMoves` filtrent sur le mobile lui-même (un test de présence de composant) et ne cherchent pas de voisins.
- Un système qui fait des changements STRUCTURELS depuis une itération parallèle utilise `OrdCmds` (pas `ParallelCommands`) : `par.command_scope(e, |mut cmd| ..)` avec l'entité qui émet, et `par.flush()` en fin de système : les commandes s'appliquent triées par entité, donc le résultat ne dépend ni du fil ni de l'exécution (test `parallel_equals_sequential`). Une impulsion de poussée se SOMME (`PushIn` par `add_push`).
- Un objet (objet au sol, orbe) porte `PushFree` : `push` ne le visite pas ; `travel` ne lui émet ni événement d'atterrissage ni dégâts de chute.
- `Config.path_budget` (variable `PATH_BUDGET`, 0 = sans limite) : au plus ce nombre de recherches de chemin par tick, les autres gardent `NeedsPath`.
