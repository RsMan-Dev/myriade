//! Landing impacts for falling blocks, before their placement/drop decision,
//! and `Block.fallOn` for every entity that isn't living.
use super::Game;
use horizon_core::{
    geometry::{Aabb, Vec3, VoxelShape},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::{
    block_entity_push::BlockPushWorld,
    block_landing::{
        BlockLandingWorld, DamageSourceOwner, LandingDamage, LandingEntityKind, LandingEvent,
        LandingSound,
    },
    entity_obstruction::EntityIdentity,
    registry::BlockStateId,
};

/// An entity that isn't living, landing (`Entity.checkFallDamage` -> `Block.fallOn`).
/// Only farmland (the level random's draw), honey (the slide sound and entity
/// event 54) and the fall-damage calls reach one: it writes no block.
struct NonLivingLanding<'g, 'a> {
    game: &'g mut Game<'a>,
    id: u32,
    at: Vec3,
    bounds: Aabb,
    /// What `causeFallDamage` was called with: a falling block hurts by it.
    fall_damage: Option<f64>,
}
impl BlockPushWorld for NonLivingLanding<'_, '_> {
    type Error = String;
    fn collision_shape(&mut self, state: BlockStateId, p: BlockPos) -> Result<VoxelShape, String> {
        let mut scene = myriade_engine::collisions::SceneCollisions {
            world: &self.game.world,
            binding: self.game.collision,
            subject: None,
        };
        self.game
            .collision
            .collision_shape(&mut scene, state, [p.x, p.y, p.z])
            .map_err(|e| e.to_string())
    }
    fn entities(&mut self, _: Aabb) -> Result<Vec<EntityIdentity>, String> {
        Ok(Vec::new())
    }
    fn entity_bounds(&self, _: EntityIdentity) -> Aabb {
        self.bounds
    }
    fn teleport_relative(&mut self, _: EntityIdentity, _: Vec3) -> Result<(), String> {
        Ok(())
    }
}
impl BlockLandingWorld for NonLivingLanding<'_, '_> {
    type DamageSource = ();
    type Sound = Option<&'static str>;
    fn is_server_level(&self) -> bool {
        true
    }
    fn is_client_side(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    // `level.getRandom()`.
    fn random_float(&mut self) -> Result<f32, String> {
        Ok(self.game.world.random.next_float())
    }
    fn random_int(&mut self, bound: i32) -> Result<i32, String> {
        self.game
            .world
            .random
            .next_int_bound(bound)
            .map_err(|e| e.to_string())
    }
    fn under_spawn_protection(&mut self, _: BlockPos) -> Result<bool, String> {
        Ok(false)
    }
    fn mob_griefing(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    fn entity_dimensions(&self) -> (f32, f32) {
        let size = self
            .bounds
            .max_position()
            .subtract(self.bounds.min_position());
        (size.x as f32, size.y as f32)
    }
    fn suppressing_bounce(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    fn damage_source(&mut self, _: DamageSourceOwner, _: LandingDamage) -> Result<(), String> {
        Ok(())
    }
    // `Entity.causeFallDamage` is false for all of these; a falling block's
    // override hurts what it lands in (the caller does, by this distance).
    fn cause_fall_damage(&mut self, distance: f64, _: f32, _: ()) -> Result<bool, String> {
        self.fall_damage = Some(distance);
        Ok(false)
    }
    fn known_sound(&self, sound: LandingSound) -> Option<&'static str> {
        match sound {
            LandingSound::HoneySlide => Some("minecraft:block.honey_block.slide"),
            LandingSound::HoneyFall => Some("minecraft:block.honey_block.fall"),
            LandingSound::TurtleEggBreak => Some("minecraft:entity.turtle.egg_break"),
            LandingSound::ShelfMushroomBounce => None,
        }
    }
    fn living_fall_sound(&mut self, _: bool) -> Result<Option<&'static str>, String> {
        Ok(None)
    }
    // `Entity.playSound`: none from a silent entity, on `getSoundSource()` (NEUTRAL).
    fn entity_sound(
        &mut self,
        sound: Option<&'static str>,
        volume: f32,
        pitch: f32,
    ) -> Result<(), String> {
        if let Some(name) = sound
            && !self.game.entity_silent(self.id)
        {
            let at = [self.at.x, self.at.y, self.at.z];
            self.game.audio.play_on(6, name, Some(at), volume, pitch);
        }
        Ok(())
    }
    fn block_sound(
        &mut self,
        _: BlockPos,
        _: Option<&'static str>,
        _: f32,
        _: f32,
    ) -> Result<(), String> {
        Ok(())
    }
    // `Level.broadcastEntityEvent`.
    fn broadcast_entity_event(&mut self, event: u8) -> Result<(), String> {
        let at = [self.at.x, self.at.y, self.at.z];
        self.game.lan_entity_event(self.id as i32, event as i8, at);
        Ok(())
    }
    fn destroy_block(&mut self, _: BlockPos, _: bool) -> Result<bool, String> {
        Ok(false)
    }
    fn set_block(&mut self, _: BlockPos, _: BlockStateId, _: u32) -> Result<bool, String> {
        Ok(false)
    }
    fn block_event(
        &mut self,
        _: LandingEvent,
        _: BlockPos,
        _: BlockStateId,
        _: bool,
    ) -> Result<(), String> {
        Ok(())
    }
    fn level_event(&mut self, _: i32, _: BlockPos, _: i32) -> Result<(), String> {
        Ok(())
    }
}

impl Game<'_> {
    /// `Block.fallOn` for entity `id`, which isn't living, landing at `at`
    /// (its box `bounds`) after falling `fall`, on its supporting block (else
    /// `getOnPosLegacy`). Returns the distance it called `causeFallDamage`
    /// with: none on powder snow, the fall plus 2.5 on a stalagmite's tip,
    /// half on a bed. A client runs none of it.
    pub(super) fn non_living_fall_on(
        &mut self,
        id: u32,
        at: Vec3,
        bounds: Aabb,
        fall: f64,
    ) -> Option<f64> {
        let pos = self.landing_pos(at, bounds);
        let state = self.world.block(pos.x, pos.y, pos.z);
        let motion = self.motion;
        let mut landing = NonLivingLanding {
            game: self,
            id,
            at,
            bounds,
            fall_damage: None,
        };
        if let Err(e) =
            motion
                .landing
                .fall_on(&mut landing, state, pos, LandingEntityKind::Other, fall)
        {
            log::warn!("Horizon: fallOn: {e}");
        }
        landing.fall_damage
    }
    /// `HostEvent::Landed` of a non-living body of P (primed TNT, falling block) that fell `fall`: `fallOn` of the block under it, then HIT_GROUND.
    pub(in crate::game) fn ecs_landed(&mut self, id: u32, at: [f64; 3], fall: f64) {
        let at = horizon_core::geometry::Vec3::new(at[0], at[1], at[2]);
        let bounds = myriade_engine::falling_block::bounds(at);
        self.non_living_fall_on(id, at, bounds, fall);
        self.post_landing_event(at, bounds, false);
    }
    /// The hurt of a player by a falling block (`Hurt` of P): a falling block names no attacker, so none is set.
    pub(in crate::game) fn falling_hurt_player(&mut self, position: Vec3, amount: f32, kind: &str) {
        if self.creative || self.spectator() {
            return;
        }
        if self.player.as_ref().is_none_or(|p| {
            p.health.dead_or_dying() || !myriade_engine::falling_block::bounds(position).intersects(p.bounds())
        }) {
            return;
        }
        let Some(id) = self.health_resources.damage.id(kind) else {
            return;
        };
        let attack = myriade_engine::health::Attack {
            position: Some([position.x, position.y, position.z]),
            ..Default::default()
        };
        let outcome = self
            .hurt_player_equipment(id, amount, None, false, 0, false, &attack, None)
            .unwrap_or_default();
        if outcome.hurt {
            // LivingEntity.hurtServer: ENTITY_HURT_PLAYER, the falling block
            // (`DamageSources.fallingBlock(this)` and the like) as the direct entity.
            let block = myriade_engine::advancements::EntityView {
                kind: "minecraft:falling_block".into(),
                position: [position.x, position.y, position.z],
                ..Default::default()
            };
            self.trigger_entity_hurt_player(&myriade_engine::advancements::DamageView {
                kind: kind.to_owned(),
                direct: Some(block.clone()),
                is_direct: true,
                source: Some(block),
                original: amount,
                actual: outcome.dealt,
                blocked: false,
            });
        }
    }
}
