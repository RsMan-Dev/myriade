//! `PandaSneezeGoal` (`mobs/panda.rs:903-928`): a cub sneezes now and then, a weak one more often. Priority 8, no flag.
//! Its asks come every 2 ticks: the number of asks before the first success is drawn once and armed in the wheel.
use super::panda_state::{self as ps, Gene, FLAG_SNEEZE};
use super::*;
use crate::cow::now_of;
use crate::goals::{Req, Requests};

/// The values are the `reduced` ones of the `Mob`: `rnd(reduced(500))` = 250 for a weak cub, `rnd(reduced(6000))` = 3000.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub weak_asks: i32,
    pub asks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { weak_asks: 250, asks: 3000 };
}

/// `armed`: its deadline is in the wheel; `at`: the entity's time it is due at.
#[derive(Component, Clone, Copy, Debug)]
pub struct Sneeze {
    pub def: Params,
    pub slot: u8,
    pub armed: bool,
    pub at: u64,
}

pub static MODULE: Module = Module {
    name: "panda_sneeze",
    flags: 0,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    ps::ensure(w, e);
    w.entity_mut(e).insert(Sneeze { def: *params::<Params>(p), slot: inst.slot, armed: false, at: 0 });
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    arm_sneeze(w, e);
}
/// One deadline: the ask is made (`can_use`). A deadline that comes early (the wheel's limit) arms the rest.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(s) = w.get::<Sneeze>(e).copied() else { return false };
    if now_of(w, e) < s.at {
        ps::arm_to(w, e, s.slot, s.at);
        return false;
    }
    w.get_mut::<Sneeze>(e).unwrap().armed = false;
    true
}
/// `PandaSneezeGoal.canUse`: a cub that is acting (`acts`). Otherwise the next ask is armed.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    if ps::is_baby(w, e) && ps::acts(w, e) {
        return true;
    }
    arm_sneeze(w, e);
    false
}
/// `PandaSneezeGoal.start`: the sneeze flag goes up (its counter and effects are the panda's own rule, not ported).
/// The goal does not continue: it ends at once.
fn start(w: &mut World, e: Entity, inst: Inst) {
    ps::set_flag(w, e, FLAG_SNEEZE, true);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    arm_sneeze(w, e);
}

/// Arms the next ask of a cub: a weak cub's draws are the two `Mob` draws of the same ask, so the first success of
/// either comes first (the two draws are independent and the success of the ask is their union).
fn arm_sneeze(w: &mut World, e: Entity) {
    let Some(s) = w.get::<Sneeze>(e).copied() else { return };
    if s.armed || !ps::is_baby(w, e) {
        return;
    }
    let mut k = ps::asks_until(w, e, s.def.asks);
    if ps::gene(w, e) == Gene::Weak {
        k = k.min(ps::asks_until(w, e, s.def.weak_asks));
    }
    let at = now_of(w, e) + 2 * k as u64;
    ps::arm_to(w, e, s.slot, at);
    let mut m = w.get_mut::<Sneeze>(e).unwrap();
    m.armed = true;
    m.at = at;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd, Species};
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 8, &Params::DEFAULT)];

    /// A cow with the panda goal alone (a test configuration: the species of the process is not changed).
    fn pandas(seed: u64, babies: &[bool]) -> (Herd, Vec<Entity>) {
        let mut h = Herd::with_slab_awake(8, true, false);
        let mut sp: Species = *species();
        sp.goals = Config { entries: &ENTRIES };
        let mut out = Vec::new();
        for (i, &baby) in babies.iter().enumerate() {
            let e = spawn_species(&mut h.world, &sp, 0.5 + 2.0 * i as f64, 0.5, seed + i as u64);
            h.world.entity_mut(e).insert(ps::Gene::Weak);
            if baby {
                h.world.entity_mut(e).insert(crate::cow::GrowAt(u64::MAX));
            }
            out.push(e);
        }
        crate::goals::arm_all(&mut h.world);
        (h, out)
    }
    fn sneezing(h: &Herd, e: Entity) -> bool {
        h.world.get::<ps::PandaState>(e).is_some_and(|s| s.flags & FLAG_SNEEZE != 0)
    }

    /// A weak cub sneezes within a few thousand ticks (departure), its goal is over the next tick (no continuation);
    /// the adult beside it, same seed, never does in the same time (the cub is the witness that the test can see it).
    #[test]
    fn a_weak_cub_sneezes_and_its_goal_ends_at_once_while_an_adult_does_not() {
    if crate::cow::skip_unless_walker("a_weak_cub_sneezes_and_its_goal_ends_at_once_while_an_adult_does_not") {
        return;
    }
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let (mut h, es) = pandas(7, &[true, false]);
        let (cub, adult) = (es[0], es[1]);
        let mut at = None;
        for t in 1..=6000 {
            h.tick();
            if sneezing(&h, cub) {
                at = Some(t);
                break;
            }
        }
        let t = at.expect("the weak cub never sneezed in 6000 ticks: the goal does not start");
        h.tick();
        assert!(!h.world.get::<crate::goals::GoalSet>(cub).unwrap().is_running(&MODULE), "the goal goes on after its start");
        assert!(!sneezing(&h, adult), "an adult sneezed");
        assert!(t > 2, "sneezed on the first tick: the first ask comes after the wheel");
    }

    /// The goal is armed again after it ends: a cub that sneezed is asked again (the second sneeze comes after the first
    /// one's flag is cleared, as the panda's own rule does; here the goal's deadline is what is checked).
    #[test]
    fn after_its_end_the_cub_arms_its_next_ask() {
    if crate::cow::skip_unless_walker("after_its_end_the_cub_arms_its_next_ask") {
        return;
    }
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let (mut h, es) = pandas(11, &[true]);
        let cub = es[0];
        for _ in 0..6000 {
            h.tick();
            if sneezing(&h, cub) {
                break;
            }
        }
        assert!(sneezing(&h, cub), "no sneeze to end");
        h.tick();
        let s = *h.world.get::<Sneeze>(cub).unwrap();
        assert!(s.armed && s.at > crate::cow::now_of(&h.world, cub), "no next deadline is armed after the end");
    }
}
