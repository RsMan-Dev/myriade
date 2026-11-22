//! `EndermanTakeBlockGoal`: an enderman that carries nothing, with `mobGriefing`, takes now and then the block it looks at
//! in a 4 x 3 x 4 box around it, if the block is in the holdable tag and reachable from its column (the outline clip).
//! One chance in `asks` (`reducedTickDelay(20)`) per ask: the draw is a deadline of the wheel, armed while the enderman
//! carries nothing. The goal ticks once, at its start (as the `Mob`, which has no `canContinueToUse`): the block goes
//! (air through `BlockChanges`), the block is carried, and the drop goal is armed.
use super::*;
use crate::{cow::*, goals::*};
use crate::rules::take_block::{reach_line, take_cell, take_wait};
use horizon_client_lib::mobs::MobWorld;
use horizon_world::registry::BlockStateId;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `reducedTickDelay(20)`: one chance in this many per ask.
    pub asks: i32,
    /// The block tag of the blocks an enderman may take.
    pub holdable: &'static str,
    /// The block keeps its properties when taken (`enderman.rs:30`: the `BlockState` is carried whole).
    pub keep_props: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { asks: 10, holdable: "minecraft:enderman_holdable", keep_props: true };
}
/// The block the enderman carries, by name (`None`: nothing).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Carried(pub Option<&'static str>);
/// The properties the carried block had when taken (`'static`: `blocks::set_state` takes them so); empty when not kept.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CarriedProps(pub &'static [(&'static str, &'static str)]);
/// The draw of this goal: its parameters and whether its deadline is in the wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Taking {
    pub asks: i32,
    pub holdable: &'static str,
    pub keep_props: bool,
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
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Taking { asks: p.asks, holdable: p.holdable, keep_props: p.keep_props, armed: false, slot: inst.slot });
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
    let wait = with_rng(w, e, |r| take_wait(geometric(r, t.asks)));
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
/// The properties of the block state `id` as `'static` pairs, read from the registry (which lives as long as the world).
/// `blocks::set_state` takes `'static` properties: each state is copied once and kept, so the set kept is bounded by the
/// block states (DOUBT: the framework should take the state's id or owned properties).
fn held_props(mw: &'static MobWorld<'static>, id: BlockStateId) -> &'static [(&'static str, &'static str)] {
    static KEPT: Mutex<BTreeMap<BlockStateId, &'static [(&'static str, &'static str)]>> = Mutex::new(BTreeMap::new());
    let mut kept = KEPT.lock().unwrap_or_else(|poison| poison.into_inner());
    kept.entry(id).or_insert_with(|| {
        let pairs: Vec<(&'static str, &'static str)> = mw
            .registry
            .state(id)
            .into_iter()
            .flat_map(|s| s.properties.iter())
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        Box::leak(pairs.into_boxed_slice())
    })
}
/// `EndermanTakeBlockGoal.tick`: three draws (x, y, z) in the box, the block must be holdable and reachable.
fn start(w: &mut World, e: Entity, inst: Inst) {
    let p = w.get::<Pos>(e).unwrap().0;
    let (tag, keep_props) = {
        let t = w.get::<Taking>(e).unwrap();
        (t.holdable, t.keep_props)
    };
    let (u0, u1, u2) = with_rng(w, e, |r| (r.next_f64(), r.next_f64(), r.next_f64()));
    let [x, y, z] = take_cell(p, [u0, u1, u2]);
    let mw = w.resource::<Level>().mw.0;
    let id = mw.world.block(x, y, z);
    let name: &'static str = &mw.facts.get(id).name;
    if mw.has_tag(tag, name) {
        // Reachable: the outline clip from the column's centre at the block's height hits this block first.
        let (from, to) = reach_line(p, [x, y, z]);
        if !mw.outline_hit(from, to).is_some_and(|hit| hit != [x, y, z]) {
            w.resource_mut::<BlockChanges>().0.push((x, y, z, "minecraft:air"));
            let props = if keep_props { held_props(mw, id) } else { &[] };
            w.get_mut::<Carried>(e).unwrap().0 = Some(name);
            w.entity_mut(e).insert(CarriedProps(props));
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
    use crate::blocks;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 1, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
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
    /// The properties are kept: a grass block with `snowy=true` is taken with that property (the control: dirt has none).
    #[test]
    fn a_taken_block_keeps_its_properties() {
        let Some(mut h) = herd() else { return };
        let e = enderman(&mut h);
        h.tick();
        let p = h.world.get::<Pos>(e).unwrap().0;
        let (x, y, z) = (p.x.floor() as i32 - 1, p.y.floor() as i32 + 1, p.z.floor() as i32);
        blocks::set_state(&mut h.world, [x, y, z], "minecraft:grass_block", &[("snowy", "true")]);
        h.tick();
        assert_eq!(blocks::property(blocks::world_of(&h.world), [x, y, z], "snowy").as_deref(), Some("true"), "the grass block is not placed with snowy=true");
        for _ in 0..600 {
            crate::goals::fire(&mut h.world, e, &MODULE);
            h.tick();
            if carried(&h, e).is_some() {
                break;
            }
        }
        assert_eq!(carried(&h, e), Some("minecraft:grass_block"), "the grass block in reach is not taken");
        let props = h.world.get::<CarriedProps>(e).unwrap().0;
        assert_eq!(props, &[("snowy", "true")][..], "the taken block lost its properties");
    }
}
