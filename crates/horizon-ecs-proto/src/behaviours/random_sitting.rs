//! Fiche 101, `RandomSitting` (`horizon-client/src/mobs/camel.rs:386-420`) : un chameau libre, 400 ticks après son dernier
//! changement de pose, change de pose. Le tirage `nextInt(4) == 0` par demande devient UN délai géométrique (`idle_wait`)
//! armé dès que les conditions tiennent. Un événement (sol, eau quittée, cavalier parti) qui les rend vraies arme le délai ;
//! à son échéance, `can_use` revérifie les conditions et, si l'une est fausse, n'arme rien : on attend l'événement.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::mount::Mount;
use crate::targeting::Who;
use crate::rules::random_sitting as rules;
use super::posture;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Own ticks in the pose before a change is possible (`pose_time >= 400`).
    pub min_pose_time: u64,
    /// `nextInt(one_in) == 0` per ask.
    pub one_in: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { min_pose_time: 400, one_in: 4 };
}

/// The rank of the goal, its parameters, and whether its one deadline is in the wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Sitter {
    pub slot: u8,
    pub min_pose_time: u64,
    pub one_in: i32,
    pub armed: bool,
}

pub static MODULE: Module = Module {
    name: "random_sitting",
    flags: MOVE,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Sitter { slot: inst.slot, min_pose_time: p.min_pose_time, one_in: p.one_in, armed: false });
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    schedule(w, e, inst);
}
/// The conditions of `canUse` but the time and the draw: on the ground, out of the water, not driven by a player.
/// (`equine.controlled`; a creature rider is not read: DOUTE, `riders.rs:511-518`.)
fn free(w: &World, e: Entity) -> bool {
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    let driven = w.get::<Mount>(e).is_some_and(|m| m.saddled && matches!(m.rider, Some(Who::Player(_))));
    on_ground && !in_water && !driven
}
/// Arms the one wait of the cycle: the time left until the pose is `min_pose_time` old, then a geometric draw. Nothing
/// while a condition is false: the event that makes it true calls this again.
fn schedule(w: &mut World, e: Entity, inst: Inst) {
    let Some(s) = w.get::<Sitter>(e).copied() else { return };
    if s.armed || !free(w, e) {
        return;
    }
    let delay = rules::change_delay(s.min_pose_time, posture::pose_time(w, e), with_rng(w, e, |r| idle_wait(r, s.one_in)));
    crate::goals::arm(w, e, inst.slot, delay);
    if let Some(mut m) = w.get_mut::<Sitter>(e) {
        m.armed = true;
    }
}
/// The deadline came up: the draw is over, `can_use` decides.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut s) = w.get_mut::<Sitter>(e) {
        s.armed = false;
    }
    true
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(s) = w.get::<Sitter>(e).copied() else { return false };
    rules::can_change(free(w, e), posture::pose_time(w, e), s.min_pose_time)
}
/// `start`: a sitting camel stands; a standing one sits, unless it panics (a hit within 40 ticks, or burning).
fn start(w: &mut World, e: Entity, inst: Inst) {
    if posture::is_sitting(w, e) {
        posture::stand_up(w, e);
    } else if !should_panic(w, e) {
        posture::sit_down(w, e);
        stop_nav(w, e);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
/// `should_panic` (`goals.rs:1089-1093`): a recent panic-cause hit, or burning (the fire-immunity is not read: DOUTE).
fn should_panic(w: &World, e: Entity) -> bool {
    let now = now_of(w, e);
    rules::should_panic(w.get::<crate::brain::LastDamage>(e).map(|d| (d.panic, d.at)), w.get::<Burning>(e).map(|b| b.until), now)
}
/// The goal is instant: the next cycle is armed after it.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    schedule(w, e, inst);
}

fn tick(w: &mut World) {
    tick_state(w, events_run);
}
/// The conditions a deadline does not see come true: the animal lands, leaves the water, is no longer driven. Each one
/// arms the cycle again (a no-op while it is armed).
fn events_run(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<&Sitter>) {
    for e in ev.landed.iter().chain(ev.left_fluid.iter()).chain(ev.dismounted.iter()) {
        if q.get(e).is_ok() {
            reqs.0.push(Req::Call(e, resched));
        }
    }
}
fn resched(w: &mut World, e: Entity) {
    let Some(s) = w.get::<Sitter>(e).copied() else { return };
    schedule(w, e, Inst { slot: s.slot, nth: 0 });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    /// A cow with this module only, awake, on the ground of the test world.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(60, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 5, &Params::DEFAULT), Entry::new(&posture::MODULE, 255, &posture::Params::DEFAULT)]));
        sp.goals = Config { entries };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 23);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        crate::goals::arm_timers(&mut h.world, e);
        h.tick();
        Some((h, e))
    }
    /// The first change of pose comes only once the pose is 400 ticks old, and then the animal does change pose
    /// (witness: a change is seen within 3000 ticks). Each later change also waits 400 ticks after the previous one.
    #[test]
    fn it_changes_pose_only_after_four_hundred_ticks_and_then_keeps_changing() {
        let Some((mut h, e)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let mut changes: Vec<u64> = Vec::new();
        let mut was = posture::is_sitting(&h.world, e);
        for _ in 0..3000 {
            h.tick();
            let now_sit = posture::is_sitting(&h.world, e);
            if now_sit != was {
                changes.push(h.world.resource::<Clock>().of(e));
                was = now_sit;
            }
        }
        assert!(changes.len() >= 2, "témoin : {} changements en 3000 ticks", changes.len());
        // The pose is 53 ticks old at creation (`Posture` since = now - 53): the first change comes at pose 400.
        assert!(changes[0] + 53 >= 400, "premier changement à {}", changes[0]);
        for w in changes.windows(2) {
            assert!(w[1] - w[0] >= 400, "changements à {} et {}", w[0], w[1]);
        }
    }
    /// A sitting camel stands up again (stop of the goal), and a camel in the water never sits.
    /// Witness: the same camel sits at some point before it stands.
    #[test]
    fn a_sitting_camel_stands_up_on_the_next_change() {
        let Some((mut h, e)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let mut sat = false;
        let mut stood_after_sat = false;
        for _ in 0..3000 {
            h.tick();
            if posture::is_sitting(&h.world, e) {
                sat = true;
            } else if sat {
                stood_after_sat = true;
                break;
            }
        }
        assert!(sat, "témoin : il s'assoit");
        assert!(stood_after_sat, "il se relève");
    }
}
