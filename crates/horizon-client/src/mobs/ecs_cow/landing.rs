//! A cow landing on a block: that block's `fallOn` rule (`mobs/landing.rs`, for a cow). Sounds,
//! particles and game events have no listener in this prototype; block writes are not applied.
use super::{event::CowEvent, view::CowItem};
use crate::{
    collisions::SceneCollisions,
    health::DamageLevel,
    mobs::{DamageKind, MobWorld},
};
use horizon_core::{
    geometry::{Aabb, Vec3, VoxelShape},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::{
    block_entity_push::BlockPushWorld,
    block_landing::{BlockLandingWorld, DamageSourceOwner, LandingDamage, LandingEvent, LandingSound},
    entity_obstruction::EntityIdentity,
    registry::BlockStateId,
};

pub(super) struct CowLanding<'a, 'b, 'c, 'w, 'l> {
    pub cow: &'a mut CowItem<'b, 'c>,
    pub w: &'a MobWorld<'w>,
    pub level: &'a mut DamageLevel<'l>,
    pub events: &'a mut Vec<CowEvent>,
    pub pos: [i32; 3],
}
impl BlockPushWorld for CowLanding<'_, '_, '_, '_, '_> {
    type Error = String;
    fn collision_shape(&mut self, state: BlockStateId, p: BlockPos) -> Result<VoxelShape, String> {
        let mut scene = SceneCollisions { world: self.w.world, binding: self.w.collision, subject: None };
        self.w
            .collision
            .collision_shape(&mut scene, state, [p.x, p.y, p.z])
            .map_err(|e| e.to_string())
    }
    fn entities(&mut self, bounds: Aabb) -> Result<Vec<EntityIdentity>, String> {
        Ok(if bounds.intersects(self.cow.bounds()) { vec![EntityIdentity(0)] } else { vec![] })
    }
    fn entity_bounds(&self, _: EntityIdentity) -> Aabb {
        self.cow.bounds()
    }
    fn teleport_relative(&mut self, _: EntityIdentity, m: Vec3) -> Result<(), String> {
        let walked = self.cow.motion.position;
        self.cow.motion.position = Vec3::new(walked.x + m.x, walked.y + m.y, walked.z + m.z);
        Ok(())
    }
}
impl BlockLandingWorld for CowLanding<'_, '_, '_, '_, '_> {
    type DamageSource = DamageKind;
    type Sound = Option<&'static str>;
    fn is_server_level(&self) -> bool {
        true
    }
    fn is_client_side(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    fn random_float(&mut self) -> Result<f32, String> {
        Ok(self.cow.random.next_float())
    }
    fn random_int(&mut self, bound: i32) -> Result<i32, String> {
        self.cow.random.next_int_bound(bound).map_err(|e| e.to_string())
    }
    fn under_spawn_protection(&mut self, _: BlockPos) -> Result<bool, String> {
        Ok(false)
    }
    fn mob_griefing(&mut self) -> Result<bool, String> {
        Ok(self.w.can_grief(self.cow.id.id))
    }
    fn entity_dimensions(&self) -> (f32, f32) {
        (self.cow.width(), self.cow.height())
    }
    fn cancel_farmland_trample(&mut self, position: BlockPos, state: BlockStateId, distance: f64) -> bool {
        let mut cancel = false;
        crate::landing::native_farmland_trample(
            self.w.loop_context,
            self.cow.id.id,
            [position.x, position.y, position.z],
            state.0,
            distance,
            &mut cancel,
        );
        cancel
    }
    fn suppressing_bounce(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    fn damage_source(&mut self, _: DamageSourceOwner, kind: LandingDamage) -> Result<DamageKind, String> {
        Ok(match kind {
            LandingDamage::Fall => DamageKind::Fall,
            LandingDamage::Stalagmite => DamageKind::Stalagmite,
        })
    }
    fn cause_fall_damage(&mut self, distance: f64, multiplier: f32, source: DamageKind) -> Result<bool, String> {
        let contact = Some((self.w, self.pos));
        Ok(self.cow.cause_fall_damage_at(
            self.w.loop_context,
            self.level,
            distance,
            multiplier,
            self.events,
            contact,
            source,
        ))
    }
    fn known_sound(&self, sound: LandingSound) -> Option<&'static str> {
        match sound {
            LandingSound::HoneySlide => Some("block.honey_block.slide"),
            LandingSound::HoneyFall => Some("block.honey_block.fall"),
            LandingSound::TurtleEggBreak => Some("entity.turtle.egg_break"),
            LandingSound::ShelfMushroomBounce => None,
        }
    }
    fn living_fall_sound(&mut self, big: bool) -> Result<Option<&'static str>, String> {
        Ok(Some(if big { "entity.generic.big_fall" } else { "entity.generic.small_fall" }))
    }
    fn entity_sound(&mut self, _: Option<&'static str>, _: f32, _: f32) -> Result<(), String> {
        Ok(())
    }
    fn block_sound(&mut self, _: BlockPos, _: Option<&'static str>, _: f32, _: f32) -> Result<(), String> {
        Ok(())
    }
    fn broadcast_entity_event(&mut self, _: u8) -> Result<(), String> {
        Ok(())
    }
    fn destroy_block(&mut self, _: BlockPos, _drops: bool) -> Result<bool, String> {
        Ok(true)
    }
    fn set_block(&mut self, _: BlockPos, _: BlockStateId, _: u32) -> Result<bool, String> {
        Ok(true)
    }
    fn block_event(&mut self, _: LandingEvent, _: BlockPos, _: BlockStateId, _: bool) -> Result<(), String> {
        Ok(())
    }
    fn level_event(&mut self, _: i32, _: BlockPos, _: i32) -> Result<(), String> {
        Ok(())
    }
}
