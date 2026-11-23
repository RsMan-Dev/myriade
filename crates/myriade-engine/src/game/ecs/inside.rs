//! The cells the ECS entities touch: P reports, per moving entity, the sensitive cells its box reaches into (`HostEvent::Inside`); each is
//! handed to `entity_inside` (plates, tripwires, buttons, detector rails), `boat_inside` for a boat (lily pad) and `entity_on_big_dripleaf`
//! for the feet layer. The table of sensitive states is built from the block classes on the first block tick and shared with P by reference.
//! Written, not run.
use crate::game::Game;
use crate::game::block_ticks::standing_feet;
use myriade_engine::ecs_bridge::EcsLevel;
use horizon_core::position::BlockPos;
use horizon_ecs_proto::host_out_inside::InsideEvent;
use horizon_world::level::{Behaviours, classes::Class};
use horizon_world::registry::BlockStateId;
use std::sync::Arc;

/// A bit per state id: the blocks that sense an entity, the lily pad (boats) and the big dripleaf (its feet layer).
fn inside_table(b: &Behaviours) -> Arc<[u64]> {
    let n = b.table.state_count();
    let mut bits = vec![0u64; n.div_ceil(64)];
    for s in 0..n as u32 {
        let id = BlockStateId(s);
        if b.senses_entities(id) || b.data(id).is(Class::LilyPad) || b.data(id).is(Class::BigDripleaf) {
            bits[s as usize / 64] |= 1 << (s % 64);
        }
    }
    bits.into()
}

/// Hands P the table the first time (no-op after).
pub(in crate::game) fn arm(b: &Behaviours, ecs: &mut EcsLevel) {
    if !ecs.has_inside_table() {
        ecs.set_inside_table(inside_table(b));
    }
}

impl Game<'_> {
    /// The blocks an ECS entity touched this tick react: the sensing ones, the lily pad under a boat, the dripleaf under its feet.
    pub(in crate::game) fn ecs_inside_event(&mut self, ev: InsideEvent) {
        self.with_level(|b, level, ticks| {
            for c in &ev.cells {
                let pos = BlockPos::new(c[0], c[1], c[2]);
                if ev.boat {
                    b.boat_inside(level, ticks, pos);
                }
                b.entity_inside(level, ticks, pos);
            }
            let mut feet = Vec::new();
            standing_feet(&mut feet, ev.at, ev.on_ground, ev.half);
            for (pos, ground, y) in feet {
                if ev.cells.contains(&[pos.x, pos.y, pos.z]) {
                    b.entity_on_big_dripleaf(level, ticks, pos, ground, y);
                }
            }
        });
        let _ = self.process_level_events();
    }
}
