# Cahier pour `codeur` : la chauve-souris (`minecraft:bat`) comme vanilla 26.3

Contexte : `suivi/fidelite.md` donne la chauve-souris rouge sur l'apparition (`BatFlags` 1 contre 0), le déplacement (au sol 26 sur 30 contre 0), les sons (0 ambiance contre 25) et la sauvegarde (`Motion`, `Rotation`).
Tout est relevé au jar (`javap -p -c -cp runtime/vanilla-26.3/server-26.3.jar net.minecraft.world.entity.ambient.Bat`, aussi `AmbientCreature`).

Fichiers :
- crates/horizon-ecs-proto/src/species/bat.rs
- crates/horizon-ecs-proto/src/behaviours/bat_flutter.rs
- crates/horizon-ecs-proto/src/behaviours/ambient_sound.rs (seulement ses `Params`, pas son code)
- crates/horizon-ecs-proto/src/nbt_fields/creatures.rs (`BatFlags`, lignes 90 et 1022-1030)
- crates/horizon-ecs-proto/src/entity_voice.rs (hauteur de voix, si le choix par espèce s'y trouve)

Modèle :
- Vol et direction : `behaviours/bat_flutter.rs:82-111` (`decide`, `steer`), `species/bat.rs:12-14` (loco `fly`, `damp_y` 0,6).
- Suspendue et réveil : `behaviours/bat_flutter.rs:39-50` (`equip`), `51-55` (`perch`), `65-67` (`take_off`), `68-81` (`wake`), `112-123` (`woke`, `hang`).
- Entrée d'ambiance : `species/templates/farm_animal.rs:24` (entrée `ambient_sound`, fonction `ambient` ligne 31) et les `Params` de `ambient_sound.rs`.
- Champ NBT : `nbt_fields/creatures.rs:1022-1030`.

Interfaces : comportement vanilla, instruction par instruction.
1. Apparition : une chauve-souris invoquée n'est PAS suspendue. `BatFlags` = 0 au départ (octet), `isResting` lit le bit 0 de ce drapeau. Aujourd'hui `equip` (`bat_flutter.rs:39-50`) la suspend si un bloc conducteur est au-dessus : supprimer ce cas, le seul chemin vers le repos est l'étape 5. Au premier tick d'IA elle vole, avec `targetPosition` vide.
2. `Bat.tick` : appelle `AmbientCreature.tick` ; si au repos : vitesse = zéro et position = (x, `floor(y) + 1 - hauteur_boîte`, z) ; sinon vitesse multipliée par (1, 0,6, 1). Puis les états d'animation (client, sans objet).
3. `customServerAiStep`, au repos, chaque tick, d'abord : le bloc au-dessus de `blockPosition().above()` n'est pas conducteur de redstone (`isRedstoneConductor`) : drapeau de repos mis à faux, et si le serveur n'est pas silencieux (`isSilent` faux) `levelEvent` 1025 (envol, `bat.takeoff`) sur la position. Sinon (conducteur) : `nextInt(200) == 0` fixe `yHeadRot` à `nextInt(360)` ; puis si un joueur existe dans les 4 blocs (conditions `forNonCombat`, portée 4,0, sans exiger la visibilité : `BAT_RESTING_TARGETING`), repos faux et le même `levelEvent` 1025 sous la même condition de silence.
4. `customServerAiStep`, en vol : la cible `targetPosition` est effacée si le bloc n'est pas de l'air ou si `y <= minY` du monde. Si la cible est vide, ou `nextInt(30) == 0`, ou la cible est à moins de 2,0 du centre de la bat (`closerToCenterThan(position, 2.0)`) : nouvelle cible `BlockPos.containing(x + nextInt(7) - nextInt(7), y + nextInt(6) - 2, z + nextInt(7) - nextInt(7))` (ordre des tirages : `nextInt(7)` deux fois pour x, `nextInt(6)` pour y, deux fois pour z). Vecteur d'écart = centre de la cible (x + 0,5, y + 0,1, z + 0,5) moins la position ; vitesse += ((`signum(dx)*0,5 - vx`)*0,1, (`signum(dy)*0,7 - vy`)*0,1, (`signum(dz)*0,5 - vz`)*0,1) avec 0,7 pris en flottant 0,699999988 et 0,1 en 0,10000000149 comme le jar. Lacet = `wrapDegrees(atan2(vz, vx) * 57,29577... - 90 - yRot)` ; `zza` = 0,5 ; `yRot` += écart. C'est déjà le contenu de `steer` : comparer les constantes une à une.
5. Fin du pas en vol : `nextInt(100) == 0` et bloc au-dessus conducteur : repos vrai (`setResting(true)`), et, si non silencieux, `levelEvent` 1025 ; c'est la seule façon de se suspendre. Rien dans `perch` ne doit forcer la position avant le tick suivant (c'est `Bat.tick` du point 2 qui la pose).
6. Blessure : `hurtServer` : si le coup est refusé par `isInvulnerableTo`, rien ; sinon, si elle est au repos, repos faux ; puis le traitement normal. (Aujourd'hui `take_off` le fait ; vérifier que l'ordre des tirages de hasard est celui du jar.)
7. Sons : `getAmbientSound` : au repos et `random.nextInt(4) != 0` : aucun son ; sinon `entity.bat.ambient`. Volume `getSoundVolume` = 0,1. Hauteur : `getVoicePitch` = celle d'`AmbientCreature` (`LivingEntity`, base 1,0 et les deux tirages) multipliée par 0,95 (`fmul`, relevé au jar). Intervalle d'ambiance : défaut de `Mob` (80), pas de surcharge. Blessure `entity.bat.hurt`, mort `entity.bat.death`. Vanilla : 25 sons d'ambiance sur la durée du banc, ours 0.
8. Mouvement : `getMovementEmission` = `EVENTS` (pas de son de pas, pas de vibrations de pas) ; `checkFallDamage` ne fait rien ; `isIgnoringBlockTriggers` vrai ; `isPushable` faux, `doPush` ne fait rien, `pushEntities` ne fait rien (`species/bat.rs` : caps et `NO_FALL` déjà là).
9. NBT : `addAdditionalSaveData` écrit `BatFlags` = drapeaux, `readAdditionalSaveData` lit `BatFlags` (défaut 0). Après rechargement, vanilla garde `Motion` et `Rotation` ; chez nous ils changent : relever par le banc `savea`/`saveb` ce que vanilla écrit à N = 1 et à N = 10, puis faire écrire à l'ECS la vitesse et la rotation réelles de la chauve-souris au tick de l'écriture (en vol : celles du pas courant ; au repos : zéro).
10. Apparition naturelle (`checkBatSpawnRules`) : `y` doit être strictement au-dessous de la hauteur de surface (`WORLD_SURFACE`) ; `nextBoolean()` vrai : refus ; luminosité locale `getMaxLocalRawBrightness` au plus `nextInt(4)` ; bloc sous les pieds dans `bats_spawnable_on` ; puis `checkMobSpawnRules`. Comparer à `spawning::Rule::Cave` ; changer seulement ce qui diffère, pas la catégorie.

Tests : banc du scratchpad `mesure/comp` (en-tête de `suivi/fidelite.md`) :
`SP=bat BOXES=spawn,move,snd,hurt,death,savea,saveb PORT=<libre> python3 drv3.py fid48 s_fid.py bat` puis le même avec `vanilla` ; analyseurs `fid.py`, `fsnd.py bat fid48`, `fidcmp.py bat fid48`, `fsave.py bat fid48`.
Résultats vanilla attendus : `BatFlags` 0 à l'apparition (43 sur 43) ; au sol 0 sur 30 à 100 et 600 ticks ; `entity.bat.ambient` environ 25 (volume 0,1) sur la durée du banc ; PV 5,0 et XP 0 à la mort (déjà vert) ; NBT identique après rechargement. Compiler par `PKG=horizon-ecs-proto bash rb.sh` après avoir tout écrit ; la cible `it` ne compile pas à ce HEAD (56 erreurs d'avant), ne pas y toucher.

Fin : toutes les cases rouges de la chauve-souris deviennent vertes au banc (N = 10 au moins) ; compilation sans erreur ni avertissement nouveau ; rapport de 10 lignes au plus.

Interdits :
- toucher un autre fichier que ceux listés (besoin : `LEVER :`), en particulier `locomotion/fly.rs`, `species/mod.rs`, `myriade-engine` ;
- ajouter une dépendance ; laisser un dossier `target` dans le worktree ; lancer les tests unitaires `it` de `horizon-ecs-proto` ;
- nom d'espèce codé en dur dans un système ; cache par mob entre ticks ; plus de 10 % de commentaires ; plus de 300 lignes ajoutées d'un coup.
