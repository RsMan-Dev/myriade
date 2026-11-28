# Cahier : `InPowderSnow` relu chaque tick

Contexte. Vanilla (`Entity.baseTick`, jar : `isInPowderSnow` copié dans `wasInPowderSnow` puis remis à faux ; `LivingEntity.aiStep` appelle `applyEffectsFromBlocks()` chaque tick, qui ajoute un déplacement nul `oldPosition -> position` quand rien n'a bougé ; `InsideBlockEffectType.FREEZE` rappelle `setIsInPowderSnow(true)`). Donc une créature immobile dans la neige poudreuse la garde vraie, et la relâche DÈS le tick où le bloc disparaît. Chez nous `inside_effects.rs::moved_effects` (l. 245-300) n'est lancé que sous `Changed<Bounds>` : une créature immobile dont on casse la neige garde `InPowderSnow` jusqu'à son prochain mouvement (lecteurs : `behaviours/convert_in_water.rs:14,74,105` : le squelette qui gèle continue son compte ; `hostile.rs:708`). Cause tranchée par lecture du code et du jar ; le geste exact (casser la neige sous un squelette immobile) n'est pas encore rejoué. Limite connue : l'ENTRÉE par pose d'un bloc sur une créature immobile reste sans relecture (rare) ; ne la traite pas sans accord.

Fichiers :
- `crates/horizon-ecs-proto/src/inside_effects.rs` seul : nouveau système à côté de `moved_effects`, enregistré dans `register` (l. 137-139) ; `forget_powder` (l. ~350) et `scan` (l. 173) restent tels quels.

Modèle : `inside_effects.rs:262-270` (insertion et retrait de `InPowderSnow`/`WasInPowder`, `crate::cow::after(w, e, 1, forget_powder)`), `scan` (l. 173, champ `in_powder`), `forget_powder` (même fichier). Le coût de repos est fixe : le système ne visite QUE les entités déjà porteuses du marqueur.

Interfaces :
1. Système `powder_still` : requête `(Entity, &Bounds)` filtrée `(With<InPowderSnow>, Without<IgnoresTriggers>)` ET sans `Changed<Bounds>` (celles qui ont bougé sont déjà traitées par `moved_effects` : `Without` ne marche pas pour un changement, utilise `Ref<Bounds>` et saute si `is_changed()`).
2. Pour chacune : `scan(&level, &mut kinds, &b.0, true, false)` comme `forget_powder` ; si `!in_powder` : retirer `InPowderSnow`, et si `WasInPowder` est absent ne rien d'autre ; si vrai : ne rien faire (le marqueur reste, il n'y a pas d'effet à rejouer sans mouvement).
3. À la sortie : mêmes gestes que `moved_effects` l. 266-268 : `WasInPowder` reste un tick de plus (`crate::cow::after(w, e, 1, forget_powder)`), car vanilla garde `wasInPowderSnow` jusqu'au baseTick suivant.
4. Ordre : après `moved_effects` (`.after(moved_effects)`), pour ne pas relire ce que celui-ci vient de décider ; ne pas dupliquer `living` : le marqueur n'est posé que pour les êtres vivants, le paramètre `living` de `scan` y vaut vrai.
5. Ne pas scanner les entités sans marqueur ; ne pas ajouter de marqueur de plus ; pas de cache par mob.

Tests : banc `mesure/comp` : `SP=skeleton BOXES=spawn,powder PORT=<libre> python3 drv3.py fidXX s_fid.py skeleton` puis `vanilla` : un squelette immobile dans une colonne de neige poudreuse, le bloc retiré à t = 100 ticks : vanilla arrête la conversion (`FreezeConversion`/compte du squelette) au tick suivant ; Myriade doit faire de même (aujourd'hui il continue). Compiler par `PKG=horizon-ecs-proto bash rb.sh` (le coordinateur). Mesurer aussi qu'un monde de 1000 créatures sans neige ne paie rien de plus (la requête est vide).

Fin : le compte du squelette s'arrête le même tick que vanilla (N = 10) ; coût au repos inchangé ; compilation sans avertissement nouveau ; rapport de 8 lignes au plus. Environ 40 lignes. Difficulté : faible pour Haiku.

Interdits :
- toucher un autre fichier que `inside_effects.rs` (besoin : `LEVER :`), en particulier `species/mod.rs` ;
- ajouter une dépendance ; laisser un dossier `target` dans le worktree ; compiler ou lancer les tests (le coordinateur le fait) ;
- nom d'espèce dans un système ; cache par mob entre ticks ; valeur neutre à la place de l'absence du composant ; retirer le `Changed<Bounds>` de `moved_effects` (coût par entité) ; plus de 10 % de commentaires ; plus de 300 lignes ajoutées d'un coup.
