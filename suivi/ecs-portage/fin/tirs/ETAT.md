# Lot tirs : état (2026-11-20)

Base ecs/normes, branche ecs/fin-tirs. Cible .tgt-fin-tirs supprimée en fin de lot.

## Constat
Presque tous les buts du lot sont DÉJÀ portés sous un autre nom (la table ALIAS d'avancement.py est incomplète) : le travail réel est au niveau des instances (lignes de configuration par espèce), pas des modules.
Copie de travail de la table d'alias : scratchpad de la session (`tirs/av.py`, `cmp.py`, `AV.md`), non commitée.

## Fait (lu au jar, mesuré en test)
- `spell_use` : `Spell::Wololo` (evoker 6) et `Spell::Blindness` (illusioner 5) ; `effects::Kind::Blindness` ; `look_at_player` espèce `"*"` (classe Mob, priorité 10).
- evoker : wololo 6, pathfind_to_raid 3 (jar), look mobs 10, golem must_see=false et mémoire 60 par défaut (jar).
- illusioner : 5 lignes de raider, blindness 5, look mobs 10, cibles propres (mémoire 300, villageois et golem sans vue) au lieu de celles de l'evoker.
- pillager : évitement du creaking à 1, look mobs 10, pathfind_to_raid 4 -> 3 (jar), `raider_open_door@1` retiré (absent du jar et du Mob).
- parched : flee_sun 3 + avoid_entity loup 3. drowned : plus de `move_through_village` (Drowned.addBehaviourGoals n'appelle pas celui de Zombie).
- Test `tests/it/spells_vs_mob.rs` : wololo Mob 38 / ECS 40 (écart de 2 : le premier `can_use` est armé à 2 ticks) ; blindness 59 / 59, rien en facile.

## À trancher / écarts
- Blindness : le Mob lit la difficulté locale effective > 2 ; l'ECS n'a que l'id de difficulté : `>= 3` (difficile). Normal avec longue durée d'habitation divergerait. Supposé.
- Wololo : sans cible, la question est reposée toutes les 2 ticks (comme le sélecteur du Mob) : un scan des moutons par evoker oisif ; coût à valider.
- Visibilité (échelle de portée) du wololo non portée ; sons non portés.
- hurt_by : liste des Raiders ignorés (jar : `HurtByTargetGoal(this, Raider.class)`) non portée dans le module (evoker, illusioner, pillager, vindicator).
- Vindicator (hors lot) : pathfind_to_raid à 6 dans l'ECS, 3 au jar.
- Mob non conforme au jar : golem de l'evoker (Mob must_see=true, mémoire 300 ; jar : false, 60). L'ECS suit le jar ; test strict contre le Mob non écrit.
- Tests rouges hors lot : hoglin_vs_mob (connu), parallel_equals_sequential x4 (binaire scale_bench absent).
## Non fait
Rien de plus de l'inventaire : blaze, ghast, guardian, shulker, llama, spider, polar_bear, skeleton, wither, etc. étaient déjà complets aux instances comparées ; PiglinSpear = module `spear_use` (déjà générique), non branché sur piglin.

## Clôture (non compilé)
Alias copiés dans `alias.md`, écarts dans `suivi/ecs-ecarts.md`, `RAIDERS` porté (evoker, illusioner, pillager), un test ours polaire ajouté. À compiler et lancer après fusion : `variants_vs_mob`, `spells_vs_mob`, `no_species_names`.
