# Cahier : admire_seen (piglin voit de l'or)
Racine : crates/horizon-ecs-proto. NE COMPILE PAS, ne lance aucun test.
Fichiers :
- NOUVEAU src/behaviours/admire_seen.rs (< 140 lignes).
- src/behaviours/mod.rs : `pub mod admire_seen;` et `&admire_seen::MODULE,` dans la liste (près de `&admire::MODULE,`).
- src/species/piglin.rs : après l'entrée `admire`, `Entry::new(&admire_seen::MODULE, 1, &admire_seen::Params { items: &["minecraft:gold_ingot"], ..admire_seen::Params::DEFAULT }),` + import ; et retirer de l'en-tête la mention « the look at the dropped item (AdmireSeen) ».
- src/species/mod.rs : ligne RESOLVED de minecraft:piglin, `("admire_seen", 1)` juste après `("admire", 1)`.
- tests/it/piglin_vs_mob.rs : un test différentiel.
Modèle :
- Mob : crates/horizon-client/src/mobs/piglin.rs : `Memories::admire_seen` (172-198), `AdmireSeen` (1734-1760), compteurs décrémentés à 1955-1962, ADMIRE = 119 (ligne 18).
- Forme : src/behaviours/admire.rs (modules à état + équipement), src/behaviours/go_to_wanted.rs (recherche d'objet : `wanted_near`).
Interfaces :
Params { items, duration: i32 (119), too_far_sqr: f64 (81.0), tired: i32 (200), walk_off: i32 (200), disabled: i32 (0) } avec DEFAULT. Composant d'état `SeenAdmire { until: u64, reach: Option<u32> }` (SparseSet, présent seulement pendant le but) ; ce qui doit survivre à l'arrêt (`walk_off` : échéance en ticks absolue, `Component` SparseSet) est posé par échéance sur l'horloge `now_of`, pas décrémenté par tick. Drapeaux MOVE | LOOK. can_use : un objet voulu (de `items`, ramassable, dans 14 blocs et visible) est vu, la main gauche est vide, pas de `walk_off` en cours. Tant que le but court : marcher vers l'objet (vitesse 1.0) quand la navigation est finie ; il s'arrête si l'objet est à plus de 9 blocs (81 au carré), ou après 200 ticks d'essais (alors walk_off 200 ticks), ou si la main gauche se remplit. Réveil par événement (objet apparu / main changée) via `arm` comme `admire`, jamais de scrutation chaque tick.
Tests : aucune commande à lancer (ne compile pas, ne lance pas cargo). Tests à ÉCRIRE :
Piglin et pièce d'or au sol à 5 blocs, main gauche vide : des deux côtés le piglin marche vers l'or dans les 5 premiers ticks et la distance à l'or diminue ; objet à 12 blocs : ni l'un ni l'autre ne bouge à cause de lui (arrêt à 9).
Fin :
Fichiers écrits, aucun nom d'espèce dans le module. Rapport : fichiers, doutes.
Interdits :
Compiler, cargo, toucher au Mob, à d'autres lignes de species/mod.rs, d'autres fichiers.
