//! Boats and rafts (`AbstractBoat`): water/land/air status, floating,
//! paddling controls, collisions, riding (`positionRider`, `clampRotation`,
//! dismounting), `BoatItem` placement, `VehicleEntity.hurtServer` breaking,
//! saving and `AbstractBoatRenderer` with animated paddles.
mod placement;
pub(super) use placement::OutlineFluidRay;

pub(super) use horizon_gameplay::boats::*;

use super::{Game, InteractTarget, entity::Entity, view_vector};
use horizon_client::{
    blocks::Fluid,
    collisions::{EntityCollisions, SceneCollisions},
    items::ItemStack,
    motion::RiddenBoat,
};
use horizon_core::{
    geometry::{Aabb, ShapeOp, Vec3, VoxelShape, trig},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::collision_context::CollisionContext;
use horizon_ecs_proto::{boat_hurt::{HitResult, UseResult}, targeting::Who};
use std::{f32::consts::PI, sync::Arc};

impl Game<'_> {
    /// `FluidState.getHeight` and whether it is a source, for `fluid` or,
    /// with `None`, any fluid (`ClipContext.Fluid.ANY`).
    pub(super) fn fluid_height_of(
        &self,
        x: i32,
        y: i32,
        z: i32,
        fluid: Option<Fluid>,
    ) -> Option<(f32, bool)> {
        let f = self.res.facts().get(self.world.block(x, y, z));
        if f.fluid == Fluid::None || fluid.is_some_and(|want| f.fluid != want) {
            return None;
        }
        let above = self
            .res
            .facts()
            .get(self.world.block(x, y.wrapping_add(1), z));
        // FluidState.getHeight: 1 under the same fluid, else amount / 9.
        let height = if above.fluid == f.fluid {
            1.0
        } else {
            f32::from(f.fluid_level) / 9.0
        };
        Some((height, f.fluid_level == 8 && !f.fluid_falling))
    }
    /// `Entity.move`'s fall reset for an entity that keeps a fall and moves on
    /// by `moved` from `from`: a move of a block or more casts up to 8 blocks
    /// of it (`ClipContext.Block.FALLDAMAGE_RESETTING`, `ClipContext.Fluid.WATER`)
    /// and a hit resets the fall, before the move's own drop counts.
    pub(super) fn vehicle_fall_reset(&self, fall: f64, from: Vec3, moved: Vec3) -> f64 {
        let length = moved.length_squared();
        if fall == 0.0 || length < 1.0 {
            return fall;
        }
        let reset = &self.res.motion().fall_reset;
        let fluids = &self.res.motion().fluids;
        let to = from.add_vector(moved.normalize().scale(length.sqrt().min(8.0)));
        let hit = horizon_core::geometry::traverse_blocks(from, to, 1_000_000, |p| {
            // A tagged block clips as a full block, not its collider.
            let name = &self.res.facts().get(self.world.block(p.x, p.y, p.z)).name;
            if reset.contains(&**name) && VoxelShape::block().clip(from, to, p).is_some() {
                return Some(());
            }
            // The water's shape, through the shared fluid shape cache.
            horizon_client::entities::water_clip(
                &self.loop_context,
                &self.world,
                fluids,
                from,
                to,
                p,
            )
            .then_some(())
        });
        if matches!(hit, Ok(Some(()))) {
            0.0
        } else {
            fall
        }
    }
    /// `boat_fall` on the plain numbers, for a boat the ECS holds.
    pub(in crate::game) fn boat_fall_at(&self, position: Vec3, on_ground: bool, fall_distance: f64, moved_y: f64) -> f64 {
        if on_ground {
            return 0.0;
        }
        if moved_y < 0.0 {
            let below = self.res.facts().get(self.world.block(
                position.x.floor() as i32,
                (position.y.floor() as i32).wrapping_sub(1),
                position.z.floor() as i32,
            ));
            if below.fluid != Fluid::Water {
                return fall_distance - f64::from(moved_y as f32);
            }
        }
        fall_distance
    }
    /// `Entity.lavaHurt`'s sound once the hurt went through.
    pub(in crate::game) fn lava_hurt_sound(&mut self, at: [f64; 3]) {
        let pitch = 2.0 + self.world.random.next_float() * 0.4;
        self.audio
            .play("minecraft:entity.generic.burn", Some(at), 0.4, pitch);
    }
    /// Where the boat at `i` lets a player facing `yaw` off.
    pub(super) fn boat_player_dismount_target(&self, id: u32, yaw: f32) -> Vec3 {
        self.boat_dismount_target(id, 0.6, yaw).unwrap_or(Vec3::ZERO)
    }
    /// `getDismountLocationForPassenger` for the player riding the ECS boat `id`: the rider leaves its seat and
    /// stands in front of it or on top.
    pub(super) fn dismount_boat(&mut self, id: u32) {
        if self.riding == Some(id) && !self.native_may_leave(id) {
            return;
        }
        self.riding = None;
        self.ecs.boat_release(id);
        let Some(at) = self.ecs_view(id).map(|v| Vec3::new(v.position[0], v.position[1], v.position[2])) else {
            return;
        };
        if self.dismount_in_portal(at) {
            return;
        }
        let target = self.boat_player_dismount_target(id, self.yaw);
        if let Some(p) = self.player.as_mut() {
            p.position = target;
            p.velocity = Vec3::ZERO;
        }
    }
    /// The ECS spawns a boat of `wood` and links it to a network id (`BoatItem.use` and the dispenser share it).
    pub(super) fn spawn_boat_ecs(&mut self, wood: &str, chest: bool, at: [f64; 3], yaw: f32) -> Option<u32> {
        let mut next = self.next_entity;
        let id = self.with_ecs(|ecs, mw| {
            ecs.spawn_boat_id(mw, wood, chest, at, yaw, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        id
    }
    /// `getDismountLocationForPassenger` for a passenger `width` wide
    /// facing `yaw`.
    pub(super) fn boat_dismount_target(&self, id: u32, width: f64, yaw: f32) -> Option<Vec3> {
        let [px, _, pz] = self.ecs.position(id)?;
        let top = self.ecs.bounds(id)?.max_position().y;
        Some(self.boat_dismount_at(px, pz, top, width, yaw))
    }
    /// The dismount spot of a boat whose origin is `(px, pz)` and whose top is `top`, for a passenger `width` wide facing `yaw`.
    pub(in crate::game) fn boat_dismount_at(&self, px: f64, pz: f64, top: f64, width: f64, yaw: f32) -> Vec3 {
        let r = yaw.to_radians();
        let distance = (WIDTH * std::f64::consts::SQRT_2 + width + 1.0e-5) / 2.0;
        let (s, c) = (-(r.sin()) as f64, r.cos() as f64);
        let m = s.abs().max(c.abs());
        let (x, z) = (px + s * distance / m, pz + c * distance / m);
        let (bx, by, bz) = (x.floor() as i32, top.floor() as i32, z.floor() as i32);
        let below = self.res.facts().get(self.world.block(bx, by - 1, bz));
        let here = self.res.facts().get(self.world.block(bx, by, bz));
        if below.fluid != Fluid::Water
            && !here.full_collision
            && (below.full_collision || below.legacy_solid)
        {
            Vec3::new(x, f64::from(by), z)
        } else {
            Vec3::new(px, top, pz)
        }
    }
    /// The boat `BoatItem.use` would place for `stack` (its SUCCESS), or
    /// None when it aims at nothing (PASS) or the spot is taken (FAIL).
    pub(super) fn boat_to_place(&self, stack: &ItemStack) -> Option<Boat> {
        let (wood, chest) = any_boat_wood(&stack.item)?;
        let hit = self.boat_item_hit()?;
        let mut boat = Boat::new(
            self.next_entity + 1,
            Arc::from(wood),
            Vec3::new(hit[0], hit[1], hit[2]),
            self.yaw,
        );
        if chest {
            boat = boat.with_chest();
        }
        let bb = boat.bounds();
        let (lo, hi) = (bb.min_position(), bb.max_position());
        let e = 1.0e-7;
        let inner = Aabb::new(
            Vec3::new(lo.x + e, lo.y + e, lo.z + e),
            Vec3::new(hi.x - e, hi.y - e, hi.z - e),
        );
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        if !scene.unobstructed(inner).unwrap_or(false) || self.boat_entity_collision(bb) {
            return None;
        }
        Some(boat)
    }
    /// The boat under the crosshair and its distance; the ECS picks it, `riding` is never a target.
    pub(super) fn boat_hit(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        self.ecs.boat_pick(eye, dir, self.entity_reach, self.riding)
    }
    /// Whether the player rides a chest boat: `HasCustomInventoryScreen`, so
    /// the inventory key sends `ServerboundPlayerCommandPacket.Action.OPEN_INVENTORY`
    /// (`ServerGamePacketListenerImpl.handlePlayerCommand` opens its screen).
    pub(super) fn riding_chest_boat(&self) -> bool {
        self.riding
            .and_then(|id| self.ecs.entity_of(id))
            .and_then(|e| self.ecs.boat_wood(e))
            .is_some_and(|(_, chest)| chest)
    }
    /// `AbstractChestBoat.openCustomInventoryScreen`: the inventory key while riding a chest boat opens its chest.
    pub(super) fn open_ridden_chest_boat(&mut self) -> bool {
        let Some(id) = self.riding.filter(|_| self.riding_chest_boat()) else {
            return false;
        };
        self.open_chest_boat_container(id);
        true
    }
    /// `AbstractBoat.modifyPassengerFluidInteractionBox`: the ridden boat's top and whether it is under water, for
    /// `Entity.getFluidInteractionBox`; the ECS holds the boat.
    pub(super) fn riding_boat_fluid_box(&self) -> Option<RiddenBoat> {
        let id = self.riding?;
        let status = self.ecs.boat_status(id)?;
        let v = self.ecs_view(id)?;
        Some(RiddenBoat {
            under_water: status.under_water,
            max_y: Boat::bounds_at(Vec3::new(v.position[0], v.position[1], v.position[2])).max_position().y,
        })
    }
    /// `BoatItem.use`: the ECS spawns the boat `boat_to_place` finds and links it to a network id.
    pub(super) fn place_boat(&mut self, stack: &ItemStack) -> bool {
        let Some(boat) = self.boat_to_place(stack) else {
            return false;
        };
        let Some((wood, chest)) = any_boat_wood(&stack.item) else {
            return false;
        };
        let (at, yaw, kind) = (
            [boat.position.x, boat.position.y, boat.position.z],
            boat.yaw,
            boat.entity_type(),
        );
        let mut next = self.next_entity;
        let id = self.with_ecs(|ecs, mw| {
            ecs.spawn_boat_id(mw, wood, chest, at, yaw, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        let Some(id) = id else {
            return false;
        };
        self.apply_item_stack_config(super::commands::Target::Boat(id), &kind, stack, true);
        let key = (at[0].floor() as i32 >> 4, at[2].floor() as i32 >> 4);
        self.world.modified.insert(key);
        self.native_vehicle_create(kind.as_str(), at);
        let ctx = self.player_context(None);
        self.post_game_event("minecraft:entity_place", at, ctx);
        if !self.creative {
            self.consume_held();
        }
        self.award_stat("used", &stack.item, 1);
        self.swing_arm();
        true
    }
    /// `AbstractBoat.interact` on the ECS: the lead, the chest or the seat of the boat `target` names, or of the one under the crosshair.
    pub(super) fn interact_boat(&mut self, target: Option<InteractTarget>) -> bool {
        let Some((id, d)) = (match target {
            Some(t) => Some((t.id as u32, 0.0)),
            None => self.boat_hit(),
        }) else {
            return false;
        };
        let sneaking = self.sneak_held;
        // Only a client's own pick is stopped by a block: the range check passed.
        if target.is_none() && !self.before_block(d) {
            return false;
        }
        let player = self.own_entity_id() as u32;
        let guest = self.ridden_by_guest(id);
        let mut used = self.with_ecs(|ecs, mw| ecs.boat_use(mw, id, player, 0, sneaking, false, guest));
        // `Entity.startRiding` leaves the vehicle the player sits in: a refused seat is retried once it is left.
        if matches!(used, UseResult::Pass) && self.riding.is_some_and(|v| v != id) {
            self.stop_riding();
            used = self.with_ecs(|ecs, mw| ecs.boat_use(mw, id, player, 0, sneaking, false, guest));
        }
        match used {
            UseResult::Pass => false,
            UseResult::Mounted => {
                self.start_riding(id);
                true
            }
            UseResult::Opened => {
                self.open_chest_boat_container(id);
                self.swing_arm();
                true
            }
            UseResult::Leashed | UseResult::Unleashed => {
                self.swing_arm();
                true
            }
        }
    }
    /// `ContainerEntity.interactWithContainerVehicle` on the ECS: the chest's slots go under the placeholder key the menu edits.
    pub(super) fn open_chest_boat_container(&mut self, id: u32) {
        let (Some(at), Some(items)) = (self.ecs.position(id), self.ecs.boat_items(id)) else {
            return;
        };
        let key = (i32::MAX - 15, super::minecarts::CART_KEY_Y, id as i32);
        if !self.world.block_entities.contains_key(&key) {
            self.world
                .block_entities
                .insert(key, horizon_client::containers::BlockEntity::Chest(items));
        }
        self.container = Some(key);
        self.right = false;
        let ctx = self.player_context(None);
        self.post_game_event("minecraft:container_open", at, ctx);
        self.anger_piglins(true);
    }
    /// `VehicleEntity.hurtServer` from the player's attack, on the boat under the crosshair.
    pub(super) fn attack_boat(&mut self) -> bool {
        let Some((id, d)) = self.boat_hit() else {
            return false;
        };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        self.strike_boat(id);
        self.swing_arm();
        true
    }
    /// The player's blow on the boat `id` (`Player.attack`'s full damage against its type); the ECS takes the hit.
    pub(super) fn strike_boat(&mut self, id: u32) {
        let (Some(e), Some(area)) = (self.ecs.entity_of(id), self.ecs.bounds(id)) else {
            return;
        };
        let Some((wood, chest)) = self.ecs.boat_wood(e) else {
            return;
        };
        let Some(row) = horizon_ecs_proto::species::boats::WOODS
            .iter()
            .find(|w| w.0 == wood)
        else {
            return;
        };
        let kind = if chest { row.2 } else { row.1 };
        let (damage, scale, _, magic) = self.melee_damage(kind, false, Some(id));
        let sweep = self.sweep_before_hit(kind, false);
        let attacker = self.own_entity_id() as u32;
        let source = horizon_client::health::HurtSource {
            damage_type: "minecraft:player_attack",
            causing: attacker as i32,
            direct: attacker as i32,
            position: None,
        };
        let vanilla = self.saved_refuses_damage(id, "minecraft:player_attack", self.creative);
        let invulnerable = crate::game::entity::damage_level_of!(self, scoped)
            .invulnerability_check(
                horizon_client::health::Hurting::Entity(id as i32, kind, source),
                vanilla,
            );
        let hurt = if invulnerable {
            None
        } else {
            self.hurt_boat_result(id, damage, self.creative, Some(Who::Player(attacker)))
        };
        // AbstractChestBoat.destroy: a player's hit that breaks a chest boat (not a creative discard) angers the piglins.
        if chest
            && hurt == Some(true)
            && !self.creative
            && self
                .session
                .rules
                .bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS)
        {
            self.anger_piglins(true);
        }
        self.vehicle_attack_sounds(area, sweep, magic.then_some(id as i32));
        if damage > 0.0
            && let Some(destroyed) = hurt
        {
            let (knockback, _) = self.extra_knockback(kind, scale);
            if knockback > 0.0 {
                // Player.causeExtraKnockback uses Entity.push for a boat, not LivingEntity.knockback.
                if !destroyed {
                    let (x, z) = self.extra_knockback_direction();
                    self.ecs.push_entity(
                        id,
                        [
                            f64::from(-(x as f32) * knockback),
                            0.1,
                            f64::from(-(z as f32) * knockback),
                        ],
                    );
                }
                self.special_attack_slowdown(knockback);
            }
            self.special_attack_exhaustion();
        }
        // Player.onAttack: the hit starts the attack strength again.
        self.attack_ticks = 0;
    }
    /// `VehicleEntity.hurtServer` on the boat `id`: the ECS adds the damage and breaks it past 40 (`creative`: discarded at once).
    /// None when the hit is cancelled, Some(true) when the boat is gone.
    pub(in crate::game) fn hurt_boat_result(
        &mut self,
        id: u32,
        mut damage: f32,
        creative: bool,
        by: Option<Who>,
    ) -> Option<bool> {
        let mut cancel = false;
        self.native_vehicle_damage("boat", &mut damage, &mut cancel);
        if cancel {
            return None;
        }
        let drops = self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS);
        match self.with_ecs(|ecs, mw| ecs.boat_hit(mw, id, damage, by, creative, drops)) {
            HitResult::Ignored => None,
            HitResult::Hurt => Some(false),
            HitResult::Broken { .. } => Some(true),
        }
    }
    /// Hurts the boat `id` from a non-player source; true when it was removed.
    pub(super) fn hurt_boat(&mut self, id: u32, damage: f32, creative: bool) -> bool {
        self.hurt_boat_result(id, damage, creative, None) == Some(true)
    }
    /// Whether a spectator must be refused this boat's menu.
    pub(super) fn boat_loot_pending(&self, id: u32) -> bool {
        matches!(
            self.persist.raw.get(&id),
            Some(horizon_core::nbt::Tag::Compound(map)) if map.contains_key(&"LootTable".into())
        )
    }
    /// Existing boat access routes all use the shared unpack operation.
    pub(super) fn unpack_boat_loot(&mut self, id: u32, player: bool) {
        let player = player.then(|| self.native_acting_player()).flatten();
        self.unpack_loot(super::container_loot::LootTarget::Boat(id), player);
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::nbt::Tag;

}
#[cfg(test)]
mod cobweb_tests {
    use super::Boat;
    use crate::game::game_tests::tests::server_game;
    use horizon_client::chunk::Chunk;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}
