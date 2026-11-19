//! `EndermanTakeBlockGoal`: an enderman that carries nothing, with `mobGriefing`, takes now and then the block it looks at
//! in a 4 x 3 x 4 box around it, if the block is in the holdable tag and reachable from its column (the outline clip).
//! One chance in `asks` (`reducedTickDelay(20)`) per ask: the draw is a deadline of the wheel, armed while the enderman
//! carries nothing. The goal ticks once, at its start (as the `Mob`, which has no `canContinueToUse`): the block goes
//! (air through `BlockChanges`), the block is carried, and the drop goal is armed.
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `reducedTickDelay(20)`: one chance in this many per ask.
    pub asks: i32,
    /// The block tag of the blocks an enderman may take.
    pub holdable: &'static str,
}
impl Params {
    pub const DEFAULT: Self = Self { asks: 10, holdable: "minecraft:enderman_holdable" };
}
/// The block the enderman carries, by name (the state's properties are not kept: DOUBT).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Carried(pub Option<&'static str>);
/// The draw of this goal: its parameters and whether its deadline is in the wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Taking {
    pub asks: i32,
    pub holdable: &'static str,
    pub armed: bool,
    pub slot: u8,
}

pub static MODULE: Module = Module {
    name: "take_block",
    flags: 0,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Transform, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Taking { asks: p.asks, holdable: p.holdable, armed: false, slot: inst.slot });
    if w.get::<Carried>(e).is_none() {
        w.entity_mut(e).insert(Carried(None));
    }
}
/// Arms the next draw while the enderman carries nothing and the rule allows it (`mobGriefing`, checked before the draw).
/// The geometric wait is the draw: `can_use` only checks the conditions.
pub fn arm_take(w: &mut World, e: Entity) {
    let Some(t) = w.get::<Taking>(e).copied() else { return };
    if t.armed || w.get::<Carried>(e).is_some_and(|c| c.0.is_some()) || !mob_griefing(w) {
        return;
    }
    let wait = with_rng(w, e, |r| 2 * geometric(r, t.asks));
    crate::goals::arm(w, e, t.slot, wait);
    w.get_mut::<Taking>(e).unwrap().armed = true;
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    arm_take(w, e);
}
/// A deadline of this goal (the draw came up) or a replayed start: always asks `can_use`.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut t) = w.get_mut::<Taking>(e) {
        t.armed = false;
    }
    true
}
/// `EndermanTakeBlockGoal.canUse` after the draw: carries nothing, and `mobGriefing`.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Carried>(e).is_some_and(|c| c.0.is_none()) && mob_griefing(w)
}
fn block_at(w: &World, x: i32, y: i32, z: i32) -> &'static str {
    let mw = w.resource::<Level>().mw.0;
    &mw.facts.get(mw.world.block(x, y, z)).name
}
/// `EndermanTakeBlockGoal.tick`: three draws (x, y, z) in the box, the block must be holdable and reachable.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let p = w.get::<Pos>(e).unwrap().0;
    let tag = w.get::<Taking>(e).unwrap().holdable;
    let (u0, u1, u2) = with_rng(w, e, |r| (r.next_f64(), r.next_f64(), r.next_f64()));
    let x = (p.x - 2.0 + u0 * 4.0).floor() as i32;
    let y = (p.y + u1 * 3.0).floor() as i32;
    let z = (p.z - 2.0 + u2 * 4.0).floor() as i32;
    let name = block_at(w, x, y, z);
    let mw = w.resource::<Level>().mw.0;
    if mw.has_tag(tag, name) {
        // Reachable: the outline clip from the column's centre at the block's height hits this block first.
        let from = Vec3::new(p.x.floor() + 0.5, f64::from(y) + 0.5, p.z.floor() + 0.5);
        let to = Vec3::new(f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5);
        if !mw.outline_hit(from, to).is_some_and(|hit| hit != [x, y, z]) {
            w.resource_mut::<BlockChanges>().0.push((x, y, z, "minecraft:air"));
            w.get_mut::<Carried>(e).unwrap().0 = Some(name);
            crate::behaviours::leave_block::arm_leave(w, e);
        }
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
/// `stop`: the draw is armed again while the enderman carries nothing.
fn stop(w: &mut World, e: Entity, _: Inst) {
    arm_take(w, e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 1, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// A cow body renamed enderman, whose only goal is the taking.
    fn enderman(h: &mut Herd) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:enderman";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        h.world.flush();
        e
    }
    fn carried(h: &Herd, e: Entity) -> Option<&'static str> {
        h.world.get::<Carried>(e).unwrap().0
    }

    /// With a holdable block one step away (dirt), the enderman takes it within a few hundred asks; with stone there, the
    /// same enderman takes nothing in the same number of asks (the control of the same test).
    #[test]
    fn an_enderman_takes_the_holdable_block_it_can_reach_and_not_the_other() {
        let Some(mut h) = herd() else { return };
        let e = enderman(&mut h);
        h.tick();
        let p = h.world.get::<Pos>(e).unwrap().0;
        let (x, y, z) = (p.x.floor() as i32 - 1, p.y.floor() as i32 + 1, p.z.floor() as i32);
        h.set_block(x, y, z, "minecraft:stone");
        for _ in 0..600 {
            crate::goals::fire(&mut h.world, e, &MODULE);
            h.tick();
        }
        assert_eq!(carried(&h, e), None, "stone is not holdable: nothing taken");
        h.set_block(x, y, z, "minecraft:dirt");
        for _ in 0..600 {
            crate::goals::fire(&mut h.world, e, &MODULE);
            h.tick();
            if carried(&h, e).is_some() {
                break;
            }
        }
        assert_eq!(carried(&h, e), Some("minecraft:dirt"), "dirt in reach is taken");
    }
}
