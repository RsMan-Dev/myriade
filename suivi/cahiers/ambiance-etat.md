# Cahier pour `codeur` : les sons d'ambiance qui lisent l'état (8 espèces et le happy ghast monté)

Contexte : `Mob.baseTick` joue `getAmbientSound` quand `nextInt(1000) < ambientSoundTime++`, puis remet le compteur à `-getAmbientSoundInterval()`. Le module `behaviours/ambient_sound.rs` porte déjà cette loi (`Params { interval, choose }`, `choose` rend le nom du son, `""` = pas de son). Il manque l'entrée `ambient_sound` des sept espèces ci-dessous (aucune n'en a : `grep ambient species/{allay,creaking,warden,armadillo,sniffer,panda,parrot}.rs` est vide), le jeu de sons du chat bébé, et l'intervalle du happy ghast monté.
Tout est relevé au jar (`javap -p -c -cp runtime/vanilla-26.3/server-26.3.jar net.minecraft.world.entity.<classe>`). Les noms de sons existent dans `runtime/resources-myriade/registries.json`.

Découpe en quatre lots, fichiers disjoints, à lancer en parallèle (chacun au plus 300 lignes) :
- Lot A : `behaviours/ambient_sound.rs`, `species/happy_ghast.rs`, `species/cat.rs`.
- Lot B : `species/allay.rs`, `species/creaking.rs`, `species/armadillo.rs`.
- Lot C : `species/warden.rs`, `species/sniffer.rs`, `species/panda.rs`.
- Lot D : `species/parrot.rs`.
Chemins relatifs à `crates/horizon-ecs-proto/src/`. Seul le lot A touche `ambient_sound.rs`. Les lots B, C, D n'ont besoin d'aucune nouveauté de ce fichier (`choose` existe).

Modèle commun :
- Entrée type : `species/cat.rs:60` (`Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { interval: Some(120), choose: Some(cat_ambient), ..ambient_sound::Params::DEFAULT })`) et `choose` : `species/cat.rs:19-33`, `species/horse.rs:12`, `species/happy_ghast.rs:118-121`. Ajouter `ambient_sound` à l'`use` des modules de l'espèce.
- Priorité : un numéro libre de la liste de l'espèce (14 pour le chat) ; le module ne prend pas de capacité qui gêne un autre but.
- Volume : `entity_voice.rs:16-38` (`volume_of`) a déjà allay 0,4 et warden 4,0 ; les autres espèces : 1,0. Hauteur : `tear_down.rs:86-106` (`voice`) tire les deux `nextFloat` de `LivingEntity.getVoicePitch` et monte le bébé de 1,5 ; rien à changer.
- `ambient_sound::play` ne joue rien quand `choose` rend `""` (vanilla : `getAmbientSound` nul).

## Lot A : généricité, happy ghast, chat bébé

Interface générique, la plus petite : dans `ambient_sound::Params`, ajouter `pub interval_of: Option<fn(&World, Entity) -> u32>` (défaut `None` dans `DEFAULT`) ; dans `delay` (`ambient_sound.rs:43-52`), l'intervalle utilisé est `p.interval_of.map_or(i, |f| f(w, e))` quand `interval` est `Some(i)`. Aucun nom d'espèce dans le système ; la fonction vit dans le fichier de l'espèce. Limite à écrire en commentaire : l'intervalle est lu à l'armement du délai suivant, pas à chaque tick (vanilla le relit à chaque tick ; l'écart n'agit que sur la durée qui suit une monte ou une descente).

Happy ghast (`animal.happyghast.HappyGhast`) :
1. `getAmbientSoundInterval` = `Animal.getAmbientSoundInterval` (120) ; si `isVehicle()` : fois 6, soit 720.
2. `getAmbientSound` : bébé `entity.ghastling.ambient`, sinon `entity.happy_ghast.ambient` (déjà `species/happy_ghast.rs:118-121`). Volume 1,0 bébé et 4,0 adulte (déjà `entity_voice.rs:22`), hauteur 1,0 fixe (`getVoicePitch` rend `fconst_1` : voir si `voice` doit l'ignorer ; relever d'abord si la hauteur du bébé 1,5 s'applique ; si `getVoicePitch` est redéfini à 1, le tirage de hasard ne se fait pas).
3. État manquant : « porte au moins un passager ». Chercher l'existant (`still_under_riders`, `species/happy_ghast.rs:13`, composant des passagers dans `behaviours/`) et le lire ; ne rien créer si un composant des passagers existe.
4. `species/happy_ghast.rs:57` : ajouter `interval_of: Some(riders_interval)`, avec `interval: Some(120)` inchangé.

Chat bébé (`animal.feline.Cat.getSoundSet`) : `isBaby()` rend les sons `babySounds` de la variante, sinon `adultSounds`. Jeu bébé, identique pour `classic` et `royal` : `entity.baby_cat.purr`, `entity.baby_cat.purreow`, `entity.baby_cat.ambient`, `entity.baby_cat.stray_ambient`. Dans `species/cat.rs:21-33` (`cat_ambient`) : si le chat est bébé (`panda_state::is_baby(w, e)`, `behaviours/panda_state.rs:56`), le jeu est `entity.baby_cat.*` quelle que soit la variante. L'ordre de choix est déjà celui du jar (apprivoisé : amour puis `nextInt(4) == 0` puis ambiance ; sauvage : `stray_ambient`) ; ne pas le changer. Intervalle 120, déjà là.

## Lot B : allay, creaking, armadillo

1. Allay (`animal.allay.Allay`, hérite de `PathfinderMob` : intervalle `Mob` 80) : `getAmbientSound` = main pleine (`hasItemInSlot(MAINHAND)`) : `entity.allay.ambient_with_item`, sinon `entity.allay.ambient_without_item`. Volume 0,4 (déjà). État : `goals.rs:418-428` (`main_hand(w, e).is_some()`). La danse (`event_listen::Dancing`, `behaviours/event_listen.rs:55`) n'est pas lue par l'ambiance : ne pas l'utiliser. Entrée dans `species/allay.rs`, `interval: Some(80)`.
2. Creaking (`monster.creaking.Creaking`, `Monster` : 80) : `isActive()` rend nul (aucun son), sinon `entity.creaking.ambient`. État : `behaviours/stare_freeze.rs:66-68` (`is_roused`, composant `Roused`). Volume 1,0. Entrée dans `species/creaking.rs`, `interval: Some(80)`.
3. Armadillo (`animal.armadillo.Armadillo`, `Animal` : 120) : `isScared()` rend nul, sinon `entity.armadillo.ambient`. État : `behaviours/armadillo_state.rs:5-9` (`ArmadilloState`, `of(w, e) == SCARED` = 2 ; vérifier au jar que `isScared` n'est que l'état `SCARED`, pas `ROLLING`). Entrée dans `species/armadillo.rs`, `interval: Some(120)`.

## Lot C : warden, sniffer, panda

1. Warden (`monster.warden.Warden`, `Monster` : 80, pas de redéfinition) : `getAmbientSound` : `isDiggingOrEmerging()` (pose `DIGGING` ou `EMERGING`) rend nul ; sinon le son du niveau de colère (`AngerLevel`) : calme `entity.warden.ambient`, agité (`>= 40`) `entity.warden.agitated`, furieux (`>= 80`) `entity.warden.angry`. État : `behaviours/suspect_anger.rs:49-95` (`Level::{Calm, Agitated, Angry}`, `level(w, e)`) ; les seuils vanilla 40 et 80 sont à comparer à `Suspects.p` (`suspect_anger.rs:91`). Pose : chercher comment `emerge` et `dig` (`species/warden.rs:17-18`) marquent leur état (composant) et le lire ; si rien n'est lisible sans ajout, `LEVER :`. Volume 4,0 (déjà). Entrée dans `species/warden.rs`, `interval: Some(80)`.
2. Sniffer (`animal.sniffer.Sniffer`, `Animal` : 120) : si l'état est `DIGGING` ou `SEARCHING` : nul ; sinon `entity.sniffer.idle`. État : `brain/behaviours/sniff.rs:22-53` (`state(w, e)`, `DIGGING` = 5, `SEARCHING` = 4). Entrée dans `species/sniffer.rs` (la liste n'a que `float` ; ajouter l'entrée, `interval: Some(120)`).
3. Panda (`animal.panda.Panda`, `Animal` : 120) : `isAggressive()` : `entity.panda.aggressive_ambient` ; sinon `isWorried()` (gène `WORRIED`, sans orage dans ce jar) : `entity.panda.worried_ambient` ; sinon `entity.panda.ambient`. État : `behaviours/panda_state.rs:30-65` (`gene(w, e)`, `is_aggressive`). Vérifier au jar le corps d'`isAggressive` (gène `AGGRESSIVE`). Entrée dans `species/panda.rs`, `interval: Some(120)`. Bébé : `AgeableMob.getVoicePitch` déjà fait par `voice`.

## Lot D : perroquet

Perroquet (`animal.parrot.Parrot` : `ShoulderRidingEntity` puis `TamableAnimal` puis `Animal` : 120 ; `getVoicePitch` = `Parrot.getPitch(random)`, soit `(nextFloat - nextFloat) * 0,2 + 1,0`, le même calcul que `LivingEntity` ; pas de bébé) :
1. `getAmbientSound` = `Parrot.getAmbient(level, level.getRandom())` : si la difficulté n'est pas `PEACEFUL` et `nextInt(1000) == 0` : tirer `nextInt(taille)` dans la liste des clés de `MOB_SOUND_MAP` (ordre et noms : section statique de `Parrot`, `javap -p -c`) et rendre `getImitatedSound(type)` (`entity.parrot.imitate.<espèce>`, ex. `entity.parrot.imitate.zombie`) ; sinon `entity.parrot.ambient`.
2. Hasard : vanilla tire dans le hasard du niveau, pas celui de l'animal ; ici, utiliser le hasard de l'animal (`goals::with_rng`), et le dire en un commentaire (l'effet : 1 son sur 1000, hors banc).
3. Difficulté : chercher la lecture existante (`grep -rn "Peaceful\|difficulty" behaviours/`) ; si aucune lisible depuis `choose`, `LEVER :`.
4. Entrée dans `species/parrot.rs` (liste lignes 14-21), `interval: Some(120)`. Pas de nom d'espèce dans un système : la table des imitations vit dans `species/parrot.rs`.

Tests : pas de test unitaire `it`. Banc du scratchpad `mesure/comp` (en-tête de `suivi/fidelite.md`) :
`SP=<espèce> BOXES=snd PORT=<libre> python3 drv3.py <tag> s_fid.py <tag>` puis le même avec `vanilla` ; analyseur `sumsnd.py <tags> <binaire> '<regex du son>'`. Attendu vanilla : même nombre de sons d'ambiance sur la durée (chaque espèce à son intervalle : allay, creaking, warden 80 ; armadillo, sniffer, panda, perroquet, chat 120 ; happy ghast 120 et 720 monté) ; à l'état marqué (armadillo roulé, creaking actif, sniffer en fouille, warden en émergence) aucun son ; panda gène agressif ou inquiet, allay main pleine, chat bébé, warden agité : le nom du son change comme ci-dessus. Compiler une seule fois par lot (`CARGO_INCREMENTAL=0`, dossier cible commun, sans `-j` élevé).

Fin : chaque son ci-dessus se joue sous le nom et à l'intervalle du jar ; compilation sans erreur ni avertissement nouveau ; rapport de 10 lignes au plus. La table attendue de `species/mod.rs` (`the_resolved_species_equal_their_expected_configuration`, lignes 787 et suivantes) doit gagner `("ambient_sound", n)` pour chaque espèce touchée : le coordinateur l'écrit, le codeur le signale (`LEVER :` pour ce fichier).

Interdits :
- toucher un autre fichier que ceux de son lot (besoin : `LEVER :`), en particulier `species/mod.rs`, `goals.rs`, `entity_voice.rs`, `tear_down.rs`, `myriade-engine` ;
- ajouter une dépendance ; laisser un dossier `target` dans le worktree ; lancer les tests unitaires `it` de `horizon-ecs-proto` ;
- nom d'espèce codé en dur dans un système (`ambient_sound.rs` reste générique) ; cache par mob entre ticks ; plus de 10 % de commentaires ; plus de 300 lignes ajoutées d'un coup.
