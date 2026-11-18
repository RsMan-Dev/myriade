//! `LivingEntity.onClimbable` for a cow (`mobs/climbing.rs`).
use super::view::CowItem;
use crate::mobs::MobWorld;
use horizon_core::position::BlockPos;

impl CowItem<'_, '_> {
    pub fn on_climbable(&self, w: &MobWorld) -> bool {
        let (x, y, z) = (
            self.motion.position.x.floor() as i32,
            self.motion.position.y.floor() as i32,
            self.motion.position.z.floor() as i32,
        );
        let state = w.world.block(x, y, z);
        let facts = w.facts.get(state);
        let mut ladder = w.has_tag("minecraft:climbable", &facts.name);
        crate::motion::native_is_ladder(w.loop_context, self.id.id, BlockPos::new(x, y, z), state, &mut ladder);
        if ladder {
            return true;
        }
        if !facts.has_class("TrapDoorBlock") || w.prop(x, y, z, "open") != Some("true") {
            return false;
        }
        let mut climbable = w.block_name(x, y - 1, z) == "minecraft:ladder"
            && w.prop(x, y - 1, z, "facing") == w.prop(x, y, z, "facing");
        crate::motion::ladder::native_makes_open_trapdoor_above_climbable(
            w.loop_context,
            w.world.block(x, y - 1, z),
            BlockPos::new(x, y - 1, z),
            state,
            &mut climbable,
        );
        climbable
    }
}
