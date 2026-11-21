//! `MeleeAttackGoal`: chase the target the target goal picked and strike it. `canUse` runs on a 20-tick grid, armed in
//! the wheel; the goal ticks (look, path every 4 to 10 ticks, strike) only while it runs.
use super::*;
use crate::rules::melee::off_grid;
use crate::targeting::{self, Target};
use crate::{cow::*, goals::*, hostile};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed modifier of the chase.
    pub speed: f64,
    /// Ticks between two strikes (`MeleeAttackGoal`: 20), and for a baby when it differs (`intervals(adult, baby)`: the zoglin 41 and 16).
    pub interval: i32,
    pub interval_baby: Option<i32>,
    /// `MeleeAttackGoal(speed, true)`: follows its target even when it does not see it, and goes on after its path ends (zoglin, panda).
    pub follow_unseen: bool,
    /// `SpiderAttackGoal.canContinue`: in light (`brightness >= BRIGHT`) the goal gives up, with one chance in `n` per tick, and drops the target.
    pub quit_in_light: Option<i32>,
    /// `StopAttackingIfTargetInvalid` (the hoglin's brain): the target is forgotten once no path has reached it for this many ticks (200).
    pub forget_unreachable: Option<i32>,
    /// A condition `canContinueToUse` asks again on every tick of the pursuit (the drowned's `okTarget`): once false the goal ends. Nothing is
    /// asked while the goal does not run (its `canUse` condition is the entry's `when`).
    pub keep_while: Option<&'static Cond>,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, interval: 20, interval_baby: None, follow_unseen: false, quit_in_light: None, forget_unreachable: None, keep_while: None };
}
/// The light at which a goal that fears light stops (`brightness >= 0.5`: the same threshold as `horizon:dark_enough`, which is its opposite).
pub const BRIGHT: f32 = 0.5;
/// The melee goal gives up in light: one chance in `.0` per tick (carried only by the creatures whose `Params.quit_in_light` is set).
#[derive(Component, Clone, Copy, Debug)]
pub struct QuitsInLight(pub i32);
/// The condition the goal keeps asking while it runs (carried only by the creatures whose `Params.keep_while` is set).
#[derive(Component, Clone, Copy)]
pub struct KeepsWhile(&'static Cond);

pub static MODULE: Module = Module {
    name: "melee",
    flags: MOVE | LOOK,
    ends_with_path: true,
    resources,
    equip,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Targets, Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};
/// The target service tells the melee goal when its animal gets a target: the first check is armed on the grid.
fn resources(w: &mut World) {
    w.resource_mut::<targeting::TargetWatchers>().0.push(|w, e| {
        if w.get::<hostile::Attack>(e).is_some_and(|a| a.melee_slot != u8::MAX) {
            hostile::arm_melee(w, e);
        }
    });
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    hostile::ensure_attack(w, e);
    let mut a = w.get_mut::<hostile::Attack>(e).unwrap();
    a.melee_slot = inst.slot;
    let p = params::<Params>(p);
    a.speed = p.speed;
    a.interval = p.interval;
    a.interval_baby = p.interval_baby;
    a.follow_unseen = p.follow_unseen;
    a.forget_unreachable = p.forget_unreachable;
    if let Some(n) = p.quit_in_light {
        w.entity_mut(e).insert(QuitsInLight(n));
    }
    if let Some(c) = p.keep_while {
        w.entity_mut(e).insert(KeepsWhile(c));
    }
}
/// `canContinueToUse`, asked before each tick of the goal (`GoalSelector` order), only for the entities that carry a condition and run the goal.
fn keep(w: &mut World) {
    if !crate::goals::sparse_any::<hostile::Meleeing>(w) {
        return;
    }
    let running: Vec<(Entity, &'static Cond, u8)> = w.query_filtered::<(Entity, &KeepsWhile, &hostile::Attack), With<hostile::Meleeing>>().iter(w).map(|(e, k, a)| (e, k.0, a.melee_slot)).collect();
    for (e, c, slot) in running {
        if !c.eval(w, e) {
            crate::goals::end_goal(w, e, slot);
        }
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((keep.run_if(any_with_component::<KeepsWhile>).before(hostile::melee_run), hostile::melee_run).in_set(slot));
}
/// `canUse` is asked only on the ticks where tick + id is even; the 20-tick wait counts from the last ask.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    if off_grid(now, phase_of(w, e)) {
        arm(w, e, inst.slot, 1);
        return false;
    }
    let world_now = w.resource::<Clock>().0;
    if let Some(mut a) = w.get_mut::<hostile::Attack>(e) {
        a.armed = false;
        // The grid of the check is the world's time, as the vanilla goal's `lastCanUseCheck`.
        a.phase = world_now;
    }
    if w.get::<Target>(e).is_none() {
        return false;
    }
    // The check on the grid: start now; if it did not start, the next check is armed.
    start_now(w, e, inst);
    false
}
fn start_now(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::try_start(w, e, inst.slot);
    if w.get::<GoalSet>(e).is_some_and(|g| !g.slot_running(inst.slot)) {
        hostile::arm_melee_after(w, e);
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    hostile::melee_target(w, e).is_some()
}
fn start(w: &mut World, e: Entity, _: Inst) {
    if let Some(p) = hostile::melee_target(w, e) {
        hostile::start_melee(w, e, p);
    }
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    hostile::stop_melee(w, e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd, Level, Pos};
    use crate::species::Config;
    use crate::targeting::{set_target, Eye, Who};

    /// `SpiderAttackGoal.canContinue`: in light the goal gives up (here one chance in 1) and the target goes; without the option, or at night,
    /// the same attacker keeps its target. The light is set by the day flag of the world and read back at the attacker's eyes.
    #[test]
    fn the_goal_gives_up_in_light_only_with_the_option() {
        if crate::cow::skip_unless_walker("melee quit in light") || crate::cow::skip_if_brain("melee quit in light") {
            return;
        }
        static QUITS: Params = Params { quit_in_light: Some(1), follow_unseen: true, ..Params::DEFAULT };
        static KEEPS: Params = Params { follow_unseen: true, ..Params::DEFAULT };
        for day in [true, false] {
            for (p, option) in [(&KEEPS, false), (&QUITS, true)] {
                let mut h = Herd::with_slab_awake(60, true, false);
                h.set_daylight(day);
                let mut sp = *species();
                let entries: &'static [Entry] = Box::leak(vec![Entry::new(&MODULE, 1, p)].into_boxed_slice());
                sp.goals = Config { entries };
                let mut prey_sp = sp;
                prey_sp.goals = Config { entries: &[] };
                let attacker = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
                let prey = spawn_species(&mut h.world, &prey_sp, 4.5, 0.5, 62);
                for _ in 0..5 {
                    h.tick();
                }
                let (pos, eye) = (h.world.get::<Pos>(attacker).unwrap().0, h.world.get::<Eye>(attacker).unwrap().0);
                let bright = h.world.resource::<Level>().mw.0.brightness(pos.x.floor() as i32, (pos.y + eye).floor() as i32, pos.z.floor() as i32) >= BRIGHT;
                assert_eq!(bright, day, "the world's light is the one asked for");
                set_target(&mut h.world, attacker, crate::targeting::Target::new(Who::Entity(prey), 200, u8::MAX));
                h.world.resource_mut::<Requests>().0.push(Req::Start(attacker, 0));
                for _ in 0..30 {
                    h.tick();
                }
                let kept = h.world.get::<crate::targeting::Target>(attacker).is_some();
                assert_eq!(kept, !(option && day), "day {day}, option {option}: the target is kept");
            }
        }
    }
}
