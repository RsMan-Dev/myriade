# Cahier : éclaboussure d'un objet au sol qui entre dans l'eau

Contexte (mesure 63, `suivi/fidelite.md` l. 77 et 138). Vanilla joue 5 fois sur 12 un `entity.generic.splash` de source ambiance (volume 0,007 à 0,026) pour le butin d'une morue ; Myriade 0 sur 12. Jar : `ItemEntity` ne surcharge pas `doWaterSplashEffect` ; `Entity.updateFluidInteraction` l'appelle quand l'eau est touchée, `wasTouchingWater` était faux et `firstTick` est faux. `doWaterSplashEffect` : volume = `min(1, sqrt(vx^2*0.2 + vy^2 + vz^2*0.2) * 0.2)` (0.9 s'il y a un passager qui conduit, 0.2 sinon), son `getSwimSplashSound` sous 0,25 sinon `getSwimHighSpeedSplashSound` (générique), hauteur `1 + (r1 - r2) * 0.4`, puis les particules. L'objet bascule dans et hors de l'eau à la surface, donc rejoue. CAUSE (lecture du code, tranchée mais non testée au banc) : `move_events.rs`, fonction `move_events`, début de boucle : `if b.object || b.orb || b.tnt { continue; }` saute l'objet AVANT le bloc `// updateFluidInteraction` (l. 170-185) qui joue l'éclaboussure. L'objet a pourtant tout ce que la requête exige (`Moving`, `Contact`, `FluidState`, `Body`, `Pos`, `item.rs:171-190`), son `sound_source` est `Ambient` (`species/item.rs:5`, donc `source_of` rend 8) et `TypeName` = `ITEM`.

Fichiers :
- `crates/horizon-ecs-proto/src/move_events.rs` seul (fonction `move_events`, l. 80-196).

Modèle : le bloc existant l. 170-185 (il est bon : volume `0.2 * vitesse`, vitesse de `s.v2` à cause du retard d'un tick de `FluidState`, qui s'applique aussi à l'objet : `item.rs:200` dit qu'un drop en eau y est dès le tick 1).

Interfaces :
1. Remplace le `continue` du début par un drapeau `still = b.object || b.orb || b.tnt` (nom libre). Pour `still` : saute tout le bloc `if cfg.events { ... }` (pas de pas, pas de nage, pas de `flap` : vanilla n'en joue pas pour un objet ; relis `Entity.move` / `isMovementNoisy` dans le jar si tu as un doute, sinon ne change rien) ; garde la mise à jour de `s.dist`, le bloc d'éclaboussure l. 170-185 et la mise à jour d'état l. 187-193.
2. Dans le bloc d'éclaboussure : l'événement de jeu `minecraft:splash` reste conditionné à `heard` comme aujourd'hui ; le son ne se joue pas si `silent`. `names.get(e)` donne `minecraft:item` : `splash_sound` rend `entity.generic.splash` (ni monstre ni espèce spéciale), à vérifier à la lecture de `species/swim.rs:6-14`.
3. Hauteur : `splash_pitch(rng)` rend 1 pour un objet sans flux aléatoire (`rng` absent) : vanilla tire `1 + (r - r') * 0.4` du hasard de l'entité. Si `Rng` n'existe pas sur l'objet, laisse 1 et note-le dans le rapport (écart de hauteur, pas de volume).
4. Le même chemin vaut alors pour l'orbe d'expérience et le TNT amorcé ; vanilla les éclabousse aussi (même `Entity`). Note-le dans le rapport : les autres objets (flèches, wagonnets…) ne sont concernés que s'ils ont `b.object`.

Tests : banc `mesure/comp` : la mesure 63 (`SP=cod BOXES=snd,death ...` : tuer une morue en eau par `/kill`, relever les `entity.generic.splash` de source 8 et leur volume) : vanilla 5 essais sur 12 avec 0,007-0,026 ; Myriade doit s'en approcher sur N = 24 essais (le tirage dépend du rebond de l'objet à la surface, donc on compare la fréquence et la plage de volume, pas chaque essai). Compiler par `PKG=horizon-ecs-proto bash rb.sh` (le coordinateur). Vérifier qu'aucun pas ni son de nage ne sort d'un objet (le banc `snd` d'un objet au sol doit rester muet).

Fin : fréquence des éclaboussures d'objet de l'ordre de 40 % des essais et volumes dans la plage vanilla ; compilation sans avertissement nouveau ; rapport de 8 lignes au plus. Environ 15 lignes. Cause POSSIBLEMENT incomplète : si la fréquence reste à 0, le prochain suspect est la hauteur de la surface (`item_fluids_again`, `item.rs:154`, ou `FluidState.in_water` d'un objet qui flotte au ras de la surface avec une hauteur de fluide sous 0,1 : vanilla bascule dans et hors de l'eau, notre `FluidState` peut rester vrai) : rends alors `QUESTION :` avec les valeurs relevées.

Interdits :
- toucher un autre fichier que `move_events.rs` (besoin : `LEVER :`), en particulier `item.rs` et `species/mod.rs` ;
- ajouter une dépendance ; laisser un dossier `target` dans le worktree ; compiler ou lancer les tests (le coordinateur le fait) ;
- nom d'espèce codé en dur dans un système ; cache par mob entre ticks ; valeur neutre à la place de l'absence d'un composant ; plus de 10 % de commentaires ; plus de 300 lignes ajoutées d'un coup.
