//! The step's view of the entities a mod call reads (`EcsCx`): the body, the identity and the health of the entities the ECS step
//! holds, read from their components (`horizon_ecs_proto::bridge::host_read`); the game host answers the handles the step does not
//! hold (players, items, blocks). The generated view (`mod_api_step_view_generated.rs`) forwards its reads here; nothing is written.
use super::mod_goals::{self, EcsCx};
use bevy_ecs::{entity::Entity, world::World};
use myriade_engine_lib::items::ItemStack;
use horizon_ecs_proto::{
    pose_state,
    arrow_more::NoPhysics,
    behaviours::elder_curse::Silent,
    bridge::{host_read, host_read_ai, host_read_living, host_state, host_vehicle, ids, teams},
    cow::{Air, Burning, FluidState, Frozen, GrowAt, Health, InvulnerableUntil, SpeciesId, SpeciesTable, now_of},
    custom_name,
    despawn::Persistent,
    raids::Raider,
    effects,
    equip_stack,
    leash::{Holder, Leash},
    dragon::parts::Parts,
    sprinting::Sprinting,
    frame_item,
    gliding::FallFlying,
    goal_edit,
    nav_supplied,
    nbt_tag,
    species::SoundCategory,
    glowing::Glowing,
    goals::{Head, Jump},
    invulnerable::Invulnerable,
    locomotion::climb::Climbing,
    mount::Mount,
    tame_flag::TameFlag,
    targeting::Who,
    using::UsingItem,
};
use myriade_types::Hand;
use myriade_types::ClipHit;
use myriade_types::EffectKey;
use mod_api::EntityRef;
use myriade_types::EquipmentSlot;
use myriade_types::LeashHolder;
use mod_api::LivingRef;
use mod_api::MobGoalEntry;
use mod_api::MobRef;
use myriade_types::PathType;
use mod_api::prelude::*;
use super::level_goals::api_id;

// The generated view is a child of this module, so that its `super::EcsCx` names the engine's context.
mod step {
    include!(concat!(env!("OUT_DIR"), "/mod_api_step_view_generated.rs"));
}

impl EcsCx<'_> {
    fn world(&self) -> &World {
        self.w
    }

    /// The ECS entity `entity` names, when it is a handle of this step's epoch that the step holds.
    fn ecs_of(&self, entity: EntityRef) -> Option<Entity> {
        if entity.epoch() != self.epoch {
            return None;
        }
        mod_goals::entity_of_id(self.world(), entity.id())
    }

    // --- the reads the generated view serves (`STEP_VIEW_READS`) -------------

    pub(super) fn r_entity_position(&self, entity: EntityRef) -> Option<[f64; 3]> {
        match self.ecs_of(entity) {
            Some(e) => host_read::motion(self.world(), e).map(|m| m.position),
            None => self.host.entity_position(entity),
        }
    }

    pub(super) fn r_entity_block_position(&self, entity: EntityRef) -> Option<[i32; 3]> {
        match self.ecs_of(entity) {
            Some(e) => host_read::motion(self.world(), e).map(|m| m.position.map(|c| c.floor() as i32)),
            None => self.host.entity_block_position(entity),
        }
    }

    pub(super) fn r_entity_velocity(&self, entity: EntityRef) -> Option<[f64; 3]> {
        match self.ecs_of(entity) {
            Some(e) => host_read::velocity(self.world(), e).map(|v| [v.x, v.y, v.z]),
            None => self.host.entity_velocity(entity),
        }
    }

    /// `getYRot` and `getXRot`: the body's yaw and the head's pitch.
    pub(super) fn r_entity_rotation(&self, entity: EntityRef) -> Option<[f32; 2]> {
        match self.ecs_of(entity) {
            Some(e) => Some([host_read::yaw(self.world(), e)?, self.world().get::<Head>(e)?.pitch]),
            None => self.host.entity_rotation(entity),
        }
    }

    pub(super) fn r_entity_head_yaw(&self, entity: EntityRef) -> Option<f32> {
        match self.ecs_of(entity) {
            Some(e) => host_read::head_body_yaw(self.world(), e).map(|(head, _)| head),
            None => self.host.entity_head_yaw(entity),
        }
    }

    pub(super) fn r_entity_bounding_box(&self, entity: EntityRef) -> Option<[[f64; 3]; 2]> {
        match self.ecs_of(entity) {
            Some(e) => host_read::bounds(self.world(), e).map(|b| {
                let (lo, hi) = (b.min_position(), b.max_position());
                [[lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]]
            }),
            None => self.host.entity_bounding_box(entity),
        }
    }

    pub(super) fn r_entity_eye_height(&self, entity: EntityRef) -> Option<f64> {
        match self.ecs_of(entity) {
            Some(e) => host_read::eye_offset(self.world(), e),
            None => self.host.entity_eye_height(entity),
        }
    }

    pub(super) fn r_entity_eye_position(&self, entity: EntityRef) -> Option<[f64; 3]> {
        match self.ecs_of(entity) {
            Some(e) => {
                let eye = host_read::eye_offset(self.world(), e)?;
                host_read::motion(self.world(), e).map(|m| [m.position[0], m.position[1] + eye, m.position[2]])
            }
            None => self.host.entity_eye_position(entity),
        }
    }

    pub(super) fn r_entity_distance_to_sqr(&self, entity: EntityRef, pos: [f64; 3]) -> Option<f64> {
        match self.ecs_of(entity) {
            Some(e) => host_read::motion(self.world(), e).map(|m| {
                let [x, y, z] = m.position;
                (x - pos[0]).powi(2) + (y - pos[1]).powi(2) + (z - pos[2]).powi(2)
            }),
            None => self.host.entity_distance_to_sqr(entity, pos),
        }
    }

    /// `LevelRef::ray_trace_entities` from the entity's eyes: the ray needs the block world, which the ECS does not hold, so the game
    /// answers (the host channel of `ModGoalsHost`).
    pub(super) fn r_entity_pick(&self, entity: EntityRef, distance: f64) -> Option<ClipHit> {
        self.host.entity_pick(entity, distance)
    }

    /// `Entity.entityTags`: the command tags of an ECS entity (`nbt_tag::names`).
    pub(super) fn r_entity_tags(&self, entity: EntityRef) -> Option<Vec<String>> {
        match self.ecs_of(entity) {
            Some(e) => Some(nbt_tag::names(self.world(), e).into_iter().map(str::to_owned).collect()),
            None => self.host.entity_tags(entity),
        }
    }

    /// `Entity.getSoundSource`: the category of the entity's species (`species::SoundCategory`).
    pub(super) fn r_entity_sound_source(&self, entity: EntityRef) -> Option<myriade_types::SoundSource> {
        match self.ecs_of(entity) {
            Some(e) => species_sound(self.world(), e),
            None => self.host.entity_sound_source(entity),
        }
    }

    /// Host channel: the portal countdown is the game's (`nether_portal.rs`), not a component.
    pub(super) fn r_entity_portal_cooldown(&self, entity: EntityRef) -> Option<i32> {
        self.host.entity_portal_cooldown(entity)
    }

    /// Host channel: the damage types are the game's (`native_entity_invulnerable.rs`), no ECS table.
    pub(super) fn r_entity_invulnerable_to_damage(&self, entity: EntityRef, damage_type: &str) -> Option<bool> {
        self.host.entity_invulnerable_to_damage(entity, damage_type)
    }

    /// `Entity.isInvulnerableToPiercingWeapon`: the same two sources as `isInvulnerable`, the `Invulnerable` marker and the window.
    pub(super) fn r_entity_invulnerable_to_piercing_weapon(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Invulnerable>(e).is_some() || invulnerable_left(self.world(), e) > 0),
            None => self.host.entity_invulnerable_to_piercing_weapon(entity),
        }
    }

    /// Host channel: the pose boxes are the game's (`native_living_position.rs`), the ECS holds the standing box only.
    pub(super) fn r_living_dimensions_for_pose(&self, living: LivingRef, pose: Pose) -> Option<[f64; 3]> {
        self.host.living_dimensions_for_pose(living, pose)
    }

    /// Host channel: the sleeping block is read through the game's block world (`native_living_refs.rs`).
    pub(super) fn r_living_bed_orientation(&self, living: LivingRef) -> Option<Direction> {
        self.host.living_bed_orientation(living)
    }

    pub(super) fn r_entity_is_sprinting(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Sprinting>(e).is_some()),
            None => self.host.entity_is_sprinting(entity),
        }
    }

    /// `Entity.getOnPos`: the cell under the feet (`rules::sniff::on_cell`).
    pub(super) fn r_entity_on_block(&self, entity: EntityRef) -> Option<[i32; 3]> {
        match self.ecs_of(entity) {
            Some(e) => host_read::motion(self.world(), e).map(|m| horizon_ecs_proto::rules::sniff::on_cell(m.position)),
            None => self.host.entity_on_block(entity),
        }
    }

    /// `Entity.doTeamsAllowDamage` against `source`: refused only by a team with friendly fire off (`teams::blocks_damage`).
    pub(super) fn r_entity_entities_team_damage_allowed(&self, entity: EntityRef, source: EntityRef) -> Option<bool> {
        match (self.ecs_of(entity), self.ecs_of(source)) {
            (Some(a), Some(b)) => Some(!teams::blocks_damage(self.world(), Who::Entity(a), Who::Entity(b))),
            _ => self.host.entity_entities_team_damage_allowed(entity, source),
        }
    }

    /// `Mob.isWithinMeleeAttackRange` against the box of `target` (`host_read_living::melee_reaches`).
    pub(super) fn r_living_in_melee_range(&self, living: LivingRef, target: EntityRef) -> Option<bool> {
        match (self.ecs_of(living.entity()), self.ecs_of(target)) {
            (Some(e), Some(t)) => host_read::bounds(self.world(), t).map(|b| host_read_living::melee_reaches(self.world(), e, b)),
            _ => self.host.living_in_melee_range(living, target),
        }
    }

    /// `LivingEntity.hasLineOfSight`: a block ray, which the ECS does not hold, so the game answers (the host channel of `ModGoalsHost`).
    pub(super) fn r_living_has_line_of_sight(&self, living: LivingRef, target: EntityRef) -> Option<bool> {
        self.host.living_has_line_of_sight(living, target)
    }

    /// Host channel: `dismount_point` reads the block floors and collisions through `MobWorld` (`host_mount.rs`), which the view lacks.
    pub(super) fn r_entity_dismount_location(&self, vehicle: EntityRef, passenger: LivingRef) -> Option<[f64; 3]> {
        self.host.entity_dismount_location(vehicle, passenger)
    }

    /// Host channel: dragon parts are boxes in the ECS (`dragon/parts.rs`, no entity handles), so the game answers them.
    pub(super) fn r_entity_parts(&self, entity: EntityRef) -> Option<Vec<EntityRef>> {
        self.host.entity_parts(entity)
    }

    /// `Entity.level`: the level of this ECS (one ECS per level, `EcsCx::level`).
    pub(super) fn r_entity_level(&self, entity: EntityRef) -> Option<mod_api::LevelRef> {
        match self.ecs_of(entity) {
            Some(_) => self.level,
            None => self.host.entity_level(entity),
        }
    }

    /// `Raider.getCurrentRaid`: the raid the raider names (`Raider.raid`), in this level.
    pub(super) fn r_mob_raid(&self, mob: MobRef) -> Option<mod_api::RaidRef> {
        match self.ecs_of(mob.entity()) {
            Some(e) => {
                let id = self.world().get::<Raider>(e)?.raid?;
                Some(mod_api::RaidRef::new(self.level?, id))
            }
            None => self.host.mob_raid(mob),
        }
    }

    /// `Entity.DATA_CUSTOM_NAME` of an ECS entity (`custom_name::get`).
    pub(super) fn r_entity_name(&self, entity: EntityRef) -> Option<String> {
        match self.ecs_of(entity) {
            Some(e) => custom_name::get(self.world(), e),
            None => self.host.entity_name(entity),
        }
    }

    /// Every ECS entity is in its level: a handle that resolves here is in it.
    pub(super) fn r_entity_added_to_level(&self, entity: EntityRef) -> bool {
        match self.ecs_of(entity) {
            Some(_) => true,
            None => self.host.entity_added_to_level(entity),
        }
    }

    /// The goals of an ECS mob's selector, as `goals` reads them (`level_goals::api_id`, `goal_edit::list`).
    fn goal_entries(&self, e: Entity, target: bool) -> Option<Vec<MobGoalEntry>> {
        let list = goal_edit::list(self.world(), e, target)?;
        Some(list.into_iter().map(|(priority, m, running)| MobGoalEntry { priority: i32::from(priority), id: api_id(m), running }).collect())
    }

    /// `GoalSelector.getAvailableGoals` of an ECS mob.
    pub(super) fn r_mob_goals(&self, mob: MobRef) -> Option<Vec<MobGoalEntry>> {
        match self.ecs_of(mob.entity()) {
            Some(e) => self.goal_entries(e, false),
            None => self.host.mob_goals(mob),
        }
    }

    /// `GoalSelector` of the target selector of an ECS mob.
    pub(super) fn r_mob_target_goals(&self, mob: MobRef) -> Option<Vec<MobGoalEntry>> {
        match self.ecs_of(mob.entity()) {
            Some(e) => self.goal_entries(e, true),
            None => self.host.mob_target_goals(mob),
        }
    }

    /// `Mob.getPathfindingMalus` of an ECS mob for the path type `kind` (`nav_supplied::malus_of`).
    pub(super) fn r_mob_pathfinding_malus(&self, mob: MobRef, kind: PathType) -> Option<f32> {
        match self.ecs_of(mob.entity()) {
            Some(e) => nav_supplied::malus_of(self.world(), e, kind),
            None => self.host.mob_pathfinding_malus(mob, kind),
        }
    }

    pub(super) fn r_mob_navigation_speed(&self, mob: MobRef) -> Option<f64> {
        match self.ecs_of(mob.entity()) {
            Some(e) => host_read_ai::nav_speed_modifier(self.world(), e),
            None => self.host.mob_navigation_speed(mob),
        }
    }

    pub(super) fn r_entity_on_ground(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => host_read::motion(self.world(), e).map(|m| m.on_ground),
            None => self.host.entity_on_ground(entity),
        }
    }

    /// The snapshot carries the species, not its type id: the species resolves to its type key.
    pub(super) fn r_entity_kind(&self, entity: EntityRef) -> Option<myriade_types::EntityTypeKey> {
        match self.ecs_of(entity) {
            Some(e) => host_read::species(self.world(), e).and_then(mod_api::__private::resolved_entity_type_key),
            None => self.host.entity_kind(entity),
        }
    }

    /// `Entity.getViewVector(1.0)` from the body's yaw and the head's pitch.
    pub(super) fn r_entity_view_vector(&self, entity: EntityRef) -> Option<[f64; 3]> {
        match self.ecs_of(entity) {
            Some(e) => Some(view_vector(host_read::yaw(self.world(), e)?, self.world().get::<Head>(e)?.pitch, 0.0)),
            None => self.host.entity_view_vector(entity),
        }
    }

    /// `Entity.getUpVector(1.0)`: the view vector with its pitch turned a quarter back.
    pub(super) fn r_entity_up_vector(&self, entity: EntityRef) -> Option<[f64; 3]> {
        match self.ecs_of(entity) {
            Some(e) => Some(view_vector(host_read::yaw(self.world(), e)?, self.world().get::<Head>(e)?.pitch, -90.0)),
            None => self.host.entity_up_vector(entity),
        }
    }

    /// `Entity.getPose` as `Pose` names it: standing without a `PoseOverride`.
    pub(super) fn r_entity_pose(&self, entity: EntityRef) -> Option<&'static str> {
        match self.ecs_of(entity) {
            Some(e) => Some(pose_state::name(pose_state::get(self.world(), e).unwrap_or(0))),
            None => self.host.entity_pose(entity),
        }
    }

    /// `Entity.isCrouching`: the crouching pose (`Pose.CROUCHING`).
    pub(super) fn r_entity_is_crouching(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(pose_state::get(self.world(), e) == Some(pose_state::CROUCHING)),
            None => self.host.entity_is_crouching(entity),
        }
    }

    /// `isSecondaryUseActive` on a server: the crouching pose, as `Entity.isCrouching` reads it.
    pub(super) fn r_entity_is_shift_key_down(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(pose_state::get(self.world(), e) == Some(pose_state::CROUCHING)),
            None => self.host.entity_is_shift_key_down(entity),
        }
    }

    /// `Entity.isSpectator`: only a player is in spectator mode, and no ECS entity is a player.
    pub(super) fn r_entity_spectator(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(_) => Some(false),
            None => self.host.entity_spectator(entity),
        }
    }

    pub(super) fn r_entity_is_in_water(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(host_read::in_water(self.world(), e)),
            None => self.host.entity_is_in_water(entity),
        }
    }

    pub(super) fn r_mob_is_aggressive(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(host_read_ai::aggressive(self.world(), e)),
            None => self.host.mob_is_aggressive(mob),
        }
    }

    pub(super) fn r_mob_no_ai(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(host_read_ai::no_ai(self.world(), e)),
            None => self.host.mob_no_ai(mob),
        }
    }

    pub(super) fn r_mob_is_done(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(host_read_ai::nav_is_done(self.world(), e)),
            None => self.host.mob_is_done(mob),
        }
    }

    pub(super) fn r_mob_has_restriction(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(host_read_ai::has_restriction(self.world(), e)),
            None => self.host.mob_has_restriction(mob),
        }
    }

    pub(super) fn r_mob_restrict_center(&self, mob: MobRef) -> Option<[i32; 3]> {
        match self.ecs_of(mob.entity()) {
            Some(e) => host_read_ai::restrict_center(self.world(), e),
            None => self.host.mob_restrict_center(mob),
        }
    }

    pub(super) fn r_mob_restrict_radius(&self, mob: MobRef) -> Option<i32> {
        match self.ecs_of(mob.entity()) {
            Some(e) => host_read_ai::restrict_radius(self.world(), e),
            None => self.host.mob_restrict_radius(mob),
        }
    }

    pub(super) fn r_mob_left_handed(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(horizon_ecs_proto::left_handed::get(self.world(), e)),
            None => self.host.mob_left_handed(mob),
        }
    }

    pub(super) fn r_mob_no_action_time(&self, mob: MobRef) -> Option<i32> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(host_state::no_action_time(self.world(), e)),
            None => self.host.mob_no_action_time(mob),
        }
    }

    pub(super) fn r_mob_requires_custom_persistence(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(host_read_ai::requires_custom_persistence(self.world(), e)),
            None => self.host.mob_requires_custom_persistence(mob),
        }
    }

    pub(super) fn r_mob_can_pick_up_loot(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => host_read_ai::can_pick_up_loot(self.world(), e),
            None => self.host.mob_can_pick_up_loot(mob),
        }
    }

    pub(super) fn r_mob_can_use_portal(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(horizon_ecs_proto::portal::usable(self.world(), e, false)),
            None => self.host.mob_can_use_portal(mob),
        }
    }

    pub(super) fn r_living_sleeping_pos(&self, living: LivingRef) -> Option<[i32; 3]> {
        match self.ecs_of(living.entity()) {
            Some(e) => host_read_ai::sleeping_pos(self.world(), e),
            None => self.host.living_sleeping_pos(living),
        }
    }

    pub(super) fn r_living_can_breathe_underwater(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(host_read_living::breathes_water(self.world(), e)),
            None => self.host.living_can_breathe_underwater(living),
        }
    }

    pub(super) fn r_living_can_freeze(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(host_read_living::can_freeze(self.world(), e)),
            None => self.host.living_can_freeze(living),
        }
    }

    pub(super) fn r_living_gliding_ticks(&self, living: LivingRef) -> Option<i32> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(horizon_ecs_proto::gliding::ticks(self.world(), e)),
            None => self.host.living_gliding_ticks(living),
        }
    }

    pub(super) fn r_living_can_use_portal(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(horizon_ecs_proto::portal::usable(self.world(), e, false)),
            None => self.host.living_can_use_portal(living),
        }
    }

    pub(super) fn r_entity_no_gravity(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(horizon_ecs_proto::no_gravity::get(self.world(), e)),
            None => self.host.entity_no_gravity(entity),
        }
    }

    pub(super) fn r_entity_custom_name_visible(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(host_state::name_visible(self.world(), e)),
            None => self.host.entity_custom_name_visible(entity),
        }
    }

    /// The handle of a seated `who`: a creature by its network id, a player by its id (`Who::Player`).
    fn who_ref(&self, who: Who) -> Option<EntityRef> {
        let id = match who {
            Who::Entity(e) => ids::id_of(self.world(), e)?,
            Who::Player(p) => p,
        };
        Some(EntityRef::new(self.epoch, id))
    }

    pub(super) fn r_living_last_attacker(&self, living: LivingRef) -> Option<EntityRef> {
        match self.ecs_of(living.entity()) {
            Some(e) => self.who_ref(host_read_ai::last_hurt_by(self.world(), e)?),
            None => self.host.living_last_attacker(living),
        }
    }

    /// `LivingEntity.getLastHurtByMob`: the last attacker that is living (a player, or an entity with a `Health`), forgotten after 100 ticks.
    pub(super) fn r_living_kill_credit(&self, living: LivingRef) -> Option<EntityRef> {
        match self.ecs_of(living.entity()) {
            Some(e) => {
                let who = host_read_ai::last_hurt_by(self.world(), e)?;
                let living_attacker = match who {
                    Who::Player(_) => true,
                    Who::Entity(a) => self.world().get::<Health>(a).is_some(),
                };
                if living_attacker { self.who_ref(who) } else { None }
            }
            None => self.host.living_kill_credit(living),
        }
    }

    /// `Entity.getLeashHolder` (`Leashable.leashHolder`): the player, entity or knot that holds the lead; unleashed without one.
    pub(super) fn r_entity_leash_holder(&self, entity: EntityRef) -> Option<LeashHolder> {
        match self.ecs_of(entity) {
            Some(e) => Some(match self.world().get::<Leash>(e).map(|l| l.holder) {
                None => LeashHolder::Unleashed,
                Some(Holder::Knot(p)) => LeashHolder::Knot(p),
                Some(Holder::Player(id)) => LeashHolder::Entity(self.who_ref(Who::Player(id))?),
                Some(Holder::Entity(h)) => LeashHolder::Entity(self.who_ref(Who::Entity(h))?),
            }),
            None => self.host.entity_leash_holder(entity),
        }
    }

    /// `LivingEntity.getItemBySlot`: the stack of the slot, in the order of `equip_stack`; None for a slot the entity has not.
    pub(super) fn r_living_equipment(&self, living: LivingRef, slot: EquipmentSlot) -> Option<ItemStack> {
        match self.ecs_of(living.entity()) {
            Some(e) => equip_stack::get(self.world(), e, equipment_index(slot)?),
            None => self.host.living_equipment(living, slot),
        }
    }

    /// `LivingEntity.getItemInHand`: the main hand is slot 0 and the off hand slot 1 of `equip_stack`.
    pub(super) fn r_living_hand_item(&self, living: LivingRef, hand: Hand) -> Option<ItemStack> {
        match self.ecs_of(living.entity()) {
            Some(e) => equip_stack::get(self.world(), e, usize::from(matches!(hand, Hand::Off))),
            None => self.host.living_hand_item(living, hand),
        }
    }

    /// `LivingEntity.hasEffect`: the effect is on the entity; an effect the game has no id for is never on it.
    pub(super) fn r_living_potion_affected(&self, living: LivingRef, effect: EffectKey) -> bool {
        let Some(index) = mod_api::__private::effect_index(effect) else {
            return false;
        };
        match self.ecs_of(living.entity()) {
            Some(e) => self.held_has_effect(e, index),
            None => self.host.living_potion_affected(living, effect),
        }
    }

    /// The held entity `e` carries the registry effect at `index` (an `EffectDef.id` has no namespace, the ECS names have one).
    pub(super) fn held_has_effect(&self, e: Entity, index: usize) -> bool {
        myriade_data::potions::effect_by_index(index)
            .and_then(|def| host_read_living::effect_kind(&format!("minecraft:{}", def.id)))
            .is_some_and(|kind| effects::has(self.world(), e, kind))
    }

    pub(super) fn r_mob_target(&self, mob: MobRef) -> Option<LivingRef> {
        match self.ecs_of(mob.entity()) {
            Some(e) => self.who_ref(host_read_ai::ai_target(self.world(), e)?).map(LivingRef::new),
            None => self.host.mob_target(mob),
        }
    }

    pub(super) fn r_entity_vehicle(&self, entity: EntityRef) -> Option<EntityRef> {
        match self.ecs_of(entity) {
            Some(e) => self.who_ref(Who::Entity(host_vehicle::host_vehicle(self.world(), e)?)),
            None => self.host.entity_vehicle(entity),
        }
    }

    /// `getRootVehicle`: the last vehicle up the chain, the entity itself when it rides nothing.
    pub(super) fn r_entity_root_vehicle(&self, entity: EntityRef) -> Option<EntityRef> {
        match self.ecs_of(entity) {
            Some(e) => {
                let root = std::iter::successors(Some(e), |x| host_vehicle::host_vehicle(self.world(), *x)).last()?;
                self.who_ref(Who::Entity(root))
            }
            None => self.host.entity_root_vehicle(entity),
        }
    }

    pub(super) fn r_entity_is_passenger(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(host_vehicle::is_passenger(self.world(), e)),
            None => self.host.entity_is_passenger(entity),
        }
    }

    pub(super) fn r_entity_is_vehicle(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(!host_vehicle::seated(self.world(), e).is_empty()),
            None => self.host.entity_is_vehicle(entity),
        }
    }

    /// The riders of `entity` in its seats, creatures and players alike.
    pub(super) fn r_entity_passengers(&self, entity: EntityRef) -> Option<Vec<EntityRef>> {
        match self.ecs_of(entity) {
            Some(e) => Some(host_vehicle::seated(self.world(), e).into_iter().filter_map(|who| self.who_ref(who)).collect()),
            None => self.host.entity_passengers(entity),
        }
    }

    /// Every rider of `entity`, down the chain of vehicles (`getIndirectPassengers`), in the order they are found.
    pub(super) fn r_entity_indirect_passengers(&self, entity: EntityRef) -> Option<Vec<EntityRef>> {
        match self.ecs_of(entity) {
            Some(e) => {
                let (mut out, mut todo) = (Vec::new(), vec![e]);
                while let Some(v) = todo.pop() {
                    for who in host_vehicle::seated(self.world(), v) {
                        out.extend(self.who_ref(who));
                        if let Who::Entity(c) = who {
                            todo.push(c);
                        }
                    }
                }
                Some(out)
            }
            None => self.host.entity_indirect_passengers(entity),
        }
    }

    pub(super) fn r_merchant_offers(&self, merchant: mod_api::MerchantRef) -> Option<Vec<myriade_engine::merchant::MerchantOffer>> {
        match self.ecs_of(merchant.entity()) {
            Some(e) => host_vehicle::trade_offers(self.world(), e),
            None => self.host.merchant_offers(merchant),
        }
    }

    pub(super) fn r_merchant_profession(&self, merchant: mod_api::MerchantRef) -> Option<String> {
        match self.ecs_of(merchant.entity()) {
            Some(e) => host_vehicle::trade_profession(self.world(), e),
            None => self.host.merchant_profession(merchant),
        }
    }

    pub(super) fn r_merchant_level(&self, merchant: mod_api::MerchantRef) -> Option<i32> {
        match self.ecs_of(merchant.entity()) {
            Some(e) => host_vehicle::trade_level(self.world(), e),
            None => self.host.merchant_level(merchant),
        }
    }

    pub(super) fn r_merchant_xp(&self, merchant: mod_api::MerchantRef) -> Option<i32> {
        match self.ecs_of(merchant.entity()) {
            Some(e) => host_vehicle::trade_xp(self.world(), e),
            None => self.host.merchant_xp(merchant),
        }
    }

    pub(super) fn r_merchant_villager_type(&self, merchant: mod_api::MerchantRef) -> Option<String> {
        match self.ecs_of(merchant.entity()) {
            Some(e) => host_vehicle::trade_variant(self.world(), e),
            None => self.host.merchant_villager_type(merchant),
        }
    }

    pub(super) fn r_living_health(&self, living: LivingRef) -> Option<f32> {
        match self.ecs_of(living.entity()) {
            Some(e) => host_read::health(self.world(), e),
            None => self.host.living_health(living),
        }
    }

    pub(super) fn r_living_max_health(&self, living: LivingRef) -> Option<f32> {
        match self.ecs_of(living.entity()) {
            Some(e) => host_read::max_health(self.world(), e),
            None => self.host.living_max_health(living),
        }
    }

    pub(super) fn r_living_alive(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => host_read::health(self.world(), e).map(|h| h > 0.0),
            None => self.host.living_alive(living),
        }
    }

    // --- the entity lookups the generated Host impl forwards to ---------------

    /// `Host::entity`: a network id the step holds names its mob; any other id is the game host's (a player, an item).
    fn lookup_entity(&self, id: u32) -> Option<EntityRef> {
        match mod_goals::entity_of_id(self.world(), id) {
            Some(_) => Some(EntityRef::new(self.epoch, id)),
            None => self.host.entity(id),
        }
    }

    /// `Host::living`: a handle the step holds is a living; the rest is the game host's.
    fn lookup_living(&self, entity: EntityRef) -> Option<LivingRef> {
        match self.ecs_of(entity) {
            Some(_) => Some(LivingRef::new(entity)),
            None => self.host.living(entity),
        }
    }

    /// `Entity.getUsedItemHand`: the off hand while a stack is in use there, else the main hand.
    pub(super) fn r_living_used_item_hand(&self, living: LivingRef) -> Option<Hand> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(if self.world().get::<UsingItem>(e).is_some_and(|u| u.off_hand) { Hand::Off } else { Hand::Main }),
            None => self.host.living_used_item_hand(living),
        }
    }

    /// `EntityType.getCategory`, by the name of the species' category.
    pub(super) fn r_entity_classification(&self, entity: EntityRef) -> Option<MobCategory> {
        match self.ecs_of(entity) {
            Some(e) => host_read::category(self.world(), e).and_then(mob_category),
            None => self.host.entity_classification(entity),
        }
    }

    /// `Entity.isInvulnerable`: the `Invulnerable` flag the entity's save carries.
    pub(super) fn r_entity_permanently_invulnerable(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Invulnerable>(e).is_some()),
            None => self.host.entity_permanently_invulnerable(entity),
        }
    }

    /// `Entity.invulnerableTime`: the ticks left of the invulnerability window a mod set.
    pub(super) fn r_entity_invulnerable_time(&self, entity: EntityRef) -> Option<i32> {
        match self.ecs_of(entity) {
            Some(e) => Some(invulnerable_left(self.world(), e)),
            None => self.host.entity_invulnerable_time(entity),
        }
    }

    /// The same `invulnerableTime`: a hit within it does only what it deals beyond the last one.
    pub(super) fn r_entity_damage_cooldown_time(&self, entity: EntityRef) -> Option<i32> {
        match self.ecs_of(entity) {
            Some(e) => Some(invulnerable_left(self.world(), e)),
            None => self.host.entity_damage_cooldown_time(entity),
        }
    }

    /// `Entity.noPhysics`: set by the constructors of the kinds that ignore collisions, never cleared.
    pub(super) fn r_entity_no_physics(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<NoPhysics>(e).is_some_and(|n| n.0)),
            None => self.host.entity_no_physics(entity),
        }
    }

    pub(super) fn r_entity_is_in_lava(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<FluidState>(e).is_some_and(|f| f.in_lava)),
            None => self.host.entity_is_in_lava(entity),
        }
    }

    /// `Entity.getRemainingFireTicks`: the ticks left on fire, from the tick the fire ends at.
    pub(super) fn r_entity_fire_ticks(&self, entity: EntityRef) -> Option<i32> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Burning>(e).map_or(0, |b| ticks_left(self.world(), e, b.until))),
            None => self.host.entity_fire_ticks(entity),
        }
    }

    /// `Entity.getAirSupply`: the air held, the most (300) before any is lost.
    pub(super) fn r_entity_air_supply(&self, entity: EntityRef) -> Option<i32> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Air>(e).map_or(MAX_AIR, |a| a.0)),
            None => self.host.entity_air_supply(entity),
        }
    }

    /// `Entity.getMaxAirSupply`: 300 for the kinds that breathe air; a dolphin's and an axolotl's own values are not here yet.
    pub(super) fn r_entity_max_air_supply(&self, entity: EntityRef) -> Option<i32> {
        match self.ecs_of(entity) {
            Some(_) => Some(MAX_AIR),
            None => self.host.entity_max_air_supply(entity),
        }
    }

    pub(super) fn r_entity_is_silent(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Silent>(e).is_some()),
            None => self.host.entity_is_silent(entity),
        }
    }

    /// `Entity.isCurrentlyGlowing`: the glowing marker `setGlowingTag` sets.
    pub(super) fn r_entity_glowing(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Glowing>(e).is_some()),
            None => self.host.entity_glowing(entity),
        }
    }

    /// `Entity.getTicksFrozen`: the ticks counted in powder snow.
    pub(super) fn r_entity_ticks_frozen(&self, entity: EntityRef) -> Option<i32> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Frozen>(e).map_or(0, |f| f.0)),
            None => self.host.entity_ticks_frozen(entity),
        }
    }

    /// `Entity.isInvisible`: the invisibility effect, or the flag of an item frame or an armor stand.
    pub(super) fn r_entity_is_invisible(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<effects::Invisible>(e).is_some() || self.world().get::<frame_item::Invisible>(e).is_some_and(|i| i.0)),
            None => self.host.entity_is_invisible(entity),
        }
    }

    /// `LivingEntity.isFallFlying`: the gliding state of the elytra.
    pub(super) fn r_living_is_gliding(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(self.world().get::<FallFlying>(e).is_some()),
            None => self.host.living_is_gliding(living),
        }
    }

    /// `LivingEntity.isSleeping`: the entity lies in a bed.
    pub(super) fn r_living_is_sleeping(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(host_read_ai::sleeping_pos(self.world(), e).is_some()),
            None => self.host.living_is_sleeping(living),
        }
    }

    /// `LivingEntity.isJumping`: the jump the goals asked for this tick.
    pub(super) fn r_living_is_jumping(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(self.world().get::<Jump>(e).is_some_and(|j| j.0)),
            None => self.host.living_is_jumping(living),
        }
    }

    /// `LivingEntity.isBaby`: the entity has its growing timer.
    pub(super) fn r_living_baby(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(self.world().get::<GrowAt>(e).is_some()),
            None => self.host.living_baby(living),
        }
    }

    /// `Entity.getBbWidth` and `getBbHeight`: the bounding box's size in x and y.
    pub(super) fn r_entity_dimensions(&self, entity: EntityRef) -> Option<[f64; 2]> {
        match self.ecs_of(entity) {
            Some(e) => host_read::bounds(self.world(), e).map(|b| {
                let s = b.size();
                [s.x, s.y]
            }),
            None => self.host.entity_dimensions(entity),
        }
    }

    /// `Entity.isMultipartEntity`: the entity is made of parts (the ender dragon).
    pub(super) fn r_entity_is_multipart(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => Some(self.world().get::<Parts>(e).is_some()),
            None => self.host.entity_is_multipart(entity),
        }
    }

    /// `Mob.isPersistenceRequired`: the entity keeps its place when no player is near (`Persistent`).
    pub(super) fn r_mob_persistence_required(&self, mob: MobRef) -> Option<bool> {
        match self.ecs_of(mob.entity()) {
            Some(e) => Some(self.world().get::<Persistent>(e).is_some()),
            None => self.host.mob_persistence_required(mob),
        }
    }

    /// `LivingEntity.onClimbable`: the climbing state of the locomotion module.
    pub(super) fn r_living_is_climbing(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(self.world().get::<Climbing>(e).is_some()),
            None => self.host.living_is_climbing(living),
        }
    }

    /// `Entity.getControllingPassenger`: the rider the mount names first.
    pub(super) fn r_entity_controlling_passenger(&self, entity: EntityRef) -> Option<LivingRef> {
        match self.ecs_of(entity) {
            Some(e) => self.world().get::<Mount>(e)?.rider.and_then(|who| self.who_ref(who)).map(LivingRef::new),
            None => self.host.entity_controlling_passenger(entity),
        }
    }

    /// `Entity.isPassengerOfSameVehicle`: both ride one vehicle.
    pub(super) fn r_entity_is_passenger_of_same_vehicle(&self, entity: EntityRef, other: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => {
                let vehicle = host_vehicle::host_vehicle(self.world(), e);
                let other_vehicle = self.ecs_of(other).and_then(|o| host_vehicle::host_vehicle(self.world(), o));
                Some(vehicle.is_some() && vehicle == other_vehicle)
            }
            None => self.host.entity_is_passenger_of_same_vehicle(entity, other),
        }
    }

    /// `EntityType.fireImmune`: the species never burns.
    pub(super) fn r_entity_fire_immune(&self, entity: EntityRef) -> Option<bool> {
        match self.ecs_of(entity) {
            Some(e) => host_read::fire_immune(self.world(), e),
            None => self.host.entity_fire_immune(entity),
        }
    }

    /// `TamableAnimal.isTame`: the tame flag `tame` sets.
    pub(super) fn r_living_is_tamed(&self, living: LivingRef) -> Option<bool> {
        match self.ecs_of(living.entity()) {
            Some(e) => Some(self.world().get::<TameFlag>(e).is_some()),
            None => self.host.living_is_tamed(living),
        }
    }

    /// `Host::mob`: a handle the step holds is a mob; a player is no `MobRef`, so the rest is the game host's.
    fn lookup_mob(&self, entity: EntityRef) -> Option<MobRef> {
        match self.ecs_of(entity) {
            Some(_) => Some(MobRef::new(entity)),
            None => self.host.mob(entity),
        }
    }

    fn lookup_player(&self, uuid: u128) -> Option<mod_api::PlayerRef> {
        self.host.player(uuid)
    }

    fn lookup_item_entity(&self, entity: EntityRef) -> Option<mod_api::ItemEntityRef> {
        self.host.item_entity(entity)
    }

    fn lookup_block_entity(&self, level: mod_api::LevelRef, pos: [i32; 3]) -> Option<mod_api::BlockEntityRef> {
        self.host.block_entity(level, pos)
    }

    /// `Host::is_game_entity`-like test of the generated view: a handle the step does not hold and the game host names.
    fn is_game_entity(&self, entity: EntityRef) -> bool {
        self.ecs_of(entity).is_none() && self.host.entity(entity.id()) == Some(entity)
    }
}

/// `Entity.getMaxAirSupply` of the kinds that breathe air.
const MAX_AIR: i32 = 300;

/// The ticks left until `until`, the tick a timer ends at; negative once it has passed.
fn ticks_left(w: &World, e: Entity, until: i64) -> i32 {
    (until - now_of(w, e) as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// The ticks left of the `InvulnerableUntil` window: 0 when none runs.
fn invulnerable_left(w: &World, e: Entity) -> i32 {
    w.get::<InvulnerableUntil>(e).map_or(0, |u| ticks_left(w, e, u.0 as i64).max(0))
}

/// The sound source of the species of `e` (`Species.sound_source`).
fn species_sound(w: &World, e: Entity) -> Option<myriade_types::SoundSource> {
    let s = w.get::<SpeciesId>(e)?;
    Some(match w.get_resource::<SpeciesTable>()?.0.get(usize::from(s.0))?.sound_source {
        SoundCategory::Neutral => myriade_types::SoundSource::Neutral,
        SoundCategory::Hostile => myriade_types::SoundSource::Hostile,
        SoundCategory::Ambient => myriade_types::SoundSource::Ambient,
    })
}

/// The vanilla category name of a species, as the mob types spell it.
fn mob_category(name: &str) -> Option<MobCategory> {
    Some(match name {
        "monster" => MobCategory::Monster,
        "creature" => MobCategory::Creature,
        "ambient" => MobCategory::Ambient,
        "axolotls" => MobCategory::Axolotls,
        "underground_water_creature" => MobCategory::UndergroundWaterCreature,
        "water_creature" => MobCategory::WaterCreature,
        "water_ambient" => MobCategory::WaterAmbient,
        "misc" => MobCategory::Misc,
        _ => return None,
    })
}

/// `Entity.calculateViewVector` (`Mth` trig, radians from the degrees): the direction of `yaw` and `pitch`, the pitch moved by `turn` degrees.
fn view_vector(yaw: f32, pitch: f32, turn: f32) -> [f64; 3] {
    use horizon_core::geometry::trig::{cos, sin};
    const RAD: f32 = 0.017_453_292;
    let (p, y) = (f64::from((pitch + turn) * RAD), f64::from(-yaw * RAD));
    let (s_y, c_y, s_p, c_p) = (f64::from(sin(y)), f64::from(cos(y)), f64::from(sin(p)), f64::from(cos(p)));
    [s_y * c_p, -s_p, c_y * c_p]
}

/// The index of an equipment slot in `equip_stack` (main, off, head, chest, legs, feet, body, saddle); None for a slot a mob has no part of.
fn equipment_index(slot: EquipmentSlot) -> Option<usize> {
    Some(match slot {
        EquipmentSlot::MainHand => 0,
        EquipmentSlot::OffHand => 1,
        EquipmentSlot::Head => 2,
        EquipmentSlot::Chest => 3,
        EquipmentSlot::Legs => 4,
        EquipmentSlot::Feet => 5,
        EquipmentSlot::Body => 6,
        EquipmentSlot::Saddle => 7,
    })
}
