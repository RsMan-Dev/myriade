//! `EndermanLeaveBlockGoal`: an enderman that carries a block, with `mobGriefing`, sets it down now and then (one chance in
//! `asks`, `reducedTickDelay(2000)`, per ask): the draw is a deadline of the wheel, armed while it carries. One attempt
//! per start, at a cell of the 2 x 2 x 2 box around it: the block is set down when the cell is air, the block below is
//! solid and no other entity is in the cell; then the carried block is gone and the take goal is armed again.
//! The game's own rules of placement (`canSurvive`, the placement event) are not modelled: DOUBT.
use super::*;
use crate::behaviours::take_block::{Carried, CarriedProps};
use crate::{blocks, cow::*, goals::*};
use crate::rules::leave_block::{cell_box, cell_of, due_at, leave_wait};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `reducedTickDelay(2000)`: one chance in this many per ask.
    pub asks: i32,
    /// The names of the blocks that do not stay where the enderman sets them (the game's `canSurvive`, absent: DOUBT). Empty
    /// by default: the list is not in the `Mob` (its refusal is the game's, `EndermanPlaceBlock`), so no name is invented.
    pub fragile: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Self = Self { asks: 1000, fragile: &[] };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Leaving {
    pub asks: i32,
    pub armed: bool,
    pub slot: u8,
    pub fragile: &'static [&'static str],
    /// The entity time at which the drawn wait ends (the wheel holds at most `WHEEL_MAX` = 4 095 ticks: a longer wait is
    /// armed in pieces, a deadline before `due` is not the draw's).
    pub due: u64,
}

pub static MODULE: Module = Module {
    name: "leave_block",
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
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Leaving { asks: p.asks, armed: false, slot: inst.slot, due: 0, fragile: p.fragile });
}
/// Arms the next draw while the enderman carries a block and the rule allows it.
pub fn arm_leave(w: &mut World, e: Entity) {
    let Some(l) = w.get::<Leaving>(e).copied() else { return };
    if l.armed || w.get::<Carried>(e).is_none_or(|c| c.0.is_none()) || !mob_griefing(w) {
        return;
    }
    let wait = with_rng(w, e, |r| leave_wait(geometric(r, l.asks)));
    crate::goals::arm(w, e, l.slot, wait);
    let due = due_at(now_of(w, e), wait);
    let mut l = w.get_mut::<Leaving>(e).unwrap();
    l.armed = true;
    l.due = due;
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    arm_leave(w, e);
}
/// A deadline of this goal, or a replayed start: always asks `can_use`.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let Some(l) = w.get::<Leaving>(e).copied() else { return true };
    // A wait longer than the wheel's reach comes up in pieces: the rest is armed again, nothing is asked.
    if l.armed && now < l.due {
        crate::goals::arm(w, e, l.slot, (l.due - now) as i32);
        return false;
    }
    w.get_mut::<Leaving>(e).unwrap().armed = false;
    true
}
/// `EndermanLeaveBlockGoal.canUse` after the draw: carries a block, and `mobGriefing`.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Carried>(e).is_some_and(|c| c.0.is_some()) && mob_griefing(w)
}
/// `EndermanLeaveBlockGoal.tick`: three draws (x, y, z) in the box; the block is set down when the cell is air, solid
/// below and free of other entities (the enderman itself excluded).
fn start(w: &mut World, e: Entity, inst: Inst) {
    let p = w.get::<Pos>(e).unwrap().0;
    let Some(name) = w.get::<Carried>(e).and_then(|c| c.0) else {
        w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
        return;
    };
    let (u0, u1, u2) = with_rng(w, e, |r| (r.next_f64(), r.next_f64(), r.next_f64()));
    let [x, y, z] = cell_of(p, [u0, u1, u2]);
    let (air, solid_below) = {
        let mw = w.resource::<Level>().mw.0;
        (&*mw.facts.get(mw.world.block(x, y, z)).name == "minecraft:air", mw.solid(x, y - 1, z))
    };
    let cell = cell_box([x, y, z]);
    let mut occupied = false;
    let mut found = Vec::new();
    w.resource::<Index>().for_radius(p.x, p.z, 2.0, |c| found.push(c));
    for c in found {
        if c == e {
            continue;
        }
        if let Some(b) = w.get::<Bounds>(c)
            && b.0.intersects(cell)
        {
            occupied = true;
        }
    }
    let fragile = w.get::<Leaving>(e).unwrap().fragile;
    if air && solid_below && !occupied && !fragile.contains(&name) {
        let props = w.get::<CarriedProps>(e).map_or(&[][..], |p| p.0);
        blocks::set_state(w, [x, y, z], name, props);
        w.get_mut::<Carried>(e).unwrap().0 = None;
        crate::behaviours::take_block::arm_take(w, e);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
/// `stop`: the draw is armed again while the enderman still carries a block.
fn stop(w: &mut World, e: Entity, _: Inst) {
    arm_leave(w, e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 1, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// A cow body renamed enderman, whose only goal is the leaving, carrying `block`.
    fn enderman(h: &mut Herd, block: Option<&'static str>, props: &'static [(&'static str, &'static str)]) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:enderman";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        h.world.flush();
        h.world.entity_mut(e).insert((Carried(block), CarriedProps(props)));
        e
    }

    /// An enderman carrying dirt on open ground sets it down when its draw comes up: the block is placed in its box and the
    /// carried block is gone (the control: when it carries nothing, the same draw places nothing).
    #[test]
    fn a_carrying_enderman_sets_its_block_down_when_the_draw_comes_up() {
        let Some(mut h) = herd() else { return };
        let e = enderman(&mut h, None, &[]);
        h.tick();
        crate::goals::fire(&mut h.world, e, &MODULE);
        h.tick();
        assert!(!h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE), "nothing carried: no start");
        h.world.get_mut::<Carried>(e).unwrap().0 = Some("minecraft:dirt");
        for _ in 0..200 {
            crate::goals::fire(&mut h.world, e, &MODULE);
            h.tick();
            if h.world.get::<Carried>(e).unwrap().0.is_none() {
                break;
            }
        }
        assert!(h.world.get::<Carried>(e).unwrap().0.is_none(), "the carried dirt is set down within 200 draws");
    }

    /// The block is set down with the properties it was carried with: a grass block with `snowy=true` is found in the box
    /// around the enderman with that property (a grass block without it would not match).
    #[test]
    fn a_set_down_block_keeps_its_properties() {
        let Some(mut h) = herd() else { return };
        let e = enderman(&mut h, Some("minecraft:grass_block"), &[("snowy", "true")]);
        h.tick();
        let p = h.world.get::<Pos>(e).unwrap().0;
        for _ in 0..200 {
            crate::goals::fire(&mut h.world, e, &MODULE);
            h.tick();
            if h.world.get::<Carried>(e).unwrap().0.is_none() {
                break;
            }
        }
        assert!(h.world.get::<Carried>(e).unwrap().0.is_none(), "the grass block is not set down within 200 draws");
        h.tick();
        let (fx, fy, fz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let mw = blocks::world_of(&h.world);
        let snowy_grass = (fx - 1..=fx + 1).flat_map(|x| (fy..=fy + 2).flat_map(move |y| (fz - 1..=fz + 1).map(move |z| [x, y, z]))).any(|c| {
            &*mw.facts.get(mw.world.block(c[0], c[1], c[2])).name == "minecraft:grass_block" && blocks::property(mw, c, "snowy").as_deref() == Some("true")
        });
        assert!(snowy_grass, "no grass block with snowy=true was set down in the box");
    }
}
