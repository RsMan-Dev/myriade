//! `Entity.applyEffectsFromBlocks` for a cow (`ai::effects_from_blocks`): the blocks of this tick's
//! move are walked (`block_contact::sweep_movements`) and the ones that hurt or hold act. Mods'
//! `entity_inside` hooks, effects and the honey block's slide are not ported.
use super::{event::CowEvent, view::CowItem};
use crate::{
    health::DamageLevel,
    mobs::{Damage, DamageKind, MobWorld},
};
use horizon_core::geometry::Vec3;

impl CowItem<'_, '_> {
    pub fn effects_from_blocks(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, events: &mut Vec<CowEvent>) {
        let (tick_from, tick_to) = (self.motion.previous, self.motion.position);
        if self.motion.on_ground && self.alive() {
            let [x, y, z] = self.block_on_legacy(w);
            crate::block_contact::step_on(
                w.block_name(x, y, z),
                crate::block_contact::Contact {
                    entity: self.id.id as i32,
                    kind: &self.id.kind.id,
                    state: w.world.block(x, y, z),
                    position: horizon_core::position::BlockPos::new(x, y, z),
                },
            );
            if w.block_name(x, y, z) == "minecraft:magma_block" {
                self.damage(level, 1.0, Damage::of(DamageKind::HotFloor));
            }
            if w.block_name(x, y, z) == "minecraft:turtle_egg"
                && w.can_grief(self.id.id)
                && level.random().next_int_bound(100).unwrap_or(1) == 0
            {
                // `CrackTurtleEgg`: the egg is not broken in this world (no listener).
            }
        }
        let mut visited = crate::fast_hash::FxHashSet::default();
        let mut effects = mod_api::InsideEffects::default();
        let start = tick_from;
        let movements = [horizon_world::entity_move::MovementRecord { from: start, to: tick_to, original: None }];
        let start_box = self.bounds_at(start).deflate(Vec3::new(1e-5, 1e-5, 1e-5));
        let mut inside = CowInside { cow: self, w, level, events: &mut *events };
        let lo = start_box.min_position();
        let hi = start_box.max_position();
        for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
            for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    let at = horizon_core::position::BlockPos::new(x, y, z);
                    let state = w.world.block(x, y, z);
                    let solid = w.registry.state(state).is_none_or(|d| {
                        !matches!(d.name.as_str(), "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
                    });
                    if solid
                        && visited.insert(at.pack())
                        && let Ok(()) = crate::block_contact::InsideWalk::entity_inside(
                            &mut inside,
                            at,
                            state,
                            0,
                            false,
                            &mut effects,
                        )
                    {}
                }
            }
        }
        let _ = crate::block_contact::sweep_movements(&mut inside, &movements, &mut visited, &mut effects);
    }
}

struct CowInside<'a, 'b, 'c, 'w, 'l> {
    cow: &'a mut CowItem<'b, 'c>,
    w: &'a MobWorld<'w>,
    level: &'a mut DamageLevel<'l>,
    #[allow(dead_code)]
    events: &'a mut Vec<CowEvent>,
}

impl crate::block_contact::InsideWalk for CowInside<'_, '_, '_, '_, '_> {
    type Error = ();
    fn bounds_at(&self, at: Vec3) -> horizon_core::geometry::Aabb {
        self.cow.bounds_at(at)
    }
    fn is_alive(&self) -> bool {
        self.cow.alive()
    }
    fn block(&self, at: horizon_core::position::BlockPos) -> horizon_world::registry::BlockStateId {
        self.w.world.block(at.x, at.y, at.z)
    }
    fn is_air(&self, state: horizon_world::registry::BlockStateId) -> bool {
        self.w.registry.state(state).is_none_or(|d| {
            matches!(d.name.as_str(), "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
        })
    }
    fn inside_shape(
        &mut self,
        _state: horizon_world::registry::BlockStateId,
        _at: horizon_core::position::BlockPos,
    ) -> crate::block_contact::InsideShape {
        crate::block_contact::InsideShape::Full
    }
    fn entity_inside(
        &mut self,
        at: horizon_core::position::BlockPos,
        state: horizon_world::registry::BlockStateId,
        _step: i32,
        precise: bool,
        effects: &mut mod_api::InsideEffects,
    ) -> std::result::Result<(), ()> {
        let (x, y, z) = (at.x, at.y, at.z);
        if let Some(hooks) = crate::block_contact::HOOKS.get()
            && let Some(dimension) = crate::block_contact::current_level()
            && (hooks.inside_listens)(state)
        {
            let mut staged = mod_api::InsideEffects::default();
            let mut writes = Vec::new();
            let kind = self.cow.id.kind.id.clone();
            let id = self.cow.id.id as i32;
            (hooks.inside_fire)(self.level, dimension, id, &kind, state, at, precise, &mut staged, &mut writes);
            effects.absorb_step(staged);
            crate::block_contact::inside_written(state, at, writes);
        }
        if self.w.block_name(x, y, z) == "minecraft:powder_snow" {
            // A burning cow would destroy the block (`BreakBlock`, no listener here), then it is put out.
            self.cow.extinguish_fire();
        }
        match self.w.block_name(x, y, z) {
            "minecraft:cactus" => {
                self.cow.damage(self.level, 1.0, Damage::of(DamageKind::Cactus));
            }
            name @ ("minecraft:campfire" | "minecraft:soul_campfire")
                if self.w.prop(x, y, z, "lit") == Some("true") =>
            {
                let damage = if name == "minecraft:campfire" { 1.0 } else { 2.0 };
                self.cow.damage(self.level, damage, Damage::of(DamageKind::Campfire));
            }
            "minecraft:cobweb" => {
                self.cow.motion.fall_distance = 0.0;
                self.cow.motion.stuck_multiplier = Vec3::new(0.25, f64::from(0.05_f32), 0.25);
            }
            "minecraft:sweet_berry_bush" => {
                self.cow.motion.fall_distance = 0.0;
                self.cow.motion.stuck_multiplier = Vec3::new(f64::from(0.8_f32), 0.75, f64::from(0.8_f32));
                let moved = self.cow.motion.previous.subtract(self.cow.motion.position);
                if self.w.prop(x, y, z, "age") != Some("0")
                    && moved.horizontal_distance_squared() > 0.0
                    && (moved.x.abs() >= f64::from(0.003_f32) || moved.z.abs() >= f64::from(0.003_f32))
                {
                    self.cow.damage(self.level, 1.0, Damage::of(DamageKind::SweetBerryBush));
                }
            }
            "minecraft:powder_snow"
                if self.w.block_name(
                    self.cow.motion.position.x.floor() as i32,
                    self.cow.motion.position.y.floor() as i32,
                    self.cow.motion.position.z.floor() as i32,
                ) == "minecraft:powder_snow" =>
            {
                self.cow.motion.fall_distance = 0.0;
                self.cow.motion.stuck_multiplier = Vec3::new(f64::from(0.9_f32), 1.5, f64::from(0.9_f32));
                let known_movement = self.w.controlling_player_movement.unwrap_or(self.cow.motion.velocity);
                // The particle's velocity is drawn from the level's random.
                let _ = crate::powder_snow::particle_velocity(known_movement, self.level.random());
            }
            _ => {}
        }
        Ok(())
    }
}
use horizon_core::random::RandomSource;
