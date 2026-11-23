//! `MoveTowardsTargetGoal` (fiche 14, iron golem) : quand la cible est à moins de `within` blocs, l'animal marche vers un
//! point tiré à 16 blocs au plus et 7 en hauteur, dans un quart de tour de la position de la cible
//! (`DefaultRandomPos.getPosTowards`). Le but finit quand son chemin finit (il est redemandé aussitôt : `retry_on_end`),
//! quand la cible est perdue, morte ou au-delà de `within`. Réveil : une cible acquise (la fonction de `TargetWatchers`).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::rules::move_towards_target::{beyond_within, within_range};
use crate::spots::{self, Pick};
use crate::targeting::{self, Target, TargetWatchers, Who};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// La vitesse de la marche (0,9 pour le golem de fer) et la distance à laquelle une cible est suivie (32 blocs).
    pub speed: f64,
    pub within: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 0.9, within: 32.0 };
}
/// L'état : le rang de l'entrée, ses paramètres, et le point que `canUse` a tiré pour `start`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Approach {
    slot: u8,
    speed: f64,
    within: f64,
    wanted: [f64; 3],
}
/// Le but tourne (son système le visite).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct ApproachingTarget;

pub static MODULE: Module = Module {
    name: "move_towards_target",
    flags: MOVE,
    ends_with_path: true,
    retry_on_end: true,
    resources,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Goals, Cap::Targets, Cap::Locomotion],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
fn on_target(w: &mut World, e: Entity) {
    if let Some(a) = w.get::<Approach>(e) {
        let slot = a.slot;
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Approach { slot: inst.slot, speed: p.speed, within: p.within, wanted: [0.0; 3] });
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, approach_run);
}
/// La cible d'un créature morte n'est plus vivante (un joueur mort n'est pas lu par `targeting::info`).
fn alive(hp: &Query<&Health>, who: Who) -> bool {
    match who {
        Who::Entity(c) => hp.get(c).map_or(true, |h| h.0 > 0.0),
        Who::Player(_) => true,
    }
}
/// `canUse` : une cible à `within` blocs au plus, et un point tiré (absent : non).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(who) = w.get::<Target>(e).map(|t| t.who) else { return false };
    let Some(i) = targeting::info(w, who) else { return false };
    let (Some(p), Some(a)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Approach>(e).copied()) else { return false };
    if beyond_within(d2(p, i.pos), a.within) {
        return false;
    }
    let to = [i.pos.x, i.pos.y, i.pos.z];
    let Some(spot) = spots::pick_for(w, e, Pick::DefaultTowards { h: 16, v: 7, to, max_rad: std::f64::consts::FRAC_PI_2 }) else {
        // Le tirage a échoué (le `Mob` redemande aux ticks suivants tant que la cible est à portée) : on réessaie.
        crate::goals::arm(w, e, a.slot, 2);
        return false;
    };
    if let Some(mut a) = w.get_mut::<Approach>(e) {
        a.wanted = spot;
    }
    true
}
/// `start` : `PathNavigation.moveTo(point, vitesse)`, portée 1.
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(a) = w.get::<Approach>(e).copied() else { return };
    go_to_at(w, e, a.wanted, a.speed, 1);
    w.entity_mut(e).insert(ApproachingTarget);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<ApproachingTarget>();
    stop_nav(w, e);
}
/// `canContinueToUse` (la fin du chemin passe par `ends_with_path`) : la cible vit et reste à `within` blocs.
fn approach_run(tg: targeting::Targets, hp: Query<&Health>, q: Query<(Entity, &Pos, &Approach, Option<&Target>), With<ApproachingTarget>>, mut reqs: ResMut<Requests>) {
    for (e, pos, a, t) in &q {
        let keep = t.is_some_and(|t| tg.info(t.who).is_some_and(|i| alive(&hp, t.who) && within_range(d2(pos.0, i.pos), a.within)));
        if !keep {
            reqs.0.push(Req::End(e, a.slot));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, Pos, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::targeting::{Target, clear_target, set_target};
    use horizon_core::geometry::Vec3;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 2, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    /// Un zombie avec ce seul but, et une vache (sans buts) comme cible, à `dist` blocs sur x.
    fn world(dist: f64) -> Option<(Herd, Entity, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        cow.goals = Config { entries: &[] };
        let golem = spawn_species(&mut h.world, &sp, 0.5, 0.5, 41);
        let prey = spawn_species(&mut h.world, &cow, 0.5 + dist, 0.5, 42);
        for e in [golem, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.tick();
        let y = h.world.get::<Pos>(prey).unwrap().0.y;
        set_target(&mut h.world, golem, Target::new(Who::Entity(prey), 200, u8::MAX));
        h.teleport(prey, Vec3::new(0.5 + dist, y, 0.5));
        Some((h, golem, prey))
    }

    /// Départ avec la cible à 10 blocs (un point est tiré), pas de départ à 40 blocs (au-delà de 32).
    #[test]
    fn it_walks_toward_a_target_within_32_blocks_and_not_beyond() {
        let Some((mut h, golem, _)) = world(10.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        ticks(&mut h, 30);
        assert!(running(&h, golem), "départ à 10 blocs");
        let Some((mut far, golem_far, _)) = world(40.0) else { return };
        ticks(&mut far, 10);
        assert!(!running(&far, golem_far), "pas de départ à 40 blocs");
    }

    /// Arrêt : la cible perdue arrête le but qui marchait ; la cible à 40 blocs aussi.
    #[test]
    fn it_stops_when_the_target_is_lost_or_beyond_32_blocks() {
        let Some((mut h, golem, prey)) = world(10.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        ticks(&mut h, 30);
        assert!(running(&h, golem), "témoin : le but marche");
        let y = h.world.get::<Pos>(prey).unwrap().0.y;
        h.teleport(prey, Vec3::new(40.5, y, 0.5));
        ticks(&mut h, 30);
        assert!(!running(&h, golem), "arrêt quand la cible passe au-delà de 32 blocs");
        // Revenue dans la portée, la cible n'est pas un événement : elle est de nouveau acquise.
        h.teleport(prey, Vec3::new(10.5, y, 0.5));
        set_target(&mut h.world, golem, Target::new(Who::Entity(prey), 200, u8::MAX));
        ticks(&mut h, 30);
        assert!(running(&h, golem), "témoin : un nouveau départ à 10 blocs");
        clear_target(&mut h.world, golem);
        ticks(&mut h, 6);
        assert!(!running(&h, golem), "arrêt quand la cible est perdue");
    }

    /// Le point tiré pour une cible à 8 blocs : à 16 blocs au plus et 7 en hauteur, dans un quart de tour de la cible (origine : 16, 7, 90 degrés).
    #[test]
    fn it_draws_a_spot_within_16_blocks_and_7_high_toward_the_target() {
        let Some((mut h, golem, _)) = world(8.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let start = h.world.get::<Pos>(golem).unwrap().0;
        ticks(&mut h, 30);
        assert!(running(&h, golem), "témoin : le but marche");
        let wanted = h.world.get::<Approach>(golem).unwrap().wanted;
        let (dx, dz) = (wanted[0] - start.x, wanted[2] - start.z);
        assert!((dx * dx + dz * dz).sqrt() <= 16.0, "à 16 blocs au plus");
        assert!((wanted[1] - start.y).abs() <= 7.0, "à 7 blocs en hauteur au plus");
        assert!(dx * (8.5 - start.x) + dz * (0.5 - start.z) >= 0.0, "dans un quart de tour de la cible");
    }
}
