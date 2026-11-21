//! `Fox.StalkPreyGoal` (fiche 22): the fox creeps up on its prey (a chicken or a rabbit, `types`) from more than 6 blocks
//! away; within 6 it crouches and keeps still. The goal runs while `canUse` holds, asked on the selector ticks; a target
//! acquired wakes it, and a refusal that depends on the distance asks again in 2 ticks while the prey is there.
use super::fox_state::{self as fs, CROUCHING, INTERESTED, SITTING, FACEPLANTED, SLEEPING};
use super::fox_pounce;
use super::*;
use crate::targeting::TargetWatchers;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed of the approach.
    pub speed: f64,
    /// The squared distance under which the fox crouches (6 blocks: 36).
    pub still_sqr: f64,
    /// The creatures it stalks (`TypeName`); the species' data, empty by default.
    pub types: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.5, still_sqr: 36.0, types: &[] };
}

/// The parameters, kept from the creation (`equip`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Plan {
    speed: f64,
    still_sqr: f64,
    types: &'static [&'static str],
}
/// The goal runs (`stalk_run` visits it).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Stalking;

pub static MODULE: Module = Module {
    name: "fox_stalk_prey",
    flags: MOVE | LOOK,
    resources,
    equip,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Targets, Cap::Locomotion],
    ..Module::DEFAULT
};

/// A new target wakes the goal (`Fox.StalkPreyGoal.canUse` is asked each selector tick; this is the event that matters).
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
    w.entity_mut(e).insert(Plan { speed: p.speed, still_sqr: p.still_sqr, types: p.types });
}

/// The goal is asked on the selector ticks only.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    fs::retry_done(w, e);
    if !fs::selector_tick(w, e) {
        fs::retry_in(w, e, inst.slot, 1);
        return false;
    }
    true
}

/// `jumping`: the move control asks for a jump (`Mob.ai.jumping`, read as the drive's jump).
fn jumping(w: &World, e: Entity) -> bool {
    w.get::<Drive>(e).is_some_and(|d| d.jump)
}

/// The conditions of `canUse`, without the side effect of a retry.
fn ready(w: &World, e: Entity) -> bool {
    let Some(plan) = w.get::<Plan>(e) else { return false };
    if fs::has(w, e, SLEEPING) {
        return false;
    }
    let Some((who, info)) = fs::live_target(w, e) else { return false };
    fs::is_type(w, who, plan.types)
        && d2(fs::feet(w, e), info.pos) > plan.still_sqr
        && !fs::has(w, e, CROUCHING | INTERESTED)
        && !jumping(w, e)
}

fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if ready(w, e) {
        return true;
    }
    // The distance and the jump change without an event: asked again in 2 ticks while the prey is there and the fox is
    // not crouched already (`Mob` asks each 2 ticks).
    let Some(plan) = w.get::<Plan>(e).copied() else { return false };
    if !fs::has(w, e, SLEEPING | CROUCHING | INTERESTED)
        && fs::live_target(w, e).is_some_and(|(who, _)| fs::is_type(w, who, plan.types))
    {
        fs::retry_in(w, e, inst.slot, 2);
    }
    false
}

fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Stalking);
    fs::set(w, e, SITTING | FACEPLANTED, false);
}

/// `Fox.StalkPreyGoal.stop`: the crouch is kept only when the way to the prey is clear; then `Pounce` is asked.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Stalking>();
    fs::stop_walk(w, e);
    match fs::live_target(w, e) {
        Some((_, info)) if fs::path_clear(w, fs::feet(w, e), info.pos) => {
            fs::set(w, e, INTERESTED | CROUCHING, true);
            stop_nav(w, e);
            fs::aim_at(w, e, [info.pos.x, info.eye, info.pos.z], None);
            wake(w, e, &fox_pounce::MODULE);
        }
        _ => fs::set(w, e, INTERESTED | CROUCHING, false),
    }
}

/// The tick of the module, run only while a stalking fox lives (no registered system).
fn tick(w: &mut World) {
    stalk_run(w);
}

/// `StalkPreyGoal.tick` on the selector ticks: the goal ends when `canUse` no longer holds; else it looks at the prey and
/// either crouches (within 6 blocks) or asks the path to it.
fn stalk_run(w: &mut World) {
    let list = crate::goals::entities::<With<Stalking>>(w);
    for e in list {
        let Some(slot) = slot_of(w, e, &MODULE) else { continue };
        if !fs::selector_tick(w, e) {
            continue;
        }
        if !ready(w, e) {
            w.resource_mut::<Requests>().0.push(Req::End(e, slot));
            continue;
        }
        let Some((_, info)) = fs::live_target(w, e) else { continue };
        let plan = *w.get::<Plan>(e).unwrap();
        fs::aim_at(w, e, [info.pos.x, info.eye, info.pos.z], None);
        if d2(fs::feet(w, e), info.pos) <= plan.still_sqr {
            fs::set(w, e, INTERESTED | CROUCHING, true);
            stop_nav(w, e);
        } else {
            fs::walk_to(w, e, Vec3::new(info.pos.x, info.pos.y, info.pos.z), plan.speed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;
    use crate::targeting::{Target, Who, set_target};

    static ENTRIES: [Entry; 2] = [Entry::new(&MODULE, 5, &PREY), Entry::new(&fox_pounce::MODULE, 6, &fox_pounce::Params::DEFAULT)];
    static PREY: Params = Params { types: &["minecraft:cow"], ..Params::DEFAULT };

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    fn table_cow(h: &Herd) -> Species {
        *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap()
    }
    /// A fox (a cow body named as one, with the stalking and pounce goals) at x, and a cow that stands still at px.
    fn scene(h: &mut Herd, x: f64, px: f64) -> (Entity, Entity) {
        let mut fox = table_cow(h);
        fox.name = "minecraft:fox";
        fox.goals = Config { entries: &ENTRIES };
        let f = spawn_species(&mut h.world, &fox, x, 0.5, 11);
        let mut prey = table_cow(h);
        prey.goals = Config { entries: &[] };
        let p = spawn_species(&mut h.world, &prey, px, 0.5, 12);
        h.world.flush();
        h.world.get_mut::<ActiveUntil>(p).unwrap().0 = u64::MAX;
        let mut t = Target::new(Who::Entity(p), 100, u8::MAX);
        t.must_see = false;
        set_target(&mut h.world, f, t);
        (f, p)
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn flag(h: &Herd, e: Entity, f: u8) -> bool {
        fs::has(&h.world, e, f)
    }

    /// Far from the prey the fox starts, walks toward it, and within 6 blocks it crouches and stops with the flags set.
    #[test]
    fn a_fox_stalks_a_far_prey_and_crouches_near_it() {
        let Some(mut h) = herd() else { return };
        let (f, p) = scene(&mut h, 0.5, 14.5);
        let x0 = fs::feet(&h.world, f).x;
        let (mut started, mut stopped_at) = (false, None);
        for _ in 0..400 {
            h.tick();
            started |= running(&h, f);
            if started && !running(&h, f) {
                stopped_at = Some(());
                break;
            }
        }
        assert!(started, "a prey at 14 blocks starts the stalk");
        assert!(stopped_at.is_some(), "the goal ends once crouched");
        let d = d2(fs::feet(&h.world, f), fs::feet(&h.world, p)).sqrt();
        assert!(fs::feet(&h.world, f).x > x0 + 5.0, "the fox came toward the prey");
        assert!(d <= 6.0 + 1e-6, "within six blocks, distance {d}");
        assert!(flag(&h, f, CROUCHING) && flag(&h, f, INTERESTED), "the way is clear: crouched and interested");
    }

    /// A wall between the fox and the prey: the same approach ends with the crouch refused by `path_clear`.
    #[test]
    fn a_wall_between_fox_and_prey_drops_the_crouch() {
        let Some(mut h) = herd() else { return };
        let (f, p) = scene(&mut h, 0.5, 9.5);
        let y = fs::feet(&h.world, f).y.floor() as i32;
        for z in -30..=30 {
            for j in 1..=3 {
                h.world.resource_mut::<BlockChanges>().0.push((6, y + j, z, "minecraft:stone"));
            }
        }
        let mut started = false;
        for _ in 0..400 {
            h.tick();
            started |= running(&h, f);
        }
        let d = d2(fs::feet(&h.world, f), fs::feet(&h.world, p)).sqrt();
        assert!(started && d <= 6.0 + 1e-6, "the fox came to the wall (distance {d})");
        assert!(!running(&h, f), "the goal ended");
        assert!(!flag(&h, f, CROUCHING) && !flag(&h, f, INTERESTED), "the wall is in the line of sight: no crouch");
    }
}
