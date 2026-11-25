//! `Ghast.RandomFloatAroundGoal`: a ghast with nowhere to float to (no wanted spot, or the spot reached or too far) picks a point within
//! 16 blocks on each axis and wants to go there; its move control (`locomotion/fly.rs`, control `Float`) pushes it. The goal asks again by
//! EVENT: a system of the module looks at the wanted spot each tick of the ghast's flight (a flier does not sleep) and wakes the goal when
//! the spot is reached, too far, or dropped.
//! `blocks` (`HappyGhast.RandomFloatAroundGoal`, `FloatAround { blocks }`, `happy_ghast.rs:453-540`): a point is kept only when it is air
//! with a solid block within `blocks - 1` cells in one of the six directions; `0` keeps any point (the ghast). When no try is good, the last
//! point drawn is kept, mirrored below the ghast when the MOTION_BLOCKING surface of its column lies under it (`y = mob.y - |mob.y - y|`).
use crate::IFloor as _;
use super::*;
use crate::rules::float_around as rule;
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub blocks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { blocks: 0 };
}
/// The `blocks` of the entry (`0`: no filter).
#[derive(Component, Clone, Copy, Debug)]
pub struct FloatBlocks(pub i32);

pub static MODULE: Module = Module {
    name: "float_around",
    flags: MOVE,
    equip,
    arm,
    can_use,
    start,
    tick: Some(watch),
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(FloatBlocks(params::<Params>(p).blocks));
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    crate::goals::arm(w, e, inst.slot, 1);
}
/// `!hasWanted() || distance to it outside 1..=3600 (squared)`.
fn wants_a_spot(m: &MoveTo, p: horizon_core::geometry::Vec3) -> bool {
    !m.active || rule::wants_spot([m.x, m.y, m.z], p)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let (Some(m), Some(p)) = (w.get::<MoveTo>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { return false };
    wants_a_spot(&m, p)
}
/// `FloatAround.good`: with `blocks > 0`, the cell is air and one of the six directions holds a solid block within `blocks - 1` cells.
fn good(w: &World, blocks: i32, t: [f64; 3]) -> bool {
    if blocks <= 0 {
        return true;
    }
    let mw = w.resource::<Level>().mw.0;
    let at = [t[0].ifloor(), t[1].ifloor(), t[2].ifloor()];
    let mut cursor = myriade_level::world::BlockCursor::new(mw.world);
    if !mw.facts.is_air(cursor.block(at[0], at[1], at[2])) {
        return false;
    }
    [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]].into_iter().any(|d| {
        (1..blocks).any(|i| !mw.facts.is_air(cursor.block(at[0] + d[0] * i, at[1] + d[1] * i, at[2] + d[2] * i)))
    })
}
/// `getSuitableFlyToPosition`: the first of 64 random points within 16 blocks that is `good`; failing that, the last point drawn.
fn start(w: &mut World, e: Entity, _: Inst) {
    let p = w.get::<Pos>(e).unwrap().0;
    let blocks = w.get::<FloatBlocks>(e).map_or(0, |b| b.0);
    let mut t = [p.x, p.y, p.z];
    let mut found = blocks <= 0;
    for _ in 0..64 {
        t = with_rng(w, e, |r| {
            let mut f = || rule::spot_offset(r.next_float());
            [p.x + f(), p.y + f(), p.z + f()]
        });
        if good(w, blocks, t) {
            found = true;
            break;
        }
    }
    if !found {
        let mw = w.resource::<Level>().mw.0;
        let surface = f64::from(mw.motion_blocking_height(t[0].ifloor(), t[2].ifloor()));
        if surface < t[1] && t[1] > f64::from(mw.world.bottom_y()) {
            t[1] = p.y - (p.y - t[1]).abs();
        }
    }
    *w.get_mut::<MoveTo>(e).unwrap() = MoveTo { active: true, x: t[0], y: t[1], z: t[2], speed_modifier: 1.0 };
    // `canContinueToUse` is false (the default of `Goal`): the goal ends the tick it starts, and is asked again as soon as the spot is gone.
    end(w, e, &MODULE);
}
/// The ghast is asked again when its wanted spot is gone (the tick of the module: only the animals with the float control, read through a kept query).
fn watch(w: &mut World) {
    type Q = bevy_ecs::query::QueryState<(Entity, &'static MoveTo, &'static Pos, &'static GoalSet), With<crate::locomotion::fly::FloatState>>;
    #[derive(Resource)]
    struct Kept(Q);
    if !w.contains_resource::<Kept>() {
        let q = w.query_filtered();
        w.insert_resource(Kept(q));
    }
    // One pass over the kept query, the request pushed in place (no list of entities, no lookup per component).
    w.resource_scope(|w, mut k: Mut<Kept>| {
        let mut asks: Vec<(Entity, u8)> = Vec::new();
        for (e, m, p, g) in k.0.iter(w) {
            if wants_a_spot(m, p.0)
                && let Some(slot) = g.slot_of(&MODULE)
            {
                asks.push((e, slot));
            }
        }
        if !asks.is_empty() {
            let mut r = w.resource_mut::<Requests>();
            r.0.extend(asks.into_iter().map(|(e, s)| Req::Start(e, s)));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config(blocks: i32) -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 5, Box::leak(Box::new(Params { blocks })))]));
        Config { entries }
    }
    /// A ghast (the flier this module belongs to) with this goal alone, in the air over the slab; `None` without the test resources.
    fn world(blocks: i32) -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:ghast").unwrap();
        sp.goals = config(blocks);
        let b = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(b).unwrap().0 = u64::MAX;
        let p = h.world.get::<Pos>(b).unwrap().0;
        h.teleport(b, horizon_core::geometry::Vec3::new(p.x, p.y + 8.0, p.z));
        crate::goals::arm_all(&mut h.world);
        h.tick();
        Some((h, b))
    }
    /// Independent reading of `FloatAround.good` on the test world: air at the point, and a solid block one to `blocks - 1` cells away.
    fn good_by_hand(h: &Herd, t: [f64; 3], blocks: i32) -> bool {
        if blocks <= 0 {
            return true;
        }
        let mw = h.world.resource::<Level>().mw.0;
        let at = [t[0].ifloor(), t[1].ifloor(), t[2].ifloor()];
        let solid = |q: [i32; 3]| !mw.facts.is_air(mw.world.block(q[0], q[1], q[2]));
        if solid(at) {
            return false;
        }
        [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]]
            .iter()
            .any(|d| (1..blocks).any(|i| solid([at[0] + d[0] * i, at[1] + d[1] * i, at[2] + d[2] * i])))
    }
    /// Forty asks for a spot (each after the goal is ended), the spots drawn by `start`: with `blocks = 3` every one is good; with `blocks = 0` some are not
    /// (the filter is what keeps them).
    fn samples(blocks: i32) -> Option<Vec<[f64; 3]>> {
        let (mut h, b) = world(blocks)?;
        let mut out = Vec::new();
        for _ in 0..40 {
            h.world.get_mut::<MoveTo>(b).unwrap().active = false;
            crate::goals::end(&mut h.world, b, &MODULE);
            for _ in 0..4 {
                h.tick();
            }
            let m = *h.world.get::<MoveTo>(b).unwrap();
            if m.active {
                out.push([m.x, m.y, m.z]);
            }
        }
        Some(out)
    }
    /// The wanted spots of `blocks = 3` are all good, and of `blocks = 0` at least one is not (so the test sees the filter work).
    #[test]
    fn a_wanted_spot_is_kept_only_next_to_a_solid_block_when_blocks_are_given() {
        let Some(with) = samples(3) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        assert!(with.len() >= 10, "the ghast asked for spots: {}", with.len());
        let (h, _) = world(3).unwrap();
        assert!(with.iter().all(|t| good_by_hand(&h, *t, 3)));
        let Some(without) = samples(0) else { return };
        let (h0, _) = world(0).unwrap();
        assert!(without.iter().any(|t| !good_by_hand(&h0, *t, 3)), "no free spot was refused: the witness is lost");
    }
}
