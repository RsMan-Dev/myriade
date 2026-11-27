# Cahier pour `codeur` : la morue (`minecraft:cod`) comme vanilla 26.3

Contexte : la grille `suivi/fidelite.md` donne la morue rouge sur le déplacement, l'eau, les sons, le seau et la sauvegarde.
Tout le comportement ci-dessous est relevé au jar (`runtime/vanilla-26.3/server-26.3.jar`, `javap -p -c -cp <jar> <classe>`).
Classes : `net.minecraft.world.entity.animal.fish.Cod`, `AbstractFish`, `AbstractSchoolingFish`, `WaterAnimal`, `net.minecraft.world.entity.Bucketable`.

Fichiers :
- crates/horizon-ecs-proto/src/species/cod.rs
- crates/horizon-ecs-proto/src/species/templates/fish.rs
- crates/horizon-ecs-proto/src/behaviours/fish_swim.rs
- crates/horizon-ecs-proto/src/behaviours/swim_sound.rs
- crates/horizon-ecs-proto/src/behaviours/held_swap.rs
- crates/horizon-ecs-proto/src/species/swim.rs
- crates/horizon-ecs-proto/src/move_events.rs (éclaboussures et nage)
- crates/horizon-ecs-proto/src/nbt_fields/creatures.rs (clés `Motion`, `FromBucket`)

Modèle :
- Entrée de son d'ambiance d'une espèce : `species/templates/farm_animal.rs:24` (entrée `ambient_sound`, fonction de choix `ambient` ligne 31) et les `Params` de `behaviours/ambient_sound.rs` (champs `interval`, `choose`).
- Boucle d'IA du poisson : `behaviours/fish_swim.rs:9-121` (le but `FishSwimGoal`).
- Échange d'objet en main (seau) : `behaviours/held_swap.rs:1-53`, déjà employé dans `species/cod.rs:15` ; la ligne 4 de `held_swap.rs` dit que les données de seau ne sont pas portées.
- Nage : `species/swim.rs:16` (le son `entity.fish.swim` y est déclaré).

Interfaces : comportement vanilla, instruction par instruction (rien à recopier, tout à traduire dans les modules existants).
1. Intervalle d'ambiance : `WaterAnimal.getAmbientSoundInterval` renvoie 120 (et non 80). `Cod.getAmbientSound` renvoie `entity.cod.ambient`, mort `entity.cod.death`, blessure `entity.cod.hurt`, bond hors de l'eau `entity.cod.flop`. La loi de tirage est celle de `Mob.baseTick` déjà portée dans `ambient_sound.rs` : seul l'intervalle change pour les poissons, qui n'en ont aujourd'hui aucun (0 son contre 20 relevés en vanilla à N = 10).
2. Son de nage : `AbstractFish.getSwimSound` = `entity.fish.swim`. Il ne doit pas partir en rafale à volume 0,1 comme chez nous aujourd'hui : relever dans `Entity.applyMovementEmissionAndPlaySound` et `Entity.waterSwimSound`/`LivingEntity.getSwimSound` quand il part (le poisson est dans l'eau, la mouvement-émission est `EVENTS`/`ALL` selon la classe), et reproduire sa cadence (vanilla : 11 à 51 sons d'éclaboussure ou de nage par mesure, ours 0 pour les éclaboussures).
3. Mouvement dans l'eau (`AbstractFish.travelInWater`) : `moveRelative(0.01, entrée)`, déplacement multiplié par 0,9 sur tous les axes ; si le poisson n'a pas de cible, vitesse verticale décalée de -0,005. Aujourd'hui la morue bouge à peine pendant les 100 premiers ticks (0,1 bloc contre 3,2) : vérifier que `fish_swim` s'active dès le premier tick et que `canRandomSwim` (vrai pour la morue) n'est pas bloqué.
4. But de nage `FishSwimGoal` (priorité 4) : étend `RandomSwimmingGoal` de la base `AbstractFish` ; ne démarre que si `canRandomSwim` et pas de suiveur en cours ; déjà au modèle dans `fish_swim.rs`, à comparer ligne à ligne au jar plutôt qu'à écrire.
5. Bond hors de l'eau (`AbstractFish.aiStep`) : si le poisson n'est pas dans l'eau, est au sol et a une collision verticale, alors : vitesse += (`(nextFloat*2-1)*0,05`, 0,4, `(nextFloat*2-1)*0,05`) ; au sol faux ; synchro demandée ; son `getFlopSound` (`entity.cod.flop`) ; puis `WaterAnimal.aiStep`.
6. Air : `WaterAnimal.handleAirSupply` (tick par tick) : hors de l'eau l'air diminue et blesse, dans l'eau il remonte à 300.
7. Seau : `AbstractFish.mobInteract` appelle `Bucketable.bucketMobPickup` : si l'objet en main est un seau d'eau et le poisson vivant, objet remplacé par `minecraft:cod_bucket`, son `item.bucket.fill_fish`, poisson supprimé. Le seau porte `bucket_entity_data` = {`NoAI`, `Silent`, `NoGravity`, `Glowing`, `Invulnerable`, `PersistenceRequired`, `Health`} (la valeur observée en vanilla : `{Health: 3.0f, PersistenceRequired: 1b, NoAI: 1b}` ; les drapeaux faux n'y figurent pas). Ours rend aujourd'hui un autre objet : trouver pourquoi `held_swap` ne s'applique pas à la morue (ordre des règles, `takes_creature`) puis ajouter les données du seau.
8. Persistance : `AbstractFish.requiresCustomPersistence` = `fromBucket` ou parent ; `removeWhenFarAway` = faux si `fromBucket`. NBT : clé `FromBucket` (booléen) écrite et lue.
9. Sauvegarde et rechargement : la clé `Motion` change après rechargement chez nous ; lire `saveWithoutId` côté vanilla à N = 1 (banc `savea`/`saveb`) et faire écrire la même valeur (la vitesse est sauvegardée telle qu'elle est au tick de l'écriture).
10. Banc de poissons (`AbstractSchoolingFish`) : priorité 5 `FollowFlockLeader` ; `tick` : tous les 200 ticks tirés par `nextInt(200)` un suiveur à plus de 11 blocs (121 en carré) retourne chez son chef ; taille de banc max 8 (morue). Déjà portée dans `follow_flock_leader`, à comparer seulement.

Tests : banc dans le scratchpad `mesure/comp` (voir `suivi/fidelite.md`, en tête) ; pour la morue seule :
`SP=cod BOXES=spawn,move,env,snd,hurt,death,inter,savea,saveb PORT=<libre> python3 drv3.py fid48 s_fid.py cod` puis le même avec `vanilla`, et les analyseurs `fid.py`, `fsnd.py cod fid48`, `fidcmp.py cod fid48`, `fint.py cod fid48`, `fsave.py cod fid48`.
Résultats vanilla attendus : déplacement 100 ticks moyenne 3,2 blocs ; sons `entity.cod.ambient` environ 20 sur la durée du banc ; éclaboussures et nage 11 à 51 (même ordre de grandeur, dans le bruit) ; seau : `minecraft:cod_bucket` avec `bucket_entity_data` ci-dessus ; `Motion` identique après rechargement. Compiler par `PKG=horizon-ecs-proto bash rb.sh` (une fois, après tout écrit) ; la cible de test `it` ne compile pas à ce HEAD (56 erreurs d'avant), ne pas y toucher.

Fin : toutes les cases rouges de la morue dans `suivi/fidelite.md` deviennent vertes au banc (N = 10 au moins par case) ; compilation sans erreur ni avertissement nouveau ; rapport de 10 lignes au plus listant fichier, lignes changées et résultat du banc par case.

Interdits :
- toucher un autre fichier que ceux listés (en cas de besoin : `LEVER :`), en particulier `species/mod.rs` et les crates `myriade-engine` ;
- ajouter une dépendance ; laisser un dossier `target` dans le worktree ; lancer des tests unitaires de `horizon-ecs-proto` (cible `it`) ;
- valeur par espèce codée en dur (nom d'espèce dans un système) : passer par `Params` et les données d'espèce ;
- cache par mob entre ticks ; commenter plus de 10 % du code ; plus de 300 lignes ajoutées d'un coup.
