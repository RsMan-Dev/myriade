//! `Fox.FoxPounceGoal` (fiche 23): once fully crouched with a clear path, the fox leaps at its target, tilts with the flight,
//! bites within 2 blocks, and when it lands with its nose in snow it faceplants (`Pounce` sets FACEPLANTED and wakes the
//! faceplant goal). Its tick runs on the selector ticks only, as the `Mob`'s goal does.
use super::fox_state::{self as fs, CROUCHING, FACEPLANTED, INTERESTED, POUNCING};
use super::fox_faceplant;
use super::*;
use crate::targeting::{TargetWatchers, Who, clear_target};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;
use crate::rules::fox_pounce::{keeps_flying, next_tilt, unit_xz};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The horizontal push toward the target (x, then the upward push, then z).
    pub lunge: (f64, f64, f64),
    /// The squared distance at which the fox bites.
    pub bite_sqr: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { lunge: (0.8, 0.9, 0.8), bite_sqr: 4.0 };
}

#[derive(Component, Clone, Copy, Debug)]
struct Lunge {
    lunge: (f64, f64, f64),
    bite_sqr: f64,
}
/// `m.head_pitch`: the tilt of the head with the flight (its effect only is the tests' business).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HeadTilt(pub f32);
/// The goal runs (`pounce_run` visits it).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Pouncing;

pub static MODULE: Module = Module {
    name: "fox_pounce",
    flags: MOVE | JUMP,
    resources,
    equip,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Targets, Cap::Locomotion, Cap::Living],
    // `FoxPounceGoal.isInterruptible` is false (`fox.rs:1247`): a goal of a higher priority cannot take MOVE | JUMP from the leap.
    interruptable: false,
    ..Module::DEFAULT
};

/// A new target wakes the goal: a crouched fox may now pounce at it.
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(|w, e| {
        if w.get::<fs::FoxFlags>(e).is_some() {
            wake(w, e, &MODULE);
        }
    });
}

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    fs::ensure(w, e);
    let p = params::<Params>(p);
    w.entity_mut(e).insert((Lunge { lunge: p.lunge, bite_sqr: p.bite_sqr }, HeadTilt::default()));
}

/// The deadline of a crouch (or of a retry) came: asked on the selector ticks only.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    fs::retry_done(w, e);
    if !fs::selector_tick(w, e) {
        fs::retry_in(w, e, inst.slot, 1);
        return false;
    }
    true
}

/// `FoxPounceGoal.canUse`: fully crouched (a crouch that starts wakes it; its time is armed here), a live target, a clear
/// path (else the crouch is dropped).
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(full) = fs::fully_crouched_at(w, e) else { return false };
    let now = now_of(w, e);
    if now < full {
        fs::retry_in(w, e, inst.slot, (full - now) as i32);
        return false;
    }
    let Some((_, info)) = fs::live_target(w, e) else { return false };
    if !fs::path_clear(w, fs::feet(w, e), info.pos) {
        fs::set(w, e, CROUCHING | INTERESTED, false);
        return false;
    }
    true
}

/// `FoxPounceGoal.canContinue`: a live target, and still airborne, moving up or down, or tilted; not faceplanted.
fn can_continue(w: &World, e: Entity) -> bool {
    if fs::live_target(w, e).is_none() {
        return false;
    }
    let (vel, on_ground) = (w.get::<Vel>(e).unwrap().0, w.get::<Contact>(e).unwrap().on_ground);
    let tilt = w.get::<HeadTilt>(e).unwrap().0;
    keeps_flying(vel.y, tilt, on_ground) && !fs::has(w, e, FACEPLANTED)
}

fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Pouncing);
    fs::set(w, e, POUNCING, true);
    fs::set(w, e, INTERESTED, false);
    let lunge = w.get::<Lunge>(e).unwrap().lunge;
    if let Some((_, info)) = fs::live_target(w, e) {
        let at = fs::feet(w, e);
        fs::aim_at(w, e, [info.pos.x, info.eye, info.pos.z], Some(60.0));
        let (dx, dy, dz) = (info.pos.x - at.x, info.pos.y - at.y, info.pos.z - at.z);
        let (ux, uz) = unit_xz(dx, dy, dz);
        let v = w.get::<Vel>(e).unwrap().0;
        w.get_mut::<Vel>(e).unwrap().0 = Vec3::new(v.x + ux * lunge.0, v.y + lunge.1, v.z + uz * lunge.2);
        // The physics visits only the moving animals: the leap wakes it (as a push does).
        w.entity_mut(e).insert(Moving);
    }
    stop_nav(w, e);
    // The goal's first tick is the same one (`GoalSelector.tick` ticks what it starts).
    body(w, e);
}

fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Pouncing>();
    fs::set(w, e, CROUCHING | INTERESTED | POUNCING, false);
}

/// The tick of the module, run only while a fox that pounces lives (no registered system).
fn tick(w: &mut World) {
    pounce_run(w);
}

/// The selector tick of the goal: it ends when it can no longer continue (and wakes the faceplant when the nose is in the
/// snow), else its tick.
fn pounce_run(w: &mut World) {
    let list = crate::goals::entities::<With<Pouncing>>(w);
    for e in list {
        let Some(slot) = slot_of(w, e, &MODULE) else { continue };
        if !fs::selector_tick(w, e) {
            continue;
        }
        if !can_continue(w, e) {
            w.resource_mut::<Requests>().0.push(Req::End(e, slot));
            if fs::has(w, e, FACEPLANTED) {
                wake(w, e, &fox_faceplant::MODULE);
            }
            continue;
        }
        body(w, e);
    }
}

/// `FoxPounceGoal.tick`: look at the target, tilt the head with the flight, bite within reach, or land in snow.
fn body(w: &mut World, e: Entity) {
    let target = fs::live_target(w, e);
    if let Some((_, info)) = target {
        fs::aim_at(w, e, [info.pos.x, info.eye, info.pos.z], Some(60.0));
    }
    let vel = w.get::<Vel>(e).unwrap().0;
    let on_ground = w.get::<Contact>(e).unwrap().on_ground;
    let at = fs::feet(w, e);
    let jumping = w.get::<Drive>(e).is_some_and(|d| d.jump);
    let mut tilt = w.get::<HeadTilt>(e).unwrap().0;
    if !fs::has(w, e, FACEPLANTED) {
        tilt = next_tilt(tilt, [vel.x, vel.y, vel.z], jumping);
    }
    w.get_mut::<HeadTilt>(e).unwrap().0 = tilt;
    let bite_sqr = w.get::<Lunge>(e).unwrap().bite_sqr;
    match target {
        Some((who, info)) if d2(at, info.pos) <= bite_sqr => {
            if let Who::Entity(t) = who {
                let dmg = w.get::<AttackDamage>(e).map_or(0.0, |a| a.0);
                w.resource_mut::<DamageQueue>().add_by_entity(t, dmg, (at.x, at.z), e);
            }
        }
        _ if tilt > 0.0 && on_ground && vel.y as f32 != 0.0 && fs::in_snow(w, at) => {
            w.get_mut::<HeadTilt>(e).unwrap().0 = 60.0;
            clear_target(w, e);
            fs::set(w, e, FACEPLANTED, true);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;
    use crate::targeting::{Target, set_target};

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 6, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    fn table_cow(h: &Herd) -> Species {
        *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap()
    }
    /// A crouching fox at x with a prey cow at px as its target.
    fn scene(h: &mut Herd, x: f64, px: f64) -> (Entity, Entity) {
        let mut fox = table_cow(h);
        fox.name = "minecraft:fox";
        fox.goals = Config { entries: &ENTRIES };
        let f = spawn_species(&mut h.world, &fox, x, 0.5, 21);
        let mut prey = table_cow(h);
        prey.goals = Config { entries: &[] };
        let p = spawn_species(&mut h.world, &prey, px, 0.5, 22);
        h.world.flush();
        h.world.get_mut::<ActiveUntil>(p).unwrap().0 = u64::MAX;
        h.world.get_mut::<AttackDamage>(f).unwrap().0 = 2.0;
        let mut t = Target::new(Who::Entity(p), 100, u8::MAX);
        t.must_see = false;
        set_target(&mut h.world, f, t);
        (f, p)
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }

    /// A fox crouched at a prey 3 blocks away: it stays put before its crouch is full, leaps once it is (upward and toward
    /// the prey), is airborne while it runs, and ends with the landing.
    #[test]
    fn a_crouched_fox_leaps_at_its_prey_once_fully_crouched() {
        let Some(mut h) = herd() else { return };
        let (f, _p) = scene(&mut h, 0.5, 3.5);
        fs::set(&mut h.world, f, CROUCHING, true);
        wake(&mut h.world, f, &MODULE);
        for _ in 0..10 {
            h.tick();
        }
        assert!(!running(&h, f), "not fully crouched after 10 ticks");
        let mut started = false;
        for _ in 0..40 {
            h.tick();
            if running(&h, f) {
                started = true;
                break;
            }
        }
        assert!(started, "fully crouched: the pounce starts");
        let v = h.world.get::<Vel>(f).unwrap().0;
        assert!(v.y > 0.3, "the leap pushes up: vertical speed {}", v.y);
        assert!(v.x > 0.0, "the leap is toward the prey at +x: {}", v.x);
        assert!(fs::has(&h.world, f, POUNCING), "the pounce flag is set while it runs");
        let mut airborne = false;
        for _ in 0..200 {
            h.tick();
            airborne |= !h.world.get::<Contact>(f).unwrap().on_ground;
            if !running(&h, f) {
                break;
            }
        }
        assert!(airborne, "the fox left the ground");
        assert!(!running(&h, f), "the pounce ends at the landing");
        assert!(!fs::has(&h.world, f, POUNCING | CROUCHING), "the flags are cleared when it ends");
    }
}
