//! The wither's charge (`WitherBoss.makeInvulnerable`, `customServerAiStep` while the counter runs): `ticks` of standing still
//! (MOVE | JUMP | LOOK taken), then the blast from its eyes (radius 7, no fire, itself as source) and the roar where it stands.
//! The end is one wheel deadline (`Due::Goal`), no tick.
use super::*;
use crate::behaviours::do_nothing;
use crate::cow::*;
use crate::goals::*;
use crate::projectile::Blast;
use crate::rules::invulnerable_charge::{blast_at, charge_until};
use crate::targeting::{Eye, Who};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub ticks: i32,
    pub radius: f32,
    pub sound: &'static str,
}
impl Params {
    pub const DEFAULT: Self = Self { ticks: 220, radius: 7.0, sound: "entity.wither.spawn" };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Charge {
    until: u64,
    p: &'static Params,
    done: bool,
}
impl Charge {
    /// A charge that is not running: the marker of the species that has the goal (its NBT owner), for a bare entity.
    pub fn idle() -> Self {
        Self { until: 0, p: &Params::DEFAULT, done: true }
    }
}
pub static MODULE: Module = Module {
    name: "invulnerable_charge",
    flags: MOVE | JUMP | LOOK,
    equip,
    fired,
    can_use,
    start,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
/// `horizon:is_charging`: the animal is still charging.
pub fn charging(w: &World, e: Entity) -> bool {
    w.get::<Charge>(e).is_some_and(|c| !c.done)
}
/// The tick the charge ends, while it holds.
pub fn ends_at(w: &World, e: Entity) -> Option<u64> {
    w.get::<Charge>(e).filter(|c| !c.done).map(|c| c.until)
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    let until = charge_until(now_of(w, e), p.ticks);
    w.entity_mut(e).insert(Charge { until, p, done: false });
    w.entity_mut(e).insert(do_nothing::Invulnerable(until));
    arm_at(w, e, until, Due::Goal(inst.slot));
    crate::goals::arm(w, e, inst.slot, 0);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Charge>(e).is_some_and(|c| !c.done && now_of(w, e) >= c.until)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    charging(w, e)
}
/// Runs while the charge holds; the blast and the end come at the deadline.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(c) = w.get::<Charge>(e).copied() else { return };
    if c.done || now_of(w, e) < c.until {
        return;
    }
    w.get_mut::<Charge>(e).unwrap().done = true;
    do_nothing::set(w, e, 0);
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let eye = w.get::<Eye>(e).map_or(1.6, |l| l.0);
    let mut ev = w.resource_mut::<Events>();
    ev.exploded.push(Blast {
        at: blast_at(pos, eye),
        radius: c.p.radius,
        fire: false,
        owner: Some(Who::Entity(e)),
        source: Some(e),
    });
    ev.host.sound(c.p.sound, [pos.x, pos.y, pos.z], 1.0, 1.0);
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::species::Config;
    use crate::{cow::{ActiveUntil, Herd, SpeciesTable, spawn_species}, goals::set_lazy_look};

    static SHORT: Params = Params { ticks: 20, ..Params::DEFAULT };

    #[test]
    fn the_counter_runs_with_the_charge_and_is_zero_after_the_blast() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            crate::cow::skip_note("the_counter_runs_with_the_charge_and_is_zero_after_the_blast", "HORIZON_TEST_RESOURCES is not set");
            return;
        }
        let mut h = Herd::with_slab_awake(20, true, false);
        set_lazy_look(&mut h.world, false);
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        let entries = vec![Entry::new(&MODULE, 0, &SHORT), Entry::new(&do_nothing::MODULE, 0, &do_nothing::Params::DEFAULT)];
        sp.goals = Config { entries: Box::leak(entries.into_boxed_slice()) };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.tick();
        let first = do_nothing::counter(&h.world, e);
        assert!((1..=20).contains(&first), "the counter counts down from the charge's ticks, got {first}");
        for _ in 0..40 {
            h.tick();
        }
        assert_eq!(do_nothing::counter(&h.world, e), 0, "the blast ends the counter");
    }
}
