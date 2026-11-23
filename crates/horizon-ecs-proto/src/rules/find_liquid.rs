//! `FindLiquid` (`brain/behaviours/find_liquid.rs`): the timers of the search, the reach of a water block, and the water scan's cut.
use myriade_engine_lib::blocks::Fluid;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// The ticks added when a start happens before `until` (the search is not made again yet).
pub const UNTIL_GAP: u64 = 22;
/// The ticks until the next search.
pub const SEARCH_GAP: u64 = 40;
/// The scan's Manhattan radius.
pub const RADIUS: i32 = 18;

/// Whether the water block at `q` is far enough from `me` to be a target without air above: squared distance of 2.25 or more.
pub fn far_enough(q: [i32; 3], me: Vec3) -> bool {
    let d = [f64::from(q[0]) + 0.5 - me.x, f64::from(q[1]) + 0.5 - me.y, f64::from(q[2]) + 0.5 - me.z];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] >= 2.25
}

/// Whether the scan visits `q` at all: the entity's own column (same x and z) is skipped.
pub fn is_scanned(q: [i32; 3], p: [i32; 3]) -> bool {
    !(q[0] == p[0] && q[2] == p[2])
}

/// Whether a section of the 13-block box around `p` has a water state in its palette: where none has, the scan cannot find a block.
pub fn may_hold_water(mw: &MobWorld, p: [i32; 3]) -> bool {
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
