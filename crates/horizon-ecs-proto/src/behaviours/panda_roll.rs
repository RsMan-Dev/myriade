//! `PandaRollGoal` (`mobs/panda.rs:991-1031`): a cub, or a playful panda, rolls over when the block in front of it, one
//! below, is air, and now and then at random. Priority 12, flags MOVE | LOOK | JUMP.
//! Asks come every 2 ticks while the panda is a cub or playful. A ask with the ground in front empty is decided by that
//! test alone (no draw); any other ask counts one draw down (`asks_left`, drawn once, the first draw counting as 1).
use super::panda_state::{self as ps, Gene, FLAG_ROLL};
use super::*;
use crate::cow::{Level, Pos, Yaw, Contact};
use crate::goals::{Req, Requests, JUMP, LOOK, MOVE};
use crate::rules::panda_roll::front_below;

/// `reduced(60)` = 30 for a playful panda, `reduced(500)` = 250 otherwise.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub playful_asks: i32,
    pub asks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { playful_asks: 30, asks: 250 };
}

/// `armed`: the next ask is in the wheel; `asks_left`: the draw's countdown, `None` until it is drawn.
#[derive(Component, Clone, Copy, Debug)]
pub struct Roll {
    pub def: Params,
    pub slot: u8,
    pub armed: bool,
    pub asks_left: Option<i32>,
}

pub static MODULE: Module = Module {
    name: "panda_roll",
    flags: MOVE | LOOK | JUMP,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    // `PandaRollGoal.isInterruptible` is false (`panda.rs:1028`).
    interruptable: false,
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    ps::ensure(w, e);
    let def = *params::<Params>(p);
    w.entity_mut(e).insert(Roll { def, slot: inst.slot, armed: false, asks_left: None });
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    arm_ask(w, e);
}
/// An ask: `canUse` decides it.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(mut r) = w.get_mut::<Roll>(e) else { return false };
    r.armed = false;
    true
}
fn young_or_playful(w: &World, e: Entity) -> bool {
    ps::is_baby(w, e) || ps::gene(w, e) == Gene::Playful
}
/// `PandaRollGoal.canUse`.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(r) = w.get::<Roll>(e).copied() else { return false };
    if !young_or_playful(w, e) {
        return false;
    }
    let grounded = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    if !(grounded && ps::acts(w, e)) {
        arm_ask(w, e);
        return false;
    }
    if front_is_air(w, e) {
        return true;
    }
    let left = match r.asks_left {
        Some(k) => k,
        None if ps::gene(w, e) == Gene::Playful => ps::asks_until(w, e, r.def.playful_asks).min(ps::asks_until(w, e, r.def.asks)),
        None => ps::asks_until(w, e, r.def.asks),
    } - 1;
    if left <= 0 {
        w.get_mut::<Roll>(e).unwrap().asks_left = None;
        return true;
    }
    w.get_mut::<Roll>(e).unwrap().asks_left = Some(left);
    arm_ask(w, e);
    false
}
/// `PandaRollGoal.start`: the roll flag goes up; the goal does not continue (`canContinueToUse` is false).
fn start(w: &mut World, e: Entity, inst: Inst) {
    ps::set_flag(w, e, FLAG_ROLL, true);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    arm_ask(w, e);
}

/// Arms the next ask (2 ticks on) of a cub or a playful panda, once.
fn arm_ask(w: &mut World, e: Entity) {
    let Some(r) = w.get::<Roll>(e).copied() else { return };
    if r.armed || !young_or_playful(w, e) {
        return;
    }
    w.get_mut::<Roll>(e).unwrap().armed = true;
    crate::goals::arm(w, e, r.slot, 2);
}

/// `Panda.rollOver` test of `canUse` (`panda.rs:1005-1008`): the block one below, in the cell in front of the nose, is air.
/// The yaw is in degrees, as the `Mob`'s.
fn front_is_air(w: &World, e: Entity) -> bool {
    let p = w.get::<Pos>(e).unwrap().0;
    let (x, below, z) = front_below(p.x, p.y, p.z, f64::from(w.get::<Yaw>(e).unwrap().0));
    let mw = w.resource::<Level>().mw.0;
    mw.facts.is_air(mw.world.block(x, below, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd, Species};
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 12, &Params::DEFAULT)];

    /// Pandas on the slab, landed, yaw 0 (the cell in front is the one at +z). Returns the herd and the entities.
    fn pandas(seed: u64, genes: &[(Gene, bool)]) -> (Herd, Vec<Entity>) {
        let mut h = Herd::with_slab_awake(8, true, false);
        let mut sp: Species = *species();
        sp.goals = Config { entries: &ENTRIES };
        let mut out = Vec::new();
        for (i, &(g, baby)) in genes.iter().enumerate() {
            let e = spawn_species(&mut h.world, &sp, 0.5 + 3.0 * i as f64, 0.5, seed + i as u64);
            h.world.entity_mut(e).insert(g);
            if baby {
                h.world.entity_mut(e).insert(crate::cow::GrowAt(u64::MAX));
            }
            h.world.get_mut::<Yaw>(e).unwrap().0 = 0.0;
            out.push(e);
        }
        crate::goals::arm_all(&mut h.world);
        for _ in 0..30 {
            h.tick();
        }
        (h, out)
    }
    fn rolling(h: &Herd, e: Entity) -> bool {
        h.world.get::<ps::PandaState>(e).is_some_and(|s| s.flags & FLAG_ROLL != 0)
    }

    /// A cub facing a hole rolls at its first asks (departure within a few ticks, the ask is 2 ticks on); the roll goal
    /// is over the next tick (no continuation). A playful panda rolls at random within a few thousand ticks, and an
    /// adult cub that is neither stays put in the same time.
    #[test]
    fn a_cub_facing_a_hole_rolls_at_once_and_a_playful_panda_rolls_at_random() {
    if crate::cow::skip_unless_small_body("a_cub_facing_a_hole_rolls_at_once_and_a_playful_panda_rolls_at_random") {
        return;
    }
    if crate::cow::skip_unless_walker("a_cub_facing_a_hole_rolls_at_once_and_a_playful_panda_rolls_at_random") {
        return;
    }
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        let (mut h, es) = pandas(5, &[(Gene::Normal, true), (Gene::Playful, false), (Gene::Normal, false)]);
        let (cub, playful, adult) = (es[0], es[1], es[2]);
        // The cell in front of the cub (+z) and one below: air.
        let p = h.world.get::<Pos>(cub).unwrap().0;
        h.set_block(p.x.floor() as i32, p.y.floor() as i32 - 1, p.z.floor() as i32 + 1, "minecraft:air");
        let mut t_cub = None;
        for t in 1..=10 {
            h.tick();
            if rolling(&h, cub) {
                t_cub = Some(t);
                break;
            }
        }
        let t_cub = t_cub.expect("the cub facing a hole did not roll within 10 ticks");
        assert!(t_cub <= 4, "the cub rolled {t_cub} ticks late: the first ask is 2 ticks on");
        h.tick();
        assert!(!h.world.get::<crate::goals::GoalSet>(cub).unwrap().is_running(&MODULE), "the roll goal goes on after its start");

        let mut t_playful = None;
        for t in 1..=4000 {
            h.tick();
            if rolling(&h, playful) {
                t_playful = Some(t);
                break;
            }
        }
        assert!(t_playful.is_some(), "the playful panda never rolled in 4000 ticks");
        assert!(!rolling(&h, adult), "an adult panda that is neither cub nor playful rolled");
    }
}
