//! `CubeMobAttackGoal` (`mobs/cube.rs:415-446`, fiche 78) : un cube (slime, cube de magma) qui a une cible se tourne vers elle
//! et fixe la direction de son prochain bond, pendant `reduced(300)` ticks (150) ; le but est redemandé aussitôt après sa fin tant
//! que la cible tient (`retry_on_end`). Pas de tirage. Le but tourne à chaque tick : ses ticks sont appelés depuis `arbitrate`
//! (`tick_all`, modèle `flier_goals`), sur les entités qui portent `Aiming` ; sans porteur, rien ne coûte.
//! NON PORTÉ : l'attaque par contact (`playerTouch`, dégâts du bond) : absente du cadre. La tête (tangage de `turn_to`) n'est
//! pas tournée. DOUTE : la redraw de direction de `hop::due` n'est pas suspendue pendant la chasse (le `Mob` ne tire la
//! direction aléatoire que sans cible : `CubeRandomDirection.can_use`).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::locomotion::hop::Hop;
use crate::size::Size;
use crate::targeting::{self, Target, TargetWatchers, Who};
use horizon_core::geometry::{trig, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `CubeMobAttackGoal.start`: `reduced(tired)` ticks of running (300).
    pub tired: i32,
    /// `CubeMob.canDealDamage` for a species that always bites (`MagmaCube`): the bite is not cut by the size.
    pub bites_tiny: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { tired: 300, bites_tiny: false };
}
/// The parameters the module keeps on the animal (set at creation).
#[derive(Component, Clone, Copy, Debug)]
pub struct Attack {
    tired: i32,
    bites_tiny: bool,
}
/// The goal runs: its countdown (ticks left before it ends).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Aiming {
    left: i32,
}

pub static MODULE: Module = Module {
    name: "cube_attack",
    tick: Some(tick),
    flags: LOOK,
    resources,
    equip,
    can_use,
    start,
    stop,
    retry_on_end: true,
    requires: &[Cap::Locomotion, Cap::Targets, Cap::Goals],
    ..Module::DEFAULT
};

fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
/// A target gained by an animal that has this module wakes the goal (the selector asks on its phase).
fn on_target(w: &mut World, e: Entity) {
    if w.get::<Attack>(e).is_some() {
        wake(w, e, &MODULE);
    }
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Attack { tired: p.tired, bites_tiny: p.bites_tiny });
}
/// `reducedTickDelay`: half, rounded up.
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
/// The target the goal follows: alive and not exempt (creative, spectator, dead), as `CubeMobAttackGoal.canUse`.
fn target_of(w: &World, e: Entity) -> Option<(Who, targeting::Info)> {
    let who = w.get::<Target>(e)?.who;
    targeting::info(w, who).filter(|i| !i.exempt).map(|i| (who, i))
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    target_of(w, e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let left = w.get::<Attack>(e).map_or(0, |a| reduced(a.tired));
    w.entity_mut(e).insert(Aiming { left });
    // A started goal ticks in the tick it starts (GUIDE 9.1): the same step as the next ticks.
    step(w, e);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Aiming>();
}

/// `canContinueToUse` then `tick`: a target that is gone ends the goal; the countdown ends it at zero; otherwise turn and set the
/// direction of the next bound.
fn step(w: &mut World, e: Entity) {
    let Some((_, t)) = target_of(w, e) else {
        return end(w, e, &MODULE);
    };
    let Some(left) = w.get_mut::<Aiming>(e).map(|mut a| {
        a.left -= 1;
        a.left
    }) else {
        return;
    };
    if left <= 0 {
        return end(w, e, &MODULE);
    }
    aim(w, e, t.pos);
}
/// `Mob.turn_to(target, 10, 10)` for the body yaw (the head pitch is not ported), then `set_direction(yaw, can_deal_damage)`: the
/// yaw is the direction of the next bound (`Hop.dir`), `aggressive` the bite flag.
fn aim(w: &mut World, e: Entity, at: Vec3) {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let want = trig::atan2_degrees(at.z - p.z, at.x - p.x) - 90.0;
    let Some(yaw) = w.get_mut::<Yaw>(e).map(|mut y| {
        y.0 = crate::cow::rotlerp(y.0, want, 10.0);
        y.0
    }) else {
        return;
    };
    let aggressive = w.get::<Size>(e).is_some_and(|s| s.0 > 1) || w.get::<Attack>(e).is_some_and(|a| a.bites_tiny);
    if let Some(mut h) = w.get_mut::<Hop>(e) {
        h.dir = yaw;
        h.aggressive = aggressive;
    }
}

/// The ticks of the goal, run by `arbitrate` (`Module::tick`): one call, skipped while no animal carries the goal.
fn tick(w: &mut World) {
    static ID: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let id = *ID.get_or_init(|| crate::species::registry().module_id(&MODULE));
    if !id.is_some_and(|i| w.resource::<Carriers>().0.get(i).is_some_and(|n| *n > 0)) {
        return;
    }
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Aiming>>().iter(w).collect();
    for e in es {
        step(w, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, Pos, SpeciesTable, spawn_species};
    use crate::size::spawn_sized;
    use crate::species::Config;
    use crate::targeting::set_target;

    fn config(p: &'static Params) -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 1, p)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// The angle difference in degrees, modulo 360.
    fn angle_gap(a: f32, b: f32) -> f32 {
        ((a - b) % 360.0 + 540.0) % 360.0 - 180.0
    }
    /// A cube of `name` (its size when given) and a cow 4 blocks away on x, both landed; no target yet.
    fn world(name: &str, size: Option<i32>, p: &'static Params) -> Option<(Herd, Entity, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == name).unwrap();
        sp.goals = config(p);
        let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        cow.goals = Config { entries: &[] };
        let cube = match size {
            Some(s) => spawn_sized(&mut h.world, &sp, 0.5, 0.5, 61, s),
            None => spawn_species(&mut h.world, &sp, 0.5, 0.5, 61),
        };
        let prey = spawn_species(&mut h.world, &cow, 4.5, 0.5, 62);
        for e in [cube, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        for _ in 0..5 {
            h.tick();
        }
        Some((h, cube, prey))
    }
    fn acquire(h: &mut Herd, cube: Entity, prey: Entity) -> bool {
        set_target(&mut h.world, cube, Target::new(Who::Entity(prey), 200, u8::MAX));
        let mut started = false;
        for _ in 0..30 {
            h.tick();
            started |= running(h, cube);
        }
        started
    }

    /// Sans cible rien ne démarre ; la cible acquise fait démarrer le but (témoin) ; le cube se tourne vers la cible : son bond
    /// (`Hop.dir`) et sa direction (`Yaw`) convergent vers l'angle réel, mesuré avec `atan2` (et non avec le code testé).
    #[test]
    fn it_starts_on_a_target_and_turns_its_hop_toward_it() {
        let Some((mut h, cube, prey)) = world("minecraft:slime", Some(2), &Params::DEFAULT) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        for _ in 0..30 {
            h.tick();
        }
        assert!(!running(&h, cube), "sans cible, le but ne démarre pas");
        assert!(acquire(&mut h, cube, prey), "témoin : une cible acquise fait démarrer le but");
        for _ in 0..30 {
        }
        let p = h.world.get::<Pos>(cube).unwrap().0;
        let t = h.world.get::<Pos>(prey).unwrap().0;
        let want = ((t.z - p.z).atan2(t.x - p.x).to_degrees() - 90.0) as f32;
        let yaw = h.world.get::<Yaw>(cube).unwrap().0;
        let dir = h.world.get::<Hop>(cube).unwrap().dir;
        assert!(angle_gap(yaw, want).abs() < 0.01, "la direction du corps vise la cible : {yaw} contre {want}");
        assert!(angle_gap(dir, want).abs() < 0.01, "le prochain bond part vers la cible : {dir} contre {want}");
    }

    /// Le bond mord (`aggressive`) pour un cube qui n'est pas petit, et pour l'espèce qui mord toujours ; pas pour un petit slime.
    #[test]
    fn the_bite_follows_the_size_and_the_species_rule() {
        let cases: [(&str, Option<i32>, &'static Params, bool); 3] = [
            ("minecraft:slime", Some(1), &Params::DEFAULT, false),
            ("minecraft:slime", Some(2), &Params::DEFAULT, true),
            ("minecraft:magma_cube", Some(1), &Params { tired: 300, bites_tiny: true }, true),
        ];
        for (name, size, p, want) in cases {
            let Some((mut h, cube, prey)) = world(name, size, p) else {
                eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
                return;
            };
            assert!(acquire(&mut h, cube, prey), "témoin : {name} démarre");
            assert_eq!(h.world.get::<Hop>(cube).unwrap().aggressive, want, "{name} de taille {size:?}");
        }
    }

    /// Le compte à rebours : `reduced(300)` = 150 ticks de but. Le démarrage compte le premier ; la fin est demandée au 150e
    /// tick (témoin : le but tourne avant).
    #[test]
    fn it_runs_for_reduced_tired_ticks_then_asks_to_end() {
        let Some((mut h, cube, prey)) = world("minecraft:slime", Some(2), &Params::DEFAULT) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        assert!(acquire(&mut h, cube, prey), "témoin : le but démarre");
        let ends = |h: &Herd| h.world.resource::<Requests>().0.iter().any(|r| matches!(r, Req::End(x, _) if *x == cube));
        // The framework's own ticks (`Module::tick`, run by `arbitrate`) counted the goal down while it was acquired: the end comes when
        // `left` is spent (150 ticks in all: `reduced(300)`, the start counting as the first).
        let left = h.world.get::<Aiming>(cube).unwrap().left;
        assert!((1..=150).contains(&left), "témoin : le but a un compte à rebours ({left})");
        for j in 1..=left + 1 {
            tick(&mut h.world);
            assert_eq!(ends(&h), j >= left, "fin demandée au tick {j} (attendue au {left}e appel)");
        }
    }

    /// Une cible perdue arrête le but au tick suivant, et le but ne redémarre pas sans cible (témoin : il tournait).
    #[test]
    fn it_stops_when_its_target_is_lost() {
        let Some((mut h, cube, prey)) = world("minecraft:slime", Some(2), &Params::DEFAULT) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        assert!(acquire(&mut h, cube, prey), "témoin : le but tourne");
        h.world.entity_mut(cube).remove::<Target>();
        h.tick();
        assert!(!running(&h, cube), "sans cible, le but s'arrête");
        h.tick();
        assert!(!running(&h, cube), "et ne redémarre pas sans cible");
    }
}
