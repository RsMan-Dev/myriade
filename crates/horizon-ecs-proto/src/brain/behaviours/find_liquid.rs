//! `FindLiquid` (`AxolotlAction::FindLiquid`, `animal_brain.rs:2514-2572`, a one-shot of the idle activity): out of water, with no walk
//! target and no attack target, it looks (at most every 40 ticks: `until`) in the Manhattan ball of radius 18 (x, y in -6..=6, z in
//! 0..=6 both ways, in the `Mob`'s order) for a water block with air above, or failing that a water block farther than 1.5 blocks, and walks
//! to it at `speed`, looking at it. A start while `until` has not passed only pushes `until` 22 ticks on (the `Mob` runs it every tick;
//! here it runs when the brain is woken: the same pushes happen only at those wakes).
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::{FluidState, Level, Pos};
use horizon_client_lib::blocks::Fluid;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:find_liquid",
    mems: &[&WALK_TARGET, &LOOK_TARGET, &ATTACK_TARGET],
    requires: &[Cap::Body, Cap::Brain],
    one_shot: true,
    equip,
    can_start,
    ..Behaviour::DEFAULT
};

pub struct Params {
    pub speed: f32,
}
impl Params {
    pub const DEFAULT: Params = Params { speed: 0.15 };
}
/// The own tick before which the search is not made again.
#[derive(Component, Clone, Copy, Default)]
struct Until(u64);
fn equip(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).insert(Until(0));
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) || present(w, e, &ATTACK_TARGET) {
        return false;
    }
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    if w.get::<FluidState>(e).is_some_and(|f| f.in_water) {
        return false;
    }
    let level = *w.resource::<Level>();
    let mw = level.mw.0;
    let p = [me.x.floor() as i32, me.y.floor() as i32, me.z.floor() as i32];
    if mw.fluid(p[0], p[1], p[2]) == Fluid::Water {
        return false;
    }
    let now = now_of(w, e);
    let until = w.get::<Until>(e).map_or(0, |u| u.0);
    if now < until {
        w.entity_mut(e).insert(Until(now + 22));
        return true;
    }
    let mut found: Option<[i32; 3]> = None;
    let mut water = Column::default();
    // `BlockPos.withinManhattan`: by distance, then x, then y, then +z before -z.
    'search: for distance in 0..=18i32 {
        if distance == 0 && !may_hold_water(mw, p) {
            break;
        }
        for x in -6..=6i32 {
            for y in -6..=6i32 {
                let z = distance - x.abs() - y.abs();
                if !(0..=6).contains(&z) {
                    continue;
                }
                for q in [Some([p[0] + x, p[1] + y, p[2] + z]), (z != 0).then_some([p[0] + x, p[1] + y, p[2] - z])].into_iter().flatten() {
                    if (q[0] == p[0] && q[2] == p[2]) || water.at(mw, q) != Fluid::Water || mw.block_name(q[0], q[1], q[2]) != "minecraft:water" {
                        continue;
                    }
                    if mw.block_name(q[0], q[1] + 1, q[2]).ends_with("air") {
                        found = Some(q);
                        break 'search;
                    }
                    let d = [f64::from(q[0]) + 0.5 - me.x, f64::from(q[1]) + 0.5 - me.y, f64::from(q[2]) + 0.5 - me.z];
                    if found.is_none() && d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= 2.25 {
                        found = Some(q);
                    }
                }
            }
        }
    }
    if let Some(q) = found {
        set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Block(q)), None);
        set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Block(q), speed: c.get::<Params>().speed, close: 0 }), None);
    }
    w.entity_mut(e).insert(Until(now + 40));
    true
}

/// Whether a section of the 13-block box around `p` has a water state in its palette: where none has, the scan cannot find a block.
fn may_hold_water(mw: &horizon_client_lib::mobs::MobWorld, p: [i32; 3]) -> bool {
    for cz in (p[2] - 6) >> 4..=(p[2] + 6) >> 4 {
        for cx in (p[0] - 6) >> 4..=(p[0] + 6) >> 4 {
            let Some(chunk) = mw.world.chunks.get(&(cx, cz)) else { continue };
            for cy in (p[1] - 6) >> 4..=(p[1] + 6) >> 4 {
                let Some(sec) = chunk.section_holding(cy << 4) else { continue };
                let wet = |id: u32| mw.facts.get(horizon_world::registry::BlockStateId(id)).fluid == Fluid::Water;
                match sec.blocks.palette() {
                    Some(list) => {
                        if list.iter().any(|&id| wet(id)) {
                            return true;
                        }
                    }
                    None if sec.blocks.is_global() => return true,
                    None => {
                        if let Some(v) = sec.blocks.uniform() && wet(v) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// The fluid at a cell, the column's chunk kept between reads (the scan walks a 13-block box: a handful of chunks).
#[derive(Default)]
struct Column {
    key: Option<(i32, i32)>,
    chunk: Option<std::sync::Arc<horizon_client_render::chunk::Chunk>>,
}
impl Column {
    fn at(&mut self, mw: &horizon_client_lib::mobs::MobWorld, q: [i32; 3]) -> Fluid {
        let key = (q[0] >> 4, q[2] >> 4);
        if self.key != Some(key) {
            self.key = Some(key);
            self.chunk = mw.world.chunks.get(&key).cloned();
        }
        match &self.chunk {
            Some(c) if c.section_index(q[1]).is_some() => mw.facts.get(c.block(q[0], q[1], q[2])).fluid,
            _ => Fluid::None,
        }
    }
}
