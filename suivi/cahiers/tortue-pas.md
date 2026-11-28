# Tortue adulte : pas en trop (lot 10, suite du lot 4 de ecarts-lot9.md)

Contexte (mesure 86 et 87, 2 passes par côté, 5 tortues adultes avec IA, arène sèche fermée, paisible, 3000 ticks) :
- `entity.turtle.shamble` 1614 contre 1062 (+52 %), bébé 1074 contre 768 (+40 %).
- Distance horizontale parcourue en 3000 ticks, somme des 5 tortues, relevée tous les 20 ticks : nous 205 et 206, vanilla 152 et 144 (+39 %). Par tortue : 38 à 45 contre 21 à 35.
- Donc la CADENCE du pas n'est pas en cause en premier : c'est la MARCHE. Le reste (+52 % de pas contre +39 % de distance) est dans le bruit du relevé tous les 20 ticks.
- Vitesse de marche identique : relevé tous les 2 ticks, 0,28 à 0,29 bloc par 2 ticks quand elle avance, des deux côtés (0,14 par tick, soit `getSpeed`/2 puis min 0,06 de `TurtleMoveControl` : `turtle_control` de `locomotion/swim.rs` est conforme).
- Ce qui diffère : la part du temps en mouvement. Sur 600 ticks, nous 94 % et 100 % ; vanilla 77 % et 82 %. Nos tortues marchent par tronçons continus de 310 et 598 ticks avec des arrêts de 2 à 4 ticks ; vanilla alterne marches de 12 à 180 ticks et arrêts de 6 à 42 ticks. Il manque donc des arrêts entre deux promenades (ou une promenade n'est jamais finie).
- Écartés au jar et au code : vitesse (`MOVEMENT_SPEED` 0,25, `speedModifier` 1,0, mêmes valeurs) ; intervalle de la promenade (`TurtleRandomStrollGoal(1.0, 100)`, soit `RandomStrollGoal` 100 réduit à 50, attente moyenne ~100 ticks, `turtle_stroll.rs::equip` et `idle_wait` conformes à la lecture) ; `TurtleTravelGoal` (exige l'eau, `turtle_travel.rs::can_use` aussi) ; `TurtleGoHomeGoal` (1 chance sur 700 et seulement à plus de 64 blocs du gîte). La cause exacte n'est PAS trouvée à la ligne : elle est dans le temps passé à marcher, pas dans la vitesse ni le pas.

## Fichiers
- `crates/horizon-ecs-proto/src/behaviours/turtle_stroll.rs`, `behaviours/stroll.rs`, `cow/nav.rs` (attente, `Stroll.walking`, `stop`), `behaviours/turtle_go_to_water.rs`, `behaviours/turtle_go_home.rs` (lecture ; modification seulement là où la cause sera trouvée).
- Un test ajouté dans le fichier qui porte le correctif (module `tests` existant, jamais un test `it`).

## Modèle
1. Instrumenter d'abord, sans rien commiter : dans un test de `turtle_stroll.rs` (monde de `a_turtle_on_land_strolls_and_one_in_water_does_not`), poser une tortue adulte sur sol sec, faire 3000 ticks et relever pour chaque tick quel but tourne (`Goals` actifs) et si la tortue avance. Sortie attendue : la part du temps en mouvement doit être environ 80 % et les arrêts de 6 à 42 ticks, comme vanilla.
2. Hypothèses à départager dans cet ordre, chacune par un relevé :
   a. Après `stop` (`turtle_stroll.rs` ligne 107) `arm_now` rearme tout de suite, mais `Stroll.scheduled` peut rester vrai ou faux de façon que l'attente tirée soit sautée ou nulle : relever `wait` à chaque `arm_now` (attendu : 2 fois une géométrique de 50, moyenne ~100, jamais 0).
   b. `fired` (ligne 70) retourne vrai tout de suite si `ActiveUntil` ne bloque pas : vanilla refuse après `noActionTime >= 100` ; relever `ActiveUntil` et `now`.
   c. Un autre but pousse la tortue : `turtle_go_to_water` (priorité 3) ou `turtle_go_home` choisis sur terre sèche sans eau ni gîte lointain ; relever lequel est actif pendant les tronçons longs.
   d. Fin de chemin : la navigation n'est jamais `done` (vanilla `canContinueToUse` = `!navigation.isDone()`), la tortue repart vers le même point ; relever la distance au point visé à l'arrêt.
3. Le correctif est la ligne qui rend l'arrêt manquant. La vitesse et la cadence du pas ne se touchent pas.

## Interfaces
Aucune nouvelle. Les paramètres de `species/turtle.rs` restent ceux du jar (promenade 1,0 et 100).

## Tests
- Test de comportement (monde réel de `turtle_stroll.rs`) : sur 3000 ticks une tortue adulte sur terre ne marche pas plus de 90 % du temps et fait au moins 8 arrêts de 6 ticks ou plus.
- `cargo test -p horizon-ecs-proto turtle` vert, puis la passe complète.
- Mesure : `s_dist.py` (5 tortues, 3000 ticks, tous les 20 ticks) doit donner une distance totale entre 144 et 160 (vanilla 144 et 152) et `entity.turtle.shamble` adulte entre 1000 et 1130, bébé entre 730 et 810.

## Fin
Le banc de distance et de `shamble` rejoue vert (2 passes), la grille de `suivi/fidelite.md` passe la tortue au vert avec les chiffres.

## Interdits
Ne toucher aucun autre fichier (`LEVER :`). Aucune nouvelle dépendance. Aucun `target` laissé dans le worktree. Aucun test `it` de horizon-ecs-proto. Aucun nom d'espèce en dur dans un système. Aucun cache par mob d'un tick à l'autre. Commentaires 10 % au plus. Pas plus de 300 lignes ajoutées d'un coup. Ne pas changer la vitesse, le pas (0,15) ni l'intervalle de 100 : ils sont conformes.

## Montage du banc

Outils (hors dépôt, dans `$SP/mesure/comp/`, `SP=/tmp/claude-1000/-home-rsman-projects-horizon-server-tests/7c33b961-0885-4f39-9ae6-57a3086700bd/scratchpad`) : `drv3.py <binaire|vanilla> <scénario.py> <étiquette>` (variable `PORT` propre à chaque processus ; journaux `logs/<binaire>-<étiquette>.{drv,srv,err}`), `vh.py` (assistants `setup`, `c`), scénario `s_dist.py` (ci-dessous, gardé dans ce dossier). Les variables `SP=turtle` (exigée par `drv3.py`) et `PROBE` (relevé en plus tous les N relevés) sont lues par l'environnement.
- Binaire : copie de `wt/.tgt-integ/ecsdev/myriade-engine` (profil `ecsdev`, `CARGO_INCREMENTAL=0`) compilé au commit c47b022658 (copie nommée `cur87`, puis `cur88` ; mêmes résultats). Vanilla : `runtime/vanilla-26.3/server-26.3.jar` lancé par `drv3.py vanilla`.
- Lancement (2 passes, 4 processus à la fois) : `SP=turtle PORT=$((29500+off+p)) python3 drv3.py <cur87|vanilla> s_dist.py dd$p`, `off`=0 pour nous et 100 pour vanilla, p=1,2. Durée : environ 5 à 6 minutes.
- Arène (`setup(time_=6000)` de `vh.py`, puis `difficulty peaceful`) : `forceload` de x 400 à 440, z 0 à 40 ; sol de pierre à y=100 sur tout ce rectangle, air de y=101 à 115 ; PAS de mur autour des tortues : la boîte `fill 420 100 20 439 112 39 glass hollow` est posée à côté (x 420 à 439) et les tortues naissent dehors, en x 406 à 414, z 8. Pas d'eau, pas de sable, pas d'œuf : aucune eau à moins de 24 blocs, donc `TurtleGoToWaterGoal` ne trouve rien. Jour fixé (`time set 6000`, `advance_time false`), `spawn_mobs false`, pas de météo.
- Gîte : non posé par le banc. Le NBT lu sur les deux côtés (`/data get entity`, 100 ticks après l'invocation) donne `home_pos: [I; 0, 0, 0]` et `has_egg: 0b` : le gîte est le bloc (0, 0, 0), à plus de 400 blocs, des deux côtés (noms de champs du 26.3 : `home_pos`, `has_egg`, pas `HomePos` ni `HasEgg`). Pas de `NoAI` : IA active, `PersistenceRequired:1b`.
- Joueur : le bot `Bot0` en créatif, téléporté en (410, 103, 16), soit 6 blocs au sud-est du point de naissance (410, 101, 10) et à moins de 32 blocs de toutes les tortues pendant les 3000 ticks (pas de `noActionTime` accumulé). Il ne bouge pas.
- Invocation, 5 fois, i de 0 à 4 : `execute in minecraft:overworld run summon minecraft:turtle <406+2i> 101 8 {PersistenceRequired:1b,Tags:[s<i>]}` (adultes, sans `Age`, sans `NoAI`).
- Relevés : 150 fois, `wait_ticks n=20` puis, pour chaque i, `data get entity @e[tag=s<i>,limit=1] Pos` (soit 3000 ticks et 750 lignes `entity data: [x, y, z]` dans `logs/*.srv`) ; distance = somme des déplacements horizontaux entre relevés successifs. Banc fin : même scénario avec `range(300)` et `n=2` pour 2 tortues (relevés tous les 2 ticks).

Scénario `s_dist.py` en entier :

```python
exec(open("vh.py").read())
import os
# banc de distance de la tortue : 5 tortues adultes avec IA, arene seche fermee, 150 releves de 20 ticks
setup(time_=6000)
c("difficulty peaceful",0.1)
c("fill 420 100 20 439 112 39 glass hollow",0.5)
HOME=(410,101,10)
srv.cmd("execute as Bot0 in %s run tp @s %s %s %s"%(O,HOME[0],HOME[1]+2,HOME[2]+6),0.4); bot("wait_ticks",n=10)
for i in range(5): c("summon minecraft:turtle %d %d %d {PersistenceRequired:1b,Tags:[s%d]}"%(HOME[0]-4+2*i,HOME[1],HOME[2]-2,i),0.1)
for k in range(150):
    bot("wait_ticks",n=20)
    for i in range(5): srv.cmd("data get entity @e[tag=s%d,limit=1] Pos"%i,0.03)
    if os.environ.get("PROBE") and k%int(os.environ.get("PROBE"))==0:
        for f in ("HomePos","HasEgg","NoAI","TravelPos"): srv.cmd("data get entity @e[tag=s0,limit=1] %s"%f,0.03)
srv.cmd("say DISTEND",1.0)
```

Relevé de plus (binaire cur88, une passe par côté, `PROBE=1`, 5 tortues, 3000 ticks) :
- Lisible par `/data get entity` : `Pos`, `home_pos` (0, 0, 0 des deux côtés), `has_egg` 0b. `HomePos`, `HasEgg`, `NoAI`, `TravelPos` n'existent pas sous ces noms (« Found no elements »), et la cible de marche n'est pas dans le NBT.
- Pendant les tronçons longs, aucune tortue ne s'approche du gîte : déplacement net vers (0, 0) de -19 à +7 blocs sur 37 à 42 blocs marchés chez nous, -10 à +6 sur 27 à 37 chez vanilla, donc une errance au hasard des deux côtés. Pas d'eau dans l'arène à approcher.
- Mouvement : nos tortues avancent sur 126, 142, 149, 147, 146 des 149 relevés de 20 ticks, vanilla sur 106, 108, 98, 128, 133. L'écart de temps en marche se reproduit donc sur ce passage, sur le vrai serveur.
