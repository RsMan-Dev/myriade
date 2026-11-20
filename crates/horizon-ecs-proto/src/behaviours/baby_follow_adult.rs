//! `BabyFollowAdult` (fiche 18) : un bébé sans but de marche suit l'adulte le plus proche de son espèce qu'il voit, à
//! 5 à 16 blocs (17 exclu), à vitesse 1,25, jusqu'à 4 blocs. Le bébé demande le but à chaque tour de roue armé à +2 tant
//! qu'il n'a pas démarré (comme le `canUse` du `Mob`, une fois sur deux) ; le but tient MOVE et LOOK.
use super::armadillo_state::{self as st, IDLE};
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::targeting::Eye;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Distance minimale (blocs) pour démarrer ; la marche s'arrête à `min - 1`.
    pub min: f64,
    /// Distance maximale (blocs) : le départ est refusé à `max + 1` ou plus.
    pub max: f64,
    pub speed: f64,
    /// La portée de recherche de l'adulte (sa boîte élargie de cette valeur).
    pub range: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { min: 5.0, max: 16.0, speed: 1.25, range: 16.0 };
}

/// Les données du but sur l'animal : ses paramètres, son rang, l'adulte choisi au départ, la demande armée dans la roue,
/// et « départ fait, le chemin reste à demander » (le chemin est demandé par le système, qui a `Commands`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Follow {
    pub params: Params,
    pub slot: u8,
    pub adult: Option<Entity>,
    pub armed: bool,
    pub fresh: bool,
}
/// Le but tourne.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct FollowsAdult;

pub static MODULE: Module = Module {
    name: "baby_follow_adult",
    flags: MOVE | LOOK,
    ends_with_path: true,
    arm,
    fired,
    can_use,
    start,
    stop,
    equip,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let params = *params::<Params>(p);
    w.entity_mut(e).insert(Follow { params, slot: inst.slot, adult: None, armed: false, fresh: false });
    st::ensure(w, e);
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(follow_run.in_set(slot));
}
fn is_baby(w: &World, e: Entity) -> bool {
    w.get::<GrowAt>(e).is_some()
}
/// Une demande du but à +2 : seulement pour un bébé qui n'a pas déjà sa demande dans la roue.
fn rearm(w: &mut World, e: Entity) {
    rearm_in(w, e, 2);
}
/// The ask of a baby that has no adult to follow: the adult comes into range by a column change (the baby's own move wakes it, in
/// `follow_run`), and a slow poll (`WAIT` ticks) covers the crossing of the range inside one column, instead of an ask every 2 ticks.
const WAIT: i32 = 20;
fn rearm_in(w: &mut World, e: Entity, ticks: i32) {
    let Some(f) = w.get::<Follow>(e).copied() else { return };
    if f.armed || !is_baby(w, e) {
        return;
    }
    if let Some(mut g) = w.get_mut::<Follow>(e) {
        g.armed = true;
    }
    crate::goals::arm(w, e, f.slot, ticks);
}
/// Au tour de l'entité : un bébé demande le but.
fn arm(w: &mut World, e: Entity, _: Inst) {
    rearm(w, e);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut f) = w.get_mut::<Follow>(e) {
        f.armed = false;
    }
    true
}
fn alive(w: &World, c: Entity) -> bool {
    w.get::<Health>(c).is_none_or(|h| h.0 > 0.0)
}
/// `nearest_adult` du `Mob` : l'adulte le plus proche (de même espèce, vivant, non bébé) dont la boîte rencontre la boîte du
/// bébé élargie de `range`, et qu'il voit (ligne de vue, comme `Mob.sees`).
fn nearest_adult(w: &World, e: Entity, p: Params) -> Option<(Entity, f64)> {
    let pos = w.get::<Pos>(e)?.0;
    let area = w.get::<Bounds>(e)?.0.inflate(Vec3::new(p.range, p.range, p.range));
    let species = w.get::<SpeciesId>(e)?.0;
    let eye = w.get::<Eye>(e)?.0;
    let from = Vec3::new(pos.x, pos.y + eye, pos.z);
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, p.range, |c| near.push(c));
    let mw = w.resource::<Level>().mw.0;
    let mut best: Option<(Entity, f64)> = None;
    for c in near {
        if c == e || is_baby(w, c) || w.get::<SpeciesId>(c).map(|s| s.0) != Some(species) || !alive(w, c) {
            continue;
        }
        let (Some(cp), Some(b), Some(ce)) = (w.get::<Pos>(c), w.get::<Bounds>(c), w.get::<Eye>(c)) else { continue };
        if !b.0.intersects(area) {
            continue;
        }
        let d = d2(pos, cp.0);
        if best.is_some_and(|x| d >= x.1) {
            continue;
        }
        if mw.line_of_sight_for(Some(pos.y), from, Vec3::new(cp.0.x, cp.0.y + ce.0, cp.0.z)) {
            best = Some((c, d));
        }
    }
    best
}
/// `canUse` : un bébé, l'état IDLE (sinon le but attend ses drapeaux : c'est la fin du but qui le réveille), un adulte
/// dans [min, max + 1). Sans adulte, la demande est reprise à +2 (la condition peut changer sans événement).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if !is_baby(w, e) || st::of(w, e) != IDLE {
        return false;
    }
    // `canUse` of the `Mob` needs the navigation done (`armadillo.rs:158`): a baby that walks a path (a stroll) goes on.
    if w.get::<Following>(e).is_some() || w.get::<NeedsPath>(e).is_some() {
        rearm(w, e);
        return false;
    }
    let Some(f) = w.get::<Follow>(e).copied() else { return false };
    let p = f.params;
    match nearest_adult(w, e, p).filter(|(_, d)| *d >= p.min * p.min && *d < (p.max + 1.0) * (p.max + 1.0)) {
        Some((a, _)) => {
            if let Some(mut g) = w.get_mut::<Follow>(e) {
                g.adult = Some(a);
            }
            true
        }
        None => {
            rearm_in(w, e, WAIT);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(FollowsAdult);
    // `nav_move_to_entity(adult, FOLLOW_MIN - 1, speed)` at the start (the `Mob` asks its path in `start`): the path ends 4
    // blocks from the adult (`go_to_entity` with its reach).
    if let Some(f) = w.get::<Follow>(e).copied()
        && let Some(a) = f.adult
    {
        crate::goals::go_to_entity(w, e, a, f.params.speed, f.params.min as i32 - 1);
    }
}
/// `stop` : arrête la marche, oublie l'adulte, rend le regard, et redemande le but à +2 si le bébé l'attend encore.
fn stop(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
    w.entity_mut(e).remove::<(FollowsAdult, Looking)>();
    if let Some(mut f) = w.get_mut::<Follow>(e) {
        f.adult = None;
    }
    rearm(w, e);
}
/// `canContinueToUse` et `tick`, tant que le but tourne : le bébé est toujours bébé, l'adulte existe et est à plus de
/// `min - 1` blocs (le carré de la distance > (min - 1)^2) ; sinon le but finit. Le chemin est demandé une fois au
/// départ (`fresh`), vers la position de l'adulte à ce moment-là, et le bébé regarde l'adulte à chaque tick.
fn follow_run(
    moved: Res<CellMoves>,
    waiting: Query<&Follow, (With<GrowAt>, Without<FollowsAdult>)>,
    mut q: Query<(Entity, &Pos, &SpeciesId, &mut Follow, &mut Look, Has<Looking>, Has<GrowAt>), With<FollowsAdult>>,
    others: Query<(&Pos, &Eye, Has<GrowAt>, Option<&Health>, &SpeciesId)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    // A baby that changed column asks again at once: its surroundings changed (the adult may be in range now).
    for &m in &moved.0 {
        if let Ok(f) = waiting.get(m) {
            reqs.0.push(Req::Start(m, f.slot));
        }
    }
    for (e, pos, sp, mut f, mut look, looking, baby) in &mut q {
        let adult = f.adult.and_then(|a| others.get(a).ok().map(|(p, ce, g, h, s)| (p, ce, g, h, s, a)));
        let stays = baby
            && adult.is_some_and(|(ap, _, grown, h, s, _)| {
                !grown && h.is_none_or(|h| h.0 > 0.0) && s.0 == sp.0 && d2(pos.0, ap.0) > (f.params.min - 1.0).powi(2)
            });
        let Some((ap, ce, _, _, _, _)) = adult.filter(|_| stays) else {
            reqs.0.push(Req::End(e, f.slot));
            continue;
        };
        aim(&mut cmd, e, &mut look, looking, [ap.0.x, ap.0.y + ce.0, ap.0.z], None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::armadillo_state::{SCARED, ROLLING};
    use super::*;
    use crate::cow::{ActiveUntil, Herd, Pos, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::behaviours::rolled;
    use horizon_core::geometry::Vec3;

    /// Le suivi (priorité 4) et le but d'état (priorité 2), comme dans la configuration de l'animal à carapace.
    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([
            Entry::new(&rolled::MODULE, 2, &rolled::Params::DEFAULT),
            Entry::new(&MODULE, 4, &Params::DEFAULT),
        ]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<crate::goals::GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    fn dist(a: Vec3, b: Vec3) -> f64 {
        ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
    }
    /// Un bébé en (0.5, 0.5) et un adulte à `dx` blocs de lui, sur la même rangée.
    fn world(dx: f64) -> Option<(Herd, Entity, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = config();
        let baby = spawn_species(&mut h.world, &sp, 0.5, 0.5, 21);
        let adult = spawn_species(&mut h.world, &sp, 0.5 + dx, 0.5, 22);
        for e in [baby, adult] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.tick();
        let y = h.world.get::<Pos>(baby).unwrap().0.y;
        h.teleport(adult, Vec3::new(0.5 + dx, y, 0.5));
        h.make_baby(baby, 3000);
        h.world.resource_mut::<crate::goals::Entered>().0.push(baby);
        Some((h, baby, adult))
    }
    fn at(h: &Herd, e: Entity) -> Vec3 {
        h.world.get::<Pos>(e).unwrap().0
    }

    /// Départ seulement dans [5, 17) : à 3 blocs et à 20 blocs le bébé reste à l'arrêt ; à 7 blocs il part (témoin).
    #[test]
    fn a_baby_starts_only_for_an_adult_five_to_sixteen_blocks_away() {
        let Some((mut h, baby, adult)) = world(3.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        ticks(&mut h, 10);
        assert!(!running(&h, baby), "pas de départ à 3 blocs");
        h.teleport(adult, Vec3::new(20.5, at(&h, adult).y, 0.5));
        ticks(&mut h, 10);
        assert!(!running(&h, baby), "pas de départ à 20 blocs");
        h.teleport(adult, Vec3::new(7.5, at(&h, adult).y, 0.5));
        ticks(&mut h, 10);
        assert!(running(&h, baby), "départ à 7 blocs");
    }

    /// Poursuite puis arrêt : le bébé s'approche de l'adulte, et le but finit quand il est à 4 blocs ou moins.
    #[test]
    fn the_baby_walks_to_its_adult_and_stops_within_four_blocks() {
        let Some((mut h, baby, adult)) = world(7.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let start_d = dist(at(&h, baby), at(&h, adult));
        let mut started = false;
        for _ in 0..200 {
            h.tick();
            started |= running(&h, baby);
        }
        assert!(started, "le but a démarré");
        let end_d = dist(at(&h, baby), at(&h, adult));
        // The `Mob` stops at 4,44 blocks (path reach 4, measured in tests/lot1_vs_mob.rs).
        assert!(end_d < start_d && end_d <= 4.5, "le bébé s'est approché : {start_d} -> {end_d}");
        assert!(!running(&h, baby), "le but s'arrête à 4 blocs");
    }

    /// Un état de boule (le but de priorité 2 tient les drapeaux) : le suivi ne tourne pas ; au retour à IDLE il démarre.
    /// `canUse` of the `Mob` needs the navigation done: a baby that follows a path (a stroll) does not start; once the path
    /// is over it does (the witness: the same baby without the path starts).
    #[test]
    fn a_baby_on_a_path_does_not_start_until_the_path_is_over() {
        let Some((mut h, baby, _adult)) = world(7.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let inst = Inst { slot: 1, nth: 0 };
        // `can_use` asked at once (a tick would take the marker of a path that does not exist off).
        h.world.entity_mut(baby).insert(Following);
        assert!(!can_use(&mut h.world, baby, inst), "a baby that walks a path does not follow");
        h.world.entity_mut(baby).remove::<Following>();
        assert!(can_use(&mut h.world, baby, inst), "the witness: without the path it follows");
    }

    #[test]
    fn a_scared_baby_does_not_follow_until_its_state_is_idle() {
        let Some((mut h, baby, _adult)) = world(7.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        rolled::set_state(&mut h.world, baby, SCARED);
        ticks(&mut h, 10);
        assert!(!running(&h, baby), "pas de suivi en boule");
        rolled::set_state(&mut h.world, baby, ROLLING);
        ticks(&mut h, 4);
        assert!(!running(&h, baby));
        rolled::set_state(&mut h.world, baby, IDLE);
        ticks(&mut h, 10);
        assert!(running(&h, baby), "le suivi démarre à l'état IDLE");
    }
}
