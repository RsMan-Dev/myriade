//! Mobs in the game loop: ticking with entity snapshots, natural and
//! chunk-generation spawning (`NaturalSpawner`), mob events (attacks,
//! breeding, conversions, drops), projectiles, player interactions
//! (feeding, shearing, milking, spawn eggs, throwing) and rendering.
mod combat_adaptations;
mod pufferfish_player;
mod death_weapon;
mod destroy_rules;
mod player_loot_entity;
mod residency;
pub(in crate::game) mod section_steps;
mod world_view;
use super::api_values::TryIntoApi;
use super::level_sounds::LevelSound;
use super::piston_push::Lid;
use super::{Dim, Game, InteractTarget, view_vector};
use horizon_client::{
    Result,
    blocks::Fluid,
    health::{Attack, HealthEvent, HurtSource},
    items::{ItemStack, Items},
    mobs::{
        Damage, DamageKind, Mob, MobEvent, MobWorld, PlayerView, Seen, SpawnSetup, Species, Who,
        ai::{eating_sound, eating_sound_is_bound, feed_animal, takes_food},
        entity_sounds::full_id,
        spawning,
    },
    collisions::SceneCollisions,
    particles::{ParticleOptions, ParticleType},
    projectiles::{Projectile, ProjectileKind, clip_including_border},
};
use horizon_client::ecs_bridge::EcsLevel;
use horizon_ecs_proto::spawn_sheet::SpawnSheet;
use horizon_core::{
    geometry::{Aabb, Direction, Vec3, traverse_blocks},
    position::BlockPos,
    random::{LegacyRandom, RandomSource, XoroshiroRandom},
};
use mod_api::SoundSource;
use std::sync::Arc;

pub(super) type SpawnReason = horizon_client::mobs::spawn_rules::Reason;

/// Whether vanilla calls `finalizeSpawn` for a mob a spawn site makes, and with
/// which reason. The reason is vanilla's `EntitySpawnReason` at that site.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SpawnSite {
    /// Vanilla calls `finalizeSpawn` with this reason: `finalize_spawn` is told.
    Finalizing(SpawnReason),
    /// Vanilla creates the mob without `finalizeSpawn` (a breeding, a block's or
    /// an item's mob): the game finalizes it as before, and no mod is told.
    Plain,
    Raw,
}

/// What a spawn site does with a mob after `finalize_spawn` was heard.
#[derive(Debug, PartialEq)]
pub(in crate::game) enum Finalized {
    /// Finalized as vanilla does; the site goes on to the class's follow-ups.
    Done,
    /// A mod skipped finalization: the mob spawns uninitialised.
    Skipped,
    /// A mod stopped the spawn: the mob is not added.
    Cancelled,
}

mod death_source;
pub(in crate::game) use death_source::death_hurt_source;

/// A dying entity whose items drop (`LivingDropsEvent`).
pub(super) struct DyingDrops<'a> {
    pub(super) entity: u32,
    pub(super) kind: &'a str,
    /// The UUID of a dying player.
    pub(super) player: Option<u128>,
    /// The killing blow; none when no mod listens.
    pub(super) source: Option<HurtSource<'a>>,
    /// Whether a player hurt it within 100 ticks.
    pub(super) recently_hit: bool,
}

/// A sound a native mod queued, sent when the host is free.
pub(super) struct NativeSound {
    pub dimension: &'static str,
    pub sound: String,
    pub at: [f64; 3],
    pub source: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
    /// The connection of a player who doesn't hear it, as
    /// `Level.playSound(except, ...)`.
    pub except: Option<u32>,
    /// The entity the sound follows (`ClientboundSoundEntityPacket`), sent
    /// from `at`, where it was when the mod queued the sound.
    pub entity: Option<i32>,
}

#[derive(Default)]
struct NormalActionFrame {
    events: std::collections::VecDeque<MobEvent>,
    broke: Vec<u32>,
    actor: Option<u32>,
}
#[derive(Default)]
struct NormalMobActions {
    root: NormalActionFrame,
    nested: Vec<NormalActionFrame>,
}
impl NormalMobActions {
    fn new(events: Vec<MobEvent>) -> Self {
        Self {
            root: NormalActionFrame {
                events: events.into(),
                broke: Vec::new(),
                actor: None,
            },
            nested: Vec::new(),
        }
    }
}
struct MobOutputBatch {
    normal: NormalMobActions,
    guests: Vec<(u32, u32, MobEvent)>,
}
// Backpressure applies to producer passes retained behind either producer or
// consumer failure, never to healthy mob counts. Empty delivery resets the quota.
// Sixteen retained passes permit 0.8 seconds of production at the vanilla tick rate.
const MAX_PENDING_MOB_PASSES: usize = 16;
#[derive(Default)]
struct PendingMobBatch {
    batches: std::collections::VecDeque<MobOutputBatch>,
    pushes: horizon_client::mobs::PendingPushes,
    retained_passes: usize,
    refusal_logged: bool,
}

/// Remove the current action before dispatch; retain only the unentered tail on unwind.
pub(super) struct RemainingActions<'a, T> {
    remaining: std::vec::IntoIter<T>,
    target: &'a mut Vec<T>,
}
impl<'a, T> RemainingActions<'a, T> {
    pub(super) fn new(target: &'a mut Vec<T>) -> Self {
        Self {
            remaining: std::mem::take(target).into_iter(),
            target,
        }
    }
}
impl<T> Iterator for RemainingActions<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.remaining.next()
    }
}
impl<T> Drop for RemainingActions<'_, T> {
    fn drop(&mut self) {
        let remaining = std::mem::replace(&mut self.remaining, Vec::new().into_iter());
        *self.target = remaining.collect();
    }
}

/// Mob-system state owned by the game.
#[derive(Default)]
pub(super) struct Creatures {
    /// The entities a family tells the mods ticked, kept between ticks
    /// (game/modding/native_ticks.rs).
    pub tick_buffer: Vec<(u32, super::native_ticks::TickKind)>,
    /// Wind bursts from a guest's turn, waiting for it to end (game/interface/wind.rs).
    pub wind_bursts: Vec<super::wind::Burst>,
    /// The wind charge whose burst is being handled, by entity id: which
    /// entities have already ticked in vanilla's spawn-ordered tick list
    /// (game/interface/wind.rs).
    pub wind_source: Option<horizon_client::join_seq::JoinSeq>,
    /// Wind pushes held until the entity's own tick has run, by entity id
    /// (game/interface/wind.rs `apply_late_wind_pushes`).
    pub late_wind_pushes: Vec<(u32, crate::game::Vec3)>,
    /// Mace smash knockback on the other players from a guest's turn, waiting
    /// for it to end (game/items/mace.rs).
    pub smash_pushes: Vec<super::mace::SmashPush>,
    /// Sweep attacks from a guest's turn, waiting for it to end
    /// (game/network/lan_pvp.rs).
    pub sweeps: Vec<super::lan_pvp::Sweep>,
    /// Explosion packets from a guest's turn, waiting for it to end
    /// (game/network/lan_particles.rs).
    pub lan_explosions: Vec<super::lan_particles::LanExplosion>,
    pending: std::collections::HashMap<&'static str, PendingMobBatch>,
    /// The mobs whose own tick ran in the last `tick_creature_entities`:
    /// `ServerLevel.tick` ticks only the entities in entity-ticking range, so
    /// what the passes after the loop do for a mob's `aiStep`, goals or brain
    /// (pickups, item searches) is for these alone.
    pub ticked: horizon_client::fast_hash::FxHashSet<u32>,
    pub deferred_hits: bool,
    /// The wandering trader spawner's clock.
    pub trader: super::wandering_trader::Spawner,
    /// When each player last put a parrot on a shoulder (`timeEntitySatOnShoulder`).
    pub shoulder_time: std::collections::HashMap<[i32; 4], i64>,
    /// The cat spawner's countdown (`CatSpawner.nextTick`).
    pub cat: super::cat_spawner::CatSpawner,
    /// Each level's custom spawners, built when its first tick asks (game/mobs/custom_spawners.rs).
    pub custom_spawners: std::collections::HashMap<super::Dim, mod_api::CustomSpawners>,
    /// Native sounds waiting for the server host to be available.
    pub lan_sounds: Vec<NativeSound>,
    /// Particles from a guest's turn, waiting for it to end.
    pub lan_particles: Vec<super::lan_particles::QueuedParticle>,
}
pub(super) fn seen(m: &Mob, player: Option<[i32; 4]>, items: &Items) -> Seen {
    m.seen(player, items)
}
/// Whether a mob a packet named may take the click: `mob_hit` skips a mob in
/// its death animation, the one the player rides (it shares the player's root
/// vehicle) and the camera entity the pick starts from, so a packet that names
/// one of those does nothing rather than reaching the feed, shear, leash or
/// name paths the pick path refuses.
pub(super) fn named_mob_pickable(
    alive: bool,
    id: u32,
    riding: Option<u32>,
    camera: Option<u32>,
) -> bool {
    alive && Some(id) != riding && Some(id) != camera
}

/// What a block's `onProjectileHit` reads of the projectile that struck it:
/// a thrown projectile or a fishing bobber (`FishingHook.onHitBlock` calls
/// `Projectile.onHitBlock` too).
#[derive(Clone, Copy, Debug)]
pub(super) struct ProjectileHitter {
    /// `Projectile.getOwner`.
    pub(super) owner: Option<Who>,
    /// On a server, the guest (by connection) who owns it.
    pub(super) guest: Option<u32>,
    /// An `AbstractArrow` (a target block resets after 20 ticks, not 8).
    pub(super) arrow: bool,
    /// Its entity type, for `target_hit`'s projectile predicate.
    pub(super) kind: &'static str,
    pub(super) position: Vec3,
}

impl Game<'_> {
    /// The game's own player's uuid and entity id — the identity behind
    /// [`Self::player_view`], so a tick that lends no guest's view names its
    /// player (arismoko/horizon#1077). None where the view stands for nobody: no player of
    /// the game's own, a fake one, or a guest's turn acting in its place.
    pub(super) fn player_identity(&self) -> Option<([i32; 4], u32)> {
        if self.lan_acting_fake || self.lan_acting_uuid.is_some() || self.player.is_none() {
            return None;
        }
        let uuid = self.level.player_uuid?;
        let id = u32::try_from(self.player.as_ref()?.entity_id).ok()?;
        Some((uuid, id))
    }
    pub(super) fn player_view(&self) -> Option<PlayerView> {
        if self.lan_acting_fake {
            return None;
        }
        let p = self.player.as_ref()?;
        let w = f64::from(p.dimensions.width) / 2.0;
        let bounds = Aabb::new(
            Vec3::new(p.position.x - w, p.position.y, p.position.z - w),
            Vec3::new(
                p.position.x + w,
                p.position.y + f64::from(p.dimensions.height),
                p.position.z + w,
            ),
        );
        Some(PlayerView {
            position: p.position,
            bounds,
            eye: [
                p.position.x,
                p.position.y + f64::from(p.dimensions.eye_height),
                p.position.z,
            ],
            alive: !p.health.dead_or_dying(),
            invulnerable: p.invulnerable
                || p.health.permanently_invulnerable
                || p.health.invulnerable_time > 0,
            // EntitySelector.NO_CREATIVE_OR_SPECTATOR.
            creative: self.creative || self.spectator(),
            sneaking: p.shift_key_down,
            swimming: p.swimming,
            look: view_vector(self.yaw, self.pitch),
            main_hand: self.held().map(|s| s.item.clone()),
            off_hand: self.offhand.as_ref().map(|s| s.item.clone()),
            head: self.armor[0].as_ref().map(|s| s.item.clone()),
            chest: self.armor[1].as_ref().map(|s| s.item.clone()),
            legs: self.armor[2].as_ref().map(|s| s.item.clone()),
            feet: self.armor[3].as_ref().map(|s| s.item.clone()),
            armor_cover: self.armor.iter().filter(|a| a.is_some()).count() as f32 / 4.0,
            invisible: self.potions.effects.get("invisibility").is_some(),
            spectator: self.spectator(),
            gold_armor: self.wearing_gold(),
            hurt_by: self.player_hurt_by(),
            last_hurt: self.combat.last_hurt_mob.map(|(id, at)| (Who::Mob(id), at)),
            velocity: p.velocity,
            yaw: self.yaw,
            sprinting: p.sprinting,
            riding: self.riding.is_some(),
            hero: self.potions.effects.get("hero_of_the_village").is_some(),
            effects: self.potions.effects.list.iter().map(|e| e.id).collect(),
            uuid: self.acting_uuid(),
            allies: std::sync::Arc::new(self.player_allies()),
            remote: false,
            sleeping: self.sleeping.is_some(),
            sleep_timer: self.sleep_timer,
            passenger: self.riding.is_some(),
            riding_vehicle: self.riding,
            vehicle: self.riding.and_then(|id| {
                // A boat (`BoatMarker`) or a mount steered by the look of its rider (the nautilus kind); other vehicles are not followed.
                let nautilus = self.ecs.boat_status(id).is_none();
                if nautilus && !self.ecs.steers_by_look(id) {
                    return None;
                }
                Some(horizon_client::mobs::Vehicle {
                    nautilus,
                    bounds: self.ecs.bounds(id)?,
                    velocity: self.ecs.velocity(id)?,
                })
            }),
        })
    }
    /// Create a mob of `kind` at a position, finalized like a natural spawn.
    pub(super) fn spawn_mob(
        &mut self,
        kind: &str,
        at: [f64; 3],
        persistent: bool,
        site: SpawnSite,
    ) -> Option<u32> {
        self.ecs_spawn_mob(kind, at, persistent, None, site)
    }
    /// [`spawn_mob`](Self::spawn_mob) for a creation that snaps the mob to a
    /// yaw (`snapTo(pos, yaw, 0)`): the facing takes no draw from the level.
    pub(super) fn spawn_mob_facing(
        &mut self,
        kind: &str,
        at: [f64; 3],
        persistent: bool,
        site: SpawnSite,
        yaw: f32,
    ) -> Option<u32> {
        self.spawn_mob_facing_at(kind, at, persistent, site, yaw, None)
    }
    /// `spawn_mob_facing` whose finalize draws read the place of `finalize_at` (`finalizeSpawn` before `snapTo`).
    pub(super) fn spawn_mob_facing_at(
        &mut self,
        kind: &str,
        at: [f64; 3],
        persistent: bool,
        site: SpawnSite,
        yaw: f32,
        finalize_at: Option<[f64; 3]>,
    ) -> Option<u32> {
        self.ecs_spawn_mob_at(kind, at, persistent, Some(yaw), site, finalize_at)
    }
    /// Create a mob of `kind` at a position without `finalizeSpawn`'s rolls,
    /// matching vanilla `EntityType.create(level, reason)`.
    pub(super) fn spawn_mob_raw(
        &mut self,
        kind: &str,
        at: [f64; 3],
        persistent: bool,
    ) -> Option<u32> {
        self.spawn_mob(kind, at, persistent, SpawnSite::Raw)
    }
    /// `SummonCommand` and `SpawnEggItem`: vanilla spawns the mob, but it
    /// never calls `Mob.setPersistenceRequired` for it, so the mob is not
    /// persistent: the despawn rule holds it like any other mob, and only
    /// its NBT can make it persistent.
    pub(super) fn spawn_mob_spawned(
        &mut self,
        kind: &str,
        at: [f64; 3],
        site: SpawnSite,
    ) -> Option<u32> {
        // Dragons have a separate entity implementation, but eggs use the same spawn path.
        if matches!(kind, "ender_dragon" | "minecraft:ender_dragon") {
            return self.spawn_dragon_spawned(at, site);
        }
        // The spawn is not a natural one (no jockeys, the patrol leader roll
        // stays), but the mob is not persistent: vanilla's SummonCommand
        // never calls setPersistenceRequired. The flag is false from the
        // start, before the mob joins the level, so a mod's own write in the
        // entity_join hook survives it - clearing it after the join would
        // wipe what the hook set (Mob.setPersistenceRequired from
        // entity_join_level).
        self.spawn_mob(kind, at, false, site)
    }
    /// `Mob.burnUndead` for a mob in daylight with an item on its sun
    /// protection slot (the head, or the body for a zombie horse or
    /// nautilus): `hurtAndBreak(random.nextInt(2))` on a damageable item, and
    /// when it breaks the slot empties and `onEquippedItemBroken` broadcasts
    /// the break event.
    fn wear_sun_protection(&mut self, id: u32) {
        use horizon_core::equipment_slot::EquipmentSlot;
        let Some(species) = self.creature_species(id) else {
            return;
        };
        // `Mob.sunProtectionSlot`: zombie horses and zombie nautiluses wear it on the body.
        let slot = if matches!(species, Species::ZombieHorse | Species::ZombieNautilus) {
            EquipmentSlot::Body
        } else {
            EquipmentSlot::Head
        };
        let body = slot == EquipmentSlot::Body;
        let Some(stack) = self.ecs.equipment_stack(id, slot as usize) else {
            return;
        };
        if !horizon_client::stations::is_damageable(self.res.items(), &stack) {
            return;
        }
        let Some(at) = self.ecs.position(id) else {
            return;
        };
        let amount = self.ecs.next_int_bound(id, 2).unwrap_or(0) as u32;
        if amount == 0 {
            return;
        }
        let (updated, broken) = self.damage_living_stack(&stack, amount, id);
        if self.ecs.equipment_stack(id, slot as usize).as_ref() != Some(&stack) {
            return;
        }
        self.ecs.put_equipment_slot(id, slot as usize, updated);
        if broken.is_some() {
            // LivingEntity.entityEventForEquipmentBreak: head 49, body 65.
            self.lan_entity_event(id as i32, if body { 65 } else { 49 }, at);
        }
    }
    /// `LivingEntity.updateFallFlying`'s wear, `ItemStack.hurtAndBreak(1, this,
    /// slot)` on the glider `MobEvent::GliderWear` picked: Unbreaking (from the
    /// level's random, `Enchantment.modifyItemFilteredCount`) may save it, and
    /// the rest goes on the stack where it is worn. Only a part's `break_now`
    /// breaks one (`canGlideUsing` picks none a point would break): emptied, with its event.
    pub(in crate::game) fn wear_mob_glider(
        &mut self,
        id: u32,
        slot: horizon_core::equipment_slot::EquipmentSlot,
    ) {
        let Some(stack) = self.ecs.equipment_stack(id, slot as usize) else {
            return;
        };
        if !horizon_client::stations::is_damageable(self.res.items(), &stack) {
            return;
        }
        let (updated, broken) = self.damage_living_stack(&stack, 1, id);
        let Some(at) = self.ecs.position(id) else {
            return;
        };
        self.ecs.put_equipment_slot(id, slot as usize, updated);
        if broken.is_some() {
            let event = horizon_client::mobs::item_break::equipment_break_event(slot);
            self.lan_entity_event(id as i32, event, at);
        }
    }
    /// `LivingEntity.hurtArmor` -> `LivingEntity.doHurtEquipment`, from the hit
    /// `MobEvent::ArmorHurt` reported. Only `Horse` overrides `hurtArmor` for a
    /// mob, and its override asks for the BODY slot alone, so the body piece is
    /// the only one this wears: it loses `max(1, damage / 4)` durability,
    /// Unbreaking applied, and at its maximum the slot empties and the break
    /// event goes out (`LivingEntity.entityEventForEquipmentBreak`: 65 for the
    /// body — 49 to 52 are the four armour slots, which only a player's own
    /// pieces reach). A fire hit leaves a fire-resistant piece alone
    /// (`ItemStack.canBeHurtBy`).
    fn hurt_mob_armor(&mut self, id: u32, damage: f32, source: &HurtSource<'_>) {
        use horizon_core::equipment_slot::EquipmentSlot;
        if damage <= 0.0 {
            return;
        }
        let Some(at) = self.ecs.position(id) else {
            return;
        };
        let amount = ((damage / 4.0) as u32).max(1);
        let name = source.damage_type;
        // A piece's maximum, or none when it may not lose durability at all.
        // `doHurtEquipment` starts from the item's `equippable` component
        // (`Equippable equippable = item.get(DataComponents.EQUIPPABLE)`), so a
        // damageable item with no such component — a sword put in a horse's body
        // slot by `/item replace` — never wears; `equip_damage_on_hurt` alone
        // cannot say that, since it defaults to true when the component is absent.
        fn wearable(me: &Game<'_>, stack: &ItemStack, name: &str) -> Option<u32> {
            let info = me.res.items().get(&stack.item)?;
            if info.equip_slot.is_none()
                || !horizon_client::item_sync::damage_on_hurt(stack, info.equip_damage_on_hurt)
                || info.max_damage == 0
                || !horizon_client::stations::is_damageable(me.res.items(), stack)
                || !me.stack_can_be_hurt_by(stack, name).unwrap_or(true)
            {
                return None;
            }
            Some(info.max_damage)
        }
        let Some(body) = self.ecs.equipment_stack(id, EquipmentSlot::Body as usize) else {
            return;
        };
        if wearable(self, &body, name).is_none() {
            return;
        }
        // ArmorHurtEvent for a mob's body armor.
        let mut amount = amount;
        if mod_api::__private::listened(mod_api::HookId::ArmorHurt)
            && let Ok(body_snapshot) = body.try_into_api()
        {
            let kind = self.ecs.kind(id).unwrap_or_default().to_owned();
            let mut entries = vec![mod_api::ArmorHurt {
                slot: EquipmentSlot::Body,
                stack: body_snapshot,
                damage: super::durability_damage(damage),
                new_damage: super::durability_damage(damage),
            }];
            let mut cancel = false;
            self.native_armor_hurt(None, id, &kind, source, &mut entries, &mut cancel);
            if cancel {
                return;
            }
            amount = super::answered_wear(&entries, EquipmentSlot::Body, amount);
            if amount == 0 {
                return;
            }
            // A mod may have swapped the piece: it is not worn by this hit.
            if self.ecs.equipment_stack(id, EquipmentSlot::Body as usize) != Some(body.clone()) {
                return;
            }
        }
        let (updated, broken) = self.damage_living_stack(&body, amount, id);
        if self.ecs.equipment_stack(id, EquipmentSlot::Body as usize).as_ref() != Some(&body) {
            return;
        }
        self.ecs.put_equipment_slot(id, EquipmentSlot::Body as usize, updated);
        if broken.is_some() {
            // LivingEntity.onEquippedItemBroken through the level's broadcast.
            let event =
                horizon_client::mobs::item_break::equipment_break_event(EquipmentSlot::Body);
            self.lan_entity_event(id as i32, event, at);
        }
    }
    /// Default position for a summon command (3 blocks in front of the player).
    pub(super) fn default_summon_position(&self) -> Result<[f64; 3]> {
        let p = self.player.as_ref().ok_or("no player yet")?;
        let d = view_vector(self.yaw, 0.0);
        Ok([
            p.position.x + d[0] * 3.0,
            p.position.y,
            p.position.z + d[2] * 3.0,
        ])
    }
    /// The `summon` command: spawn a mob in front of the player or at a position.
    pub(super) fn summon_command_mob(
        &mut self,
        kind: &str,
        at: Option<[f64; 3]>,
        baby: bool,
    ) -> Result<u32> {
        let at = match at {
            Some(a) => a,
            None => self.default_summon_position()?,
        };
        let id = self
            .spawn_mob_spawned(kind, at, SpawnSite::Finalizing(SpawnReason::Command))
            .ok_or_else(|| format!("unknown or unsupported mob {kind}"))?;
        if baby {
            // A sniffer's start age (`BABY_START_AGE`) is not separated from the others' here.
            self.ecs.set_baby(id, true);
        }
        Ok(id)
    }
    /// The `mobs` command: nearby mobs with position, health and running goals.
    pub(in crate::game) fn control_mobs(&self, radius: f64) -> serde_json::Value {
        let center = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
        self.control_mobs_at(radius, center)
    }
    pub(super) fn control_mobs_at(&self, radius: f64, center: Vec3) -> serde_json::Value {
        // RESTE-SANS-ECS: the goals, gear, pois, villager, raider, riding and body keys had no ECS read here.
        let c = [center.x, center.y, center.z];
        let list: Vec<serde_json::Value> = self
            .ecs
            .ids_near(c, radius)
            .into_iter()
            .filter(|&id| self.ecs.is_mob(id))
            .filter_map(|id| {
                let pos = self.ecs.position(id)?;
                let d2: f64 = (0..3).map(|i| (pos[i] - c[i]).powi(2)).sum();
                (d2 <= radius * radius).then(|| {
                    serde_json::json!({
                        "id": id,
                        "kind": self.ecs.kind(id),
                        "pos": pos,
                        "health": self.ecs.health(id),
                        "baby": self.ecs.age(id).is_some_and(|a| a < 0),
                        "target": self.ecs.ai_target(id).map(|t| format!("{t:?}")),
                        "on_ground": self.ecs.motion(id).is_some_and(|m| m.on_ground),
                    })
                })
            })
            .collect();
        let count: usize = self.ecs.count_by_kind().iter().map(|(_, n)| n).sum();
        serde_json::json!({ "count": count, "near": list })
    }
    /// Highest motion-blocking (no leaves) block + 1 (Heightmap MOTION_BLOCKING_NO_LEAVES).
    pub(super) fn motion_top(&self, x: i32, z: i32) -> i32 {
        let top = if self.dim == Dim::Nether { 127 } else { 319 };
        for y in (self.world.bottom_y()..=top).rev() {
            let f = self.res.facts().get(self.world.block(x, y, z));
            if (f.full_collision || f.fluid != Fluid::None) && !f.name.ends_with("leaves") {
                return y + 1;
            }
        }
        self.world.bottom_y()
    }
    /// What the generator finalizes this level's structure mobs with.
    pub(super) fn share_spawn_conditions(&self) {
        if let Some(terrain) = self.end.locator() {
            terrain.set_spawn_conditions(self.difficulty_id() as u8, self.world.day_time);
        }
    }
    /// `Mob.finalizeSpawn`'s event, the half listeners own: they hear the
    /// spawn, cancel it, take it over, set its difficulty, edit the mob
    /// through its lend and change the pack's `group`, which finalization then
    /// reads. The decision and the difficulty the spawn finalizes with come
    /// back together.
    /// The spawn's kind, position and sheet come from the caller; the group the
    /// listeners change is the sheet's.
    pub(super) fn finalize_spawn_event(
        &self,
        kind: &str,
        at: [f64; 3],
        reason: SpawnReason,
        spawner: Option<mod_api::SpawnSource>,
        sheet: &mut SpawnSheet,
    ) -> (Finalized, spawning::LocalDifficulty) {
        let difficulty = self.local_difficulty(at);
        if !mod_api::__private::listened(mod_api::HookId::FinalizeSpawn) {
            return (Finalized::Done, difficulty);
        }
        self.finalize_spawn_lent(kind, at, reason, spawner, difficulty, Some(sheet))
    }
    /// `Mob.getMaxSpawnClusterSize` as the entity parts of the spawn's type answer it
    /// (`EntityPart::spawn_cluster_size`), asked with the sheet lent as it stands before the entity
    /// joins the level: the first part in dispatch order to answer wins, and none answering leaves
    /// vanilla's `vanilla` to the spawner. A type with no part costs one table lookup. Server-side.
    pub(super) fn spawn_cluster_size(
        &self,
        kind: &str,
        at: [f64; 3],
        sheet: Option<&mut SpawnSheet>,
        reason: SpawnReason,
        spawner: Option<mod_api::SpawnSource>,
        vanilla: u32,
    ) -> Option<u32> {
        if !mod_api::__private::has_entity_parts() {
            return None;
        }
        let parts = mod_api::__private::entity_parts();
        let list = parts.entity_parts(kind);
        if list.is_empty() {
            return None;
        }
        self.lend_unspawned(kind, at, sheet, |lent, _| {
            list.iter().find_map(|attached| {
                let _guard =
                    mod_api::content::crash::enter_native_mod(mod_api::content::crash::ModCall {
                        id: attached.mod_id(),
                        hook: "spawn_cluster_size",
                        part: Some(attached.type_name()),
                    });
                attached
                    .part()
                    .spawn_cluster_size(self, lent, reason, spawner, vanilla)
            })
        })
    }
    /// The shared finalization decision for a mob type the game keeps apart
    /// from mobs (the dragon), before the entity joins the level: its lend has
    /// a kind and a position and no mob.
    pub(in crate::game) fn finalize_spawn_at(
        &self,
        kind: &str,
        at: [f64; 3],
        reason: SpawnReason,
        spawner: Option<mod_api::SpawnSource>,
        sheet: Option<&mut SpawnSheet>,
    ) -> (Finalized, spawning::LocalDifficulty) {
        let difficulty = self.local_difficulty(at);
        if !mod_api::__private::listened(mod_api::HookId::FinalizeSpawn) {
            return (Finalized::Done, difficulty);
        }
        self.finalize_spawn_lent(kind, at, reason, spawner, difficulty, sheet)
    }
    /// Fires `finalize_spawn` with the sheet lent to it and the group data as the
    /// API's; a group the listener changed comes back as the sheet's.
    #[allow(clippy::too_many_arguments)]
    fn finalize_spawn_lent(
        &self,
        kind: &str,
        at: [f64; 3],
        reason: SpawnReason,
        spawner: Option<mod_api::SpawnSource>,
        difficulty: spawning::LocalDifficulty,
        mut sheet: Option<&mut SpawnSheet>,
    ) -> (Finalized, spawning::LocalDifficulty) {
        let mut heard = mod_api::Difficulty::from(difficulty);
        let (mut spawn_cancelled, mut skip) = (false, false);
        let given = sheet
            .as_ref()
            .and_then(|s| s.group.as_ref())
            .and_then(|g| super::native_unspawned_mob::native_spawn_group(kind, g));
        let mut native_group = given.clone();
        self.lend_unspawned(kind, at, sheet.as_deref_mut(), |mob, edits| {
            self.native_finalize_spawn(mod_api::__private::FinalizeSpawnCall {
                dimension: self.dim.id(),
                kind,
                reason,
                pos: at,
                spawner,
                difficulty: &mut heard,
                spawn_cancelled: &mut spawn_cancelled,
                mob,
                edits,
                group: &mut native_group,
                cancel: &mut skip,
            });
        });
        if let (Some(s), true) = (sheet, native_group != given) {
            s.group = native_group
                .as_ref()
                .and_then(|g| super::native_unspawned_mob::spawn_group(kind, g));
        }
        if spawn_cancelled {
            return (Finalized::Cancelled, difficulty);
        }
        if skip {
            return (Finalized::Skipped, difficulty);
        }
        (Finalized::Done, heard.into())
    }
    /// `ServerLevel.getCurrentDifficultyAt` in the level in place.
    pub(super) fn local_difficulty(&self, at: [f64; 3]) -> spawning::LocalDifficulty {
        local_difficulty_in(&self.world, at)
    }
    /// Generation's animals for the chunks generated since, once lit: chunk
    /// generation places them whether the level ticks or not.
    pub(super) fn spawn_fresh_chunks(&mut self) {
        if !self.world.chunks.has_fresh() {
            return;
        }
        let fresh: Vec<(i32, i32)> = self
            .world
            .chunks
            .fresh_keys_ordered()
            .into_iter()
            .filter(|k| self.world.chunks.get(k).is_some_and(|c| c.fresh && c.lit))
            .collect();
        for key in fresh {
            if let Some(c) = self.world.chunks.get_mut(&key) {
                Arc::make_mut(c).fresh = false;
            }
            self.world.chunks.remove_fresh(&key);
            self.generate_chunk_packs(key.0, key.1);
        }
    }
    /// `NoiseBasedChunkGenerator.spawnOriginalMobs`: the creature packs of a freshly generated chunk, placed by the ECS with the region's
    /// random. The settings' `disable_mob_generation` skips the pass, which is how the End's preset keeps its chunks empty (the dimension
    /// is also checked, so a preset that fails to load cannot start filling the End); a superflat world places no packs.
    fn generate_chunk_packs(&mut self, cx: i32, cz: i32) {
        if !self.world.chunks.contains_key(&(cx, cz)) || self.dim == Dim::End {
            return;
        }
        let Some(terrain) = self.end.locator() else {
            return;
        };
        if !terrain.noise_ground() || terrain.disable_mob_generation() {
            return;
        }
        let mut region = terrain.region_random(cx, cz);
        let any = region.as_any();
        let mut ecs = std::mem::replace(&mut self.ecs, EcsLevel::inert());
        let mw = self.mob_world_base(&[]);
        if let Some(r) = any.downcast_mut::<LegacyRandom>() {
            ecs.spawn_chunk_packs(&mw, cx, cz, r);
        } else if let Some(r) = any.downcast_mut::<XoroshiroRandom>() {
            ecs.spawn_chunk_packs(&mw, cx, cz, r);
        }
        drop(mw);
        self.ecs = ecs;
    }
    /// `DistanceManager.inEntityTickingRange` in an entity-ticking chunk
    /// (block coordinates), from every player's tickets (`update_chunk_tickets`).
    pub(super) fn entity_ticking_area(&self) -> impl Fn(i32, i32) -> bool + use<> {
        let levels = self.tickets.levels();
        move |x, z| levels.entity_ticking_at(x, z)
    }

    fn deliver_pending_mob_actions(&mut self, level_id: &'static str) -> Result<()> {
        let mut pending = self.creatures.pending.remove(level_id).unwrap_or_default();
        let delivery = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            while let Some(batch) = pending.batches.front_mut() {
                self.creatures.deferred_hits = true;
                let result = self.handle_mob_events_pending(&mut batch.normal);
                self.creatures.deferred_hits = false;
                result?;
                self.lan_guest_mob_events(&mut batch.guests);
                pending.batches.pop_front();
            }
            if !pending.pushes.is_empty() {
                // RESTE-SANS-ECS: the pushes were applied to the host's mobs, and the ECS has no route for them yet.
                // As in the producer's successful final sweep, removed targets
                // have no remaining owner to receive a push.
                pending.pushes = Default::default();
            }
            Ok(())
        }));
        if pending.batches.is_empty() && pending.pushes.is_empty() {
            pending.retained_passes = 0;
            pending.refusal_logged = false;
        }
        self.creatures.pending.insert(level_id, pending);
        match delivery {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// The damage source a mob died to (`die`'s), its direct entity included when the mob
    /// recorded one ([`Mob::last_direct`]).
    fn last_blow(&self, m: &Mob) -> Option<horizon_client::advancements::DamageView> {
        let (d, _) = m.last_damage?;
        let mut view = self.damage_view(d);
        if let Some(kind) = m.last_direct {
            view.direct = Some(horizon_client::advancements::EntityView {
                kind: kind.into(),
                position: d.from.unwrap_or_default(),
                ..Default::default()
            });
            view.is_direct = false;
        }
        Some(view)
    }
    /// A mob's damage source as advancement predicates see it: its type, the
    /// causing entity, and the projectile when one dealt it.
    pub(in crate::game) fn damage_view(
        &self,
        d: Damage,
    ) -> horizon_client::advancements::DamageView {
        let source = match d.attacker {
            // Its callers are KilledTrigger's: the source as their killing_blow sees it.
            Some(Who::Player) => Some(self.player_entity_for_any(&[
                "minecraft:player_killed_entity",
                "minecraft:kill_mob_near_sculk_catalyst",
            ])),
            Some(Who::Mob(id)) => self.ecs_host_view(id),
            None => None,
        };
        let projectile = match d.kind {
            DamageKind::Arrow => Some("minecraft:arrow"),
            DamageKind::Fireball => Some("minecraft:fireball"),
            DamageKind::WitherSkull => Some("minecraft:wither_skull"),
            DamageKind::Fireworks => Some("minecraft:firework_rocket"),
            _ => None,
        };
        let direct = match projectile {
            Some(kind) => Some(horizon_client::advancements::EntityView {
                kind: kind.into(),
                position: d.from.unwrap_or_default(),
                ..Default::default()
            }),
            None => source.clone(),
        };
        horizon_client::advancements::DamageView {
            kind: source_damage_type(d).to_owned(),
            // DamageSource.isDirect: the causing entity is the direct one.
            is_direct: projectile.is_none(),
            direct,
            source,
            ..Default::default()
        }
    }
    /// `dropAllDeathLoot` through `mobs::drops`: the entity loot table from the
    /// world's random sequence, equipment by drop chance, species extras and
    /// the experience orbs, spawned in order.
    /// The UUID of the player `last_hurt_by_player` keeps: the guest on
    /// connection `conn` (None once they left), else the player in place
    /// (None on a server, which has none of its own).
    /// The entity id of the player `killer_uuid` names for `conn`.
    pub(in crate::game) fn player_entity_id_by_connection(&self, conn: Option<u32>) -> Option<i32> {
        match conn {
            Some(conn) if self.lan_acting != Some(conn) => {
                Some(self.lan.as_ref()?.hub.guest(conn)?.entity_id)
            }
            Some(_) => Some(self.own_entity_id()),
            None => None,
        }
    }
    fn killer_uuid(&mut self, conn: Option<u32>) -> Option<u128> {
        match conn {
            // During that guest's own turn, the player in place has their UUID.
            Some(conn) if self.lan_acting != Some(conn) => {
                Some(self.lan.as_ref()?.hub.guest(conn)?.uuid)
            }
            Some(_) => Some(self.native_acting_id()),
            None => None,
        }
    }
    /// `LivingEntity.dropExperience`: the experience a dying mob or player is worth once
    /// the mods have had their say; 0 when they cancel it.
    pub(super) fn living_experience_drop(
        &mut self,
        entity: u32,
        kind: &str,
        attacker: Option<u128>,
        original: i32,
    ) -> i32 {
        let (mut amount, mut cancel) = (original, false);
        self.native_living_experience_drop(
            entity,
            kind,
            attacker,
            original,
            &mut amount,
            &mut cancel,
        );
        if cancel { 0 } else { amount.max(0) }
    }

    /// Adds the items a dying entity dropped to the level, each at its own
    /// position. With mods, they first hear the list (`native_living_drops`):
    /// edited or canceled.
    pub(super) fn drop_death_items(
        &mut self,
        dying: &DyingDrops<'_>,
        stacks: Vec<(ItemStack, [f64; 3], i32)>,
        pickup_delay: u32,
        mut velocity: impl FnMut(&mut Self) -> Option<[f64; 3]>,
    ) {
        let Some(source) = &dying.source else {
            for (stack, position, age) in stacks {
                let mut random = super::spill::prepare_item_random(None);
                random.state.age = age;
                let motion = velocity(self).unwrap_or(random.velocity);
                self.add_item_entity_prepared(
                    stack,
                    position,
                    Some(motion),
                    pickup_delay,
                    Some(random),
                );
            }
            return;
        };
        // Vanilla builds each item entity (and draws its motion) when it
        // collects the drop, before the event sees the list.
        let mut drops: Vec<super::spill::BlockItemDrop> = stacks
            .into_iter()
            .map(|(stack, position, age)| {
                let random = super::spill::prepare_item_random(None);
                super::spill::BlockItemDrop {
                    random: Some(random),
                    stack,
                    position,
                    velocity: velocity(self).unwrap_or(random.velocity),
                    pickup_delay,
                    state: super::spill::DropState {
                        age,
                        ..Default::default()
                    },
                }
            })
            .filter(|drop| drop.stack.count > 0 && &*drop.stack.item != "minecraft:air")
            .collect();
        let mut cancel = false;
        if mod_api::__private::listened(mod_api::HookId::LivingDrops)
            && let Ok(mut snapshots) = drops.try_into_api()
        {
            self.native_living_drops(mod_api::__private::LivingDropsCall {
                entity: dying.entity,
                kind: dying.kind,
                player: dying.player,
                source,
                recently_hit: dying.recently_hit,
                drops: &mut snapshots,
                cancel: &mut cancel,
            });
            if let Ok(mut next) =
                super::api_values::item_drops_into_engine(&snapshots, self.items())
            {
                for (drop, original) in next.iter_mut().zip(&drops) {
                    drop.random = original.random;
                }
                drops = next;
            } else {
                cancel = false;
            }
        }
        if cancel {
            return;
        }
        for drop in drops {
            if drop.stack.count > 0 && &*drop.stack.item != "minecraft:air" {
                self.add_item_drop(drop);
            }
        }
    }
    /// Melee or ranged damage to another mob; whether the hit landed (its health fell).
    /// RESTE-SANS-ECS: the Mob's `damage_aimed` knockback and invulnerability catch-up, the axolotl attack sound, the infested spawns and the `zombie_killed` credit were host writes with no bridge setter.
    pub(in crate::game) fn mob_hits_mob(&mut self, target: u32, amount: f32, source: Damage) -> bool {
        let before = self.ecs.health(target);
        let by = match source.attacker {
            Some(Who::Mob(id)) => horizon_ecs_proto::bridge::host::HurtSource::Entity(id),
            Some(Who::Player) => match self.player_id_by_connection(self.lan_acting) {
                Some(p) => horizon_ecs_proto::bridge::host::HurtSource::Player(p),
                None => horizon_ecs_proto::bridge::host::HurtSource::Generic,
            },
            _ => horizon_ecs_proto::bridge::host::HurtSource::Generic,
        };
        self.ecs_hurt(target, amount, by);
        self.ecs.health(target) < before
    }
    /// The ECS id of the player on connection `conn`, for a mob's love cause or credit (None: no player).
    pub(in crate::game) fn player_id_by_connection(&self, conn: Option<u32>) -> Option<u32> {
        self.player_entity_id_by_connection(conn).map(|p| p as u32)
    }
    /// Remember the acting player for a mob's `lastHurtByPlayer`.
    pub(super) fn remember_mob_player_credit(&mut self, victim: u32) {
        if let Some(player) = self.player_id_by_connection(self.lan_acting) {
            self.ecs.record_credit(victim, Some(horizon_ecs_proto::targeting::Who::Player(player)));
        }
    }

    /// Resolve `EntityReference.getEntity` across levels. None inside means
    /// the player in place; Some is a guest connection.
    pub(super) fn resolve_player_credit(
        &self,
        credit: super::combat::PlayerCredit,
    ) -> Option<Option<u32>> {
        let uuid = horizon_client::lan::playerdata::uuid_ints(credit.0);
        if self.player.is_some()
            && horizon_client::mobs::tame::owned_by(Some(uuid), self.acting_uuid())
        {
            return Some(self.lan_acting);
        }
        let uuid = horizon_client::lan::playerdata::ints_uuid(uuid);
        self.lan
            .iter()
            .flat_map(|h| h.hub.guests.iter())
            .find(|g| g.placed && g.uuid == uuid)
            .map(|g| Some(g.conn))
    }
    /// `LivingEntity.dropFromGiftLootTable`: the gift table from its random
    /// sequence with the mob as `this` (a chicken's egg depends on its
    /// variant); the loot data of the game's resources decides, and without it
    /// the items' table engine (no entity) rolls it. Whether anything dropped.
    fn drop_gift_loot(&mut self, mob: u32, table: &'static str, at: [f64; 3]) -> bool {
        use horizon_world::loot::EntityLootParams;
        let loot = self.server_loot.get();
        let this = self.ecs.loot_entity(mob);
        let stacks = match (loot, this) {
            (Some(loot), Some(this)) if loot.has_table(table) => {
                let params = EntityLootParams {
                    this,
                    ..EntityLootParams::default()
                };
                let seed = self.level.seed;
                match loot.random_sequence(table) {
                    Some(id) => {
                        let random = self.rand_sequences.get(id, seed);
                        loot.roll_entity(table, random, &params)
                            .iter()
                            .filter(|s| !s.is_empty())
                            .map(from_loot)
                            .collect()
                    }
                    None => Vec::new(),
                }
            }
            _ => self
                .res
                .items()
                .loot_table(table, false, None, &mut self.world.random),
        };
        let dropped = !stacks.is_empty();
        for d in stacks {
            // The ItemEntity constructor's motion, from the entity's own
            // random (not the level's).
            self.spawn_item_own(d, at, 10);
        }
        dropped
    }
    /// Adds a projectile. A server launches it in the ECS level, which gives its id and flies it; a client keeps the copy the
    /// server sent in its list (display state). Its id.
    pub(in crate::game) fn add_projectile(
        &mut self,
        p: horizon_client::projectiles::Projectile,
    ) -> u32 {
        // Already in the level: its flight is the ECS's.
        if p.id != 0 {
            return p.id;
        }
        let Some(id) = self.ecs_launch_item(
            p.entity_type(),
            [p.position.x, p.position.y, p.position.z],
            [p.velocity.x, p.velocity.y, p.velocity.z],
            self.ecs_owner(p.owner),
            p.item.clone(),
        ) else {
            return 0;
        };
        let player = (p.owner == Some(Who::Player))
            .then(|| horizon_client::lan::playerdata::ints_uuid(self.acting_player_uuid()));
        let name = if player.is_some() {
            self.acting_player_name().to_owned()
        } else {
            String::new()
        };
        self.native_projectile_launch(player, &name, id, p.entity_type());
        id
    }
    /// `MushroomCow.shear`'s `sendParticles(EXPLOSION, x, getY(0.5), z, 1, 0, 0, 0, 0)`:
    /// one puff over a sheared mooshroom, half its `height` up, sent to the
    /// players within range.
    pub(super) fn mooshroom_shear_puff(&mut self, at: [f64; 3], height: f64) {
        let at = [at[0], at[1] + 0.5 * height, at[2]];
        let options = ParticleOptions::simple(ParticleType::Explosion);
        self.lan_level_particles(&options, at, [0.0; 3], 0.0, 1, None);
    }
    /// `Entity.sendBubbleColumnParticles` at the top of the column block `pos`: two
    /// splashes and two bubbles over it, which the level random places, sent to the
    /// players within range.
    pub(super) fn bubble_column_particles(&mut self, pos: BlockPos) {
        for _ in 0..2 {
            let r = &mut self.world.random;
            let splash = [
                f64::from(pos.x) + r.next_double(),
                f64::from(pos.y + 1),
                f64::from(pos.z) + r.next_double(),
            ];
            let bubble = [
                f64::from(pos.x) + r.next_double(),
                f64::from(pos.y + 1),
                f64::from(pos.z) + r.next_double(),
            ];
            let splash_options = ParticleOptions::simple(ParticleType::Splash);
            self.lan_level_particles(&splash_options, splash, [0.0; 3], 1.0, 1, None);
            let bubble_options = ParticleOptions::simple(ParticleType::Bubble);
            self.lan_level_particles(&bubble_options, bubble, [0.0, 0.01, 0.0], 0.2, 1, None);
        }
    }
    /// A mob's projectile enters the level.
    pub(in crate::game) fn launch_mob_projectile(&mut self, mut p: Projectile) {
        self.next_entity += 1;
        p.id = self.next_entity;
        self.spawn_mob_projectile(p);
    }
    /// The centre of the creature `id` on the bridge (half its height up); None when it is gone.
    pub(in crate::game) fn creature_center(&self, id: u32) -> Option<[f64; 3]> {
        let (p, _, h) = self.ecs_body(id)?;
        Some([p.x, p.y + f64::from(h) / 2.0, p.z])
    }
    /// `AbstractCubeMob.dealDamage`'s pitch: two draws from the cube's own random, in order.
    pub(in crate::game) fn cube_attack_pitch(&mut self, id: u32) -> f32 {
        let a = self.ecs.next_float(id).unwrap_or(0.0);
        let b = self.ecs.next_float(id).unwrap_or(0.0);
        (a - b) * 0.2 + 1.0
    }
    /// `Entity.igniteForSeconds`: `seconds * 20` ticks of fire, scaled by the
    /// target's burning-time attribute.
    fn ignite_entity(&mut self, target: Who, seconds: i32) -> Result<()> {
        match target {
            Who::Player => {
                if !self.creative
                    && let Some(p) = self.player.as_mut()
                {
                    p.ignite_for_ticks(seconds * 20)?;
                }
            }
            Who::Mob(id) => self.ecs.ignite(id, seconds * 20),
        }
        Ok(())
    }
    pub(super) fn handle_mob_events(&mut self, mut events: Vec<MobEvent>) -> Result<()> {
        self.handle_mob_events_reusable(&mut events)
    }

    pub(crate) fn handle_mob_events_reusable(
        &mut self,
        events: &mut Vec<MobEvent>,
    ) -> Result<()> {
        if self.ecs.is_active() {
            return self.apply_mob_events(events);
        }
        let mut actions = NormalMobActions::new(std::mem::take(events));
        let result = self.handle_mob_events_pending(&mut actions);
        *events = actions.root.events.into();
        result
    }

    /// One event through the creature code, for the variants `apply_mob_events` leaves to it.
    pub(in crate::game) fn handle_mob_events_legacy(&mut self, event: MobEvent) -> Result<()> {
        let mut actions = NormalMobActions::new(vec![event]);
        self.handle_mob_events_pending(&mut actions)
    }

    fn handle_mob_events_pending(&mut self, actions: &mut NormalMobActions) -> Result<()> {
        loop {
            let frame = actions.nested.last_mut().unwrap_or(&mut actions.root);
            let Some(e) = frame.events.pop_front() else {
                if actions.nested.pop().is_some() {
                    continue;
                }
                return Ok(());
            };
            let actor = frame.actor;
            let broke = &mut frame.broke;
            match e {
                MobEvent::ForPlayer { conn, event } => {
                    let mut event = *event;
                    // Projectiles are spawned in the world rather than run
                    // as a guest, but must retain the selected guest target.
                    if let Some(conn) = conn
                        && let MobEvent::Projectile(p) = &mut event
                        && let ProjectileKind::ShulkerBullet { target, .. } = &mut p.kind
                        && *target == Some(Who::Player)
                    {
                        *target = self.guest_identity(conn).map(|(_, id)| Who::Mob(id));
                        self.handle_mob_events(vec![event])?;
                    } else if let Some(conn) = conn {
                        if let Some(mob) = actor {
                            self.lan_guest_mob_events(&mut vec![(conn, mob, event)]);
                        } else {
                            // No nested block-break actor is in scope. A
                            // missing guest must not redirect to another player.
                            if let Some(result) =
                                self.as_guest(conn, |game| game.handle_mob_events(vec![event]))
                            {
                                result?;
                            }
                        }
                    } else {
                        self.handle_mob_events(vec![event])?;
                    }
                }
                MobEvent::BubbleColumnParticles { pos } => {
                    self.bubble_column_particles(BlockPos::new(pos[0], pos[1], pos[2]))
                }
                MobEvent::LandingDust {
                    state,
                    at,
                    count,
                    speed,
                } => self.landing_dust(state, Vec3::new(at[0], at[1], at[2]), count, speed),
                MobEvent::SquidCycle { mob, at } => self.lan_entity_event(mob as i32, 19, at),
                MobEvent::RabbitHop { mob, at } => self.lan_entity_event(mob as i32, 1, at),
                MobEvent::Drown { mob, at, bubbles } => self.mob_drown(mob, at, bubbles),
                MobEvent::PowderSnowParticle { at, velocity } => {
                    self.snowflake_particle(at, velocity);
                }
                MobEvent::SunProtectionWear { mob } => self.wear_sun_protection(mob),
                MobEvent::ArmorHurt {
                    mob,
                    amount,
                    source,
                } => self.hurt_mob_armor(mob, amount, &source),
                MobEvent::GliderWear { mob, slot } => self.wear_mob_glider(mob, slot),
                MobEvent::EntityEvent { mob, event, at } => {
                    self.lan_entity_event(mob as i32, event, at)
                }
                MobEvent::LevelParticles {
                    kind,
                    at,
                    spread,
                    speed,
                    count,
                } => {
                    if let Some(ty) = horizon_client::particles::ParticleType::from_name(kind) {
                        let options = horizon_client::particles::ParticleOptions::simple(ty);
                        self.lan_level_particles(&options, at, spread, speed, count, None);
                    }
                }
                // The window's own particles and its local sound are not sent.
                MobEvent::ItemParticles { .. } | MobEvent::LocalSound(..) => {}
                MobEvent::Sound(name, at, volume, pitch) => {
                    self.audio.play(&full_id(name), Some(at), volume, pitch)
                }
                MobEvent::ProfessionWorkSound(name, at, volume, pitch) => {
                    self.audio.play(&name, Some(at), volume, pitch)
                }
                MobEvent::SoundId(name, at, volume, pitch) => {
                    self.audio.play(&name, Some(at), volume, pitch)
                }
                MobEvent::Hiss { mob } => {
                    // RESTE-SANS-ECS: `variant_sound(m, "hiss")` takes the Mob's variant table; the cat's hiss is played here.
                    if let Some(at) = self.creature_center(mob) {
                        self.audio.play(&full_id("entity.cat.hiss"), Some(at), 1.0, 1.0);
                    }
                }
                MobEvent::BoundSound {
                    mob,
                    sound,
                    source,
                    at,
                    volume,
                    pitch,
                } => {
                    let name = full_id(sound);
                    let sound = LevelSound::new(&name, at, volume, pitch).in_source(source);
                    self.play_bound_sound(sound, mob, None);
                }
                MobEvent::LevelEventSound {
                    sound,
                    at,
                    volume,
                    spread,
                } => {
                    // WitherBoss.customServerAiStep: the spawn roar is global
                    // level event 1023 at the block position.
                    if sound == "entity.wither.spawn" {
                        let pos = at.map(|v| v.floor() as i32);
                        self.global_level_event(1023, pos, 0);
                    } else {
                        // The client's own level random: nothing drawn from the server's.
                        let mut r = horizon_core::random::LegacyRandom::new(
                            self.world.game_time.wrapping_mul(341_873_128_712),
                        );
                        let pitch = (r.next_float() - r.next_float()) * spread + 1.0;
                        self.audio.play(&full_id(sound), Some(at), volume, pitch)
                    }
                }
                MobEvent::GameEvent {
                    event, at, source, ..
                } if event == "minecraft:flap"
                    && matches!(self.creature_species(source), Some(Species::Chicken | Species::Parrot)) =>
                {
                    let ctx = self.who_context(Some(Who::Mob(source)), None);
                    self.post_game_event(event, at, ctx);
                }
                // A mob's own `gameEvent(..)` calls: Armadillo.rollUp/rollOut's ENTITY_ACTION,
                // PRIME_FUSE, EAT, a fox's DRINK, the shulker's lid, an evoker's vexes
                // (ENTITY_PLACE), TELEPORT, EQUIP/UNEQUIP, updateFallFlying's ELYTRA_GLIDE.
                MobEvent::GameEvent {
                    event, at, source, ..
                } if matches!(
                    event,
                    "minecraft:entity_action"
                        | "minecraft:prime_fuse"
                        | "minecraft:eat"
                        | "minecraft:drink"
                        | "minecraft:container_open"
                        | "minecraft:container_close"
                        | "minecraft:entity_place"
                        | "minecraft:teleport"
                        | "minecraft:equip"
                        | "minecraft:unequip"
                        | "minecraft:elytra_glide"
                ) =>
                {
                    let ctx = self.who_context(Some(Who::Mob(source)), None);
                    self.post_game_event(event, at, ctx);
                }
                MobEvent::GameEvent {
                    event: event @ ("minecraft:block_place" | "minecraft:block_destroy"),
                    at,
                    source,
                    state,
                } => {
                    let ctx = self.who_context(Some(Who::Mob(source)), Some(state));
                    self.post_game_event(event, at, ctx);
                }
                // `FarmlandBlock.turnToBaseBlock` and `TurtleEggBlock.decreaseEggs`:
                // the landing's own block change posts its game event, from the
                // mob that landed (`Context.of(entity, state)`) unless the block
                // destroyed itself under it (`Context.of(state)`).
                MobEvent::LandingGameEvent {
                    event,
                    at,
                    source,
                    state,
                } => {
                    let who = source.map(Who::Mob);
                    self.post_block_game_event(event, at, who, Some(state));
                }
                // The sculk layer tracks steps and bee/bat flaps itself
                // (`entity_motion_events`, `tick_mob_sculk_events`). Birds
                // above use their movement's flyDist/nextFlap threshold.
                MobEvent::GameEvent { .. } => {}
                MobEvent::AttackPlayer(amount, from) => {
                    self.combat.hit_landed =
                        self.mob_hits_player(amount, from, "minecraft:mob_attack");
                }
                // Bee.doHurtTarget: a sting that hurt leaves the stinger in the
                // body, then the poison; the bee's own stinger, calm-down and
                // sound follow only then too.
                MobEvent::Sting {
                    target,
                    amount,
                    poison_ticks,
                    source,
                } => self.sting(target, amount, poison_ticks, source),
                MobEvent::AttackPlayerWithEffect {
                    amount,
                    from,
                    effect,
                    ticks,
                    amplifier,
                } => {
                    self.combat.hit_landed =
                        self.mob_hits_player(amount, from, "minecraft:mob_attack");
                    if self.combat.hit_landed {
                        self.give_player_effect(effect, ticks, amplifier);
                    }
                }
                MobEvent::DamagePlayer { amount, from, kind } => {
                    self.mob_hits_player(amount, from, kind);
                }
                MobEvent::ElderCurse { at, silent } => self.elder_curse(at, silent),
                MobEvent::SetVelocity { target, velocity } => {
                    let v = Vec3::new(velocity[0], velocity[1], velocity[2]);
                    match target {
                        Who::Player => {
                            if let Some(p) = self.player.as_mut() {
                                p.velocity = v;
                            }
                        }
                        Who::Mob(id) => {
                            if let Some(e) = self.ecs.entity_of(id) {
                                self.ecs.set_velocity(e, v);
                            }
                        }
                    }
                }
                MobEvent::Push { target, velocity } => {
                    let v = Vec3::new(velocity[0], velocity[1], velocity[2]);
                    match target {
                        Who::Player => {
                            if let Some(p) = self.player.as_mut() {
                                p.velocity = p.velocity.add_vector(v);
                            }
                        }
                        Who::Mob(id) => {
                            if let (Some(e), Some(old)) = (self.ecs.entity_of(id), self.ecs.velocity(id)) {
                                self.ecs.set_velocity(e, old.add_vector(v));
                            }
                        }
                    }
                }
                MobEvent::Knockback {
                    target,
                    power,
                    x,
                    z,
                    source,
                    apply_blocking,
                } => match target {
                    // Ram knockback checks blocking independently of the hurt call.
                    Who::Player => self.mob_pushes_player(power, x, z, source, apply_blocking),
                    // RESTE-SANS-ECS: the Mob's knockback hook (`damage_level_of`, `source`) has no bridge argument; the bridge's strength and direction apply.
                    Who::Mob(id) => self.ecs.knockback(id, power, x, z),
                },
                MobEvent::LidPush {
                    shulker,
                    area,
                    delta,
                } => {
                    let (lid, delta) = (
                        Lid::Shulker(shulker),
                        Vec3::new(delta[0], delta[1], delta[2]),
                    );
                    // A client of a server pushes only its own player, as
                    // the shulker box's lid does.
                    {
                        self.lid_push(lid, area, delta);
                    }
                }
                // The sheep's red is set by the ECS evoker's `Wololo` (`behaviours/spell_use.rs`); the host event has nothing left to do.
                MobEvent::Wololo { .. } => {}
                MobEvent::Fangs {
                    at,
                    yaw,
                    delay,
                    owner,
                } => {
                    self.add_fangs(at, yaw, delay, owner);
                }
                MobEvent::ExplosionSource {
                    mob,
                    charged,
                    name,
                    in_water,
                    kind,
                } => {
                    self.combat.explosion_source = Some(mob);
                    self.combat.explosion_in_water = in_water;
                    // The creeper causes the blast's damage, but it is out of
                    // the mob list already: its name comes with the event.
                    self.combat.exploded.push((mob, name));
                    self.combat.exploded_kinds.push((mob, kind));
                    if charged {
                        self.combat.charged_exploders.push(mob);
                    }
                }
                MobEvent::LingeringCloud {
                    at,
                    effects,
                    hidden,
                    ambient,
                    color,
                } => {
                    self.spawn_creeper_cloud(at, effects, hidden, ambient, color);
                }
                MobEvent::Explode(at, radius, species, silent) => {
                    // The exploding mob causes the damage (Creeper.explodeCreeper).
                    let noted = self.combat.explosion_source.is_some();
                    let source_mob = self.combat.explosion_source.or_else(|| {
                        self.ecs.ids_near(at, 2.0).into_iter().find(|&id| {
                            self.ecs.is_mob(id)
                                && self.ecs.position(id).is_some_and(|p| {
                                    (p[0] - at[0]).powi(2) + (p[2] - at[2]).powi(2) < 1.0
                                })
                        })
                    });
                    self.combat.explosion_source = source_mob;
                    self.combat.explosion_source_named = noted;
                    let portal_proof = species == Species::SulfurCube
                        && source_mob.is_some_and(|id| {
                            self.portals
                                .entities
                                .get(&id)
                                .is_some_and(|p| p.cooldown > 0)
                        });
                    self.combat.explosion_cause = match species {
                        Species::Creeper => Some("minecraft:creeper"),
                        Species::Wither => Some("minecraft:wither"),
                        Species::SulfurCube => Some("minecraft:sulfur_cube"),
                        _ => None,
                    }
                    .map(|kind| horizon_client::advancements::EntityView {
                        kind: kind.into(),
                        position: at,
                        ..Default::default()
                    });
                    // Only a creeper or a wither empties the hives it blows up.
                    // The creeper is already gone from the list, so the event
                    // carries its kind. A sulfur cube's blast is TNT's.
                    let mut source =
                        super::explosion::ExplosionSource::default().with_silence(silent);
                    if let Some(id) = source_mob
                        && matches!(species, Species::Creeper | Species::Wither)
                    {
                        let kind = match species {
                            Species::Creeper => "minecraft:creeper",
                            _ => "minecraft:wither",
                        };
                        source = source.with_entity(id, kind);
                    }
                    let result = match species {
                        Species::Creeper | Species::Wither => self.with_hive_emptying_blast(|g| {
                            g.explode(source_mob, at, radius, source)
                        }),
                        Species::SulfurCube => self.explode_sulfur_cube(
                            source_mob,
                            at,
                            radius,
                            source.with_portal_proof(portal_proof),
                        ),
                        _ => self.explode(source_mob, at, radius, source),
                    };
                    self.combat.explosion_source = None;
                    self.combat.explosion_source_named = false;
                    self.combat.explosion_in_water = false;
                    self.combat.explosion_cause = None;
                    result?
                }
                MobEvent::AttackMob {
                    target,
                    amount,
                    source,
                } => {
                    self.combat.hit_landed = self.mob_attacks_mob(target, amount, source);
                }
                MobEvent::DamageMob {
                    target,
                    amount,
                    source,
                } => {
                    self.mob_hits_mob(target, amount, source);
                }
                MobEvent::AttackMobWithEffect {
                    target,
                    amount,
                    source,
                    effect,
                    ticks,
                    amplifier,
                } => {
                    // CaveSpider, Husk and WitherSkeleton.doHurtTarget: the
                    // effect sits inside the branch where the hit landed, and
                    // `LivingEntity.addEffect` takes a mob as well as a player.
                    self.combat.hit_landed = self.mob_attacks_mob(target, amount, source);
                    if self.combat.hit_landed {
                        self.ecs_add_effect(target, effect, ticks, amplifier);
                    }
                }
                MobEvent::CubeHurtPlayer { cube, amount, from } => {
                    // AbstractCubeMob.dealDamage: the attack sound (and its
                    // two pitch draws, made even for a silent cube, whose
                    // Entity.playSound is then skipped) only follows a hit
                    // that landed.
                    if self.hurt_player_from(
                        amount,
                        Some(from),
                        "minecraft:mob_attack",
                        Some(Some(cube)),
                        false,
                    ) {
                        let pitch = self.cube_attack_pitch(cube);
                        if let Some(at) = self.creature_center(cube).filter(|_| !self.ecs.is_silent(cube)) {
                            self.handle_mob_events(vec![MobEvent::Sound("entity.slime.attack", at, 1.0, pitch)])?;
                        }
                    }
                }
                MobEvent::CubeHurtMob {
                    cube,
                    target,
                    amount,
                    source,
                } => {
                    // AbstractCubeMob.dealDamage: the attack sound (and its
                    // two pitch draws, made even for a silent cube, whose
                    // Entity.playSound is then skipped) only follows a hit
                    // that landed.
                    if self.mob_hits_mob(target, amount, source) {
                        let pitch = self.cube_attack_pitch(cube);
                        if let Some(at) = self.creature_center(cube).filter(|_| !self.ecs.is_silent(cube)) {
                            self.handle_mob_events(vec![MobEvent::Sound("entity.slime.attack", at, 1.0, pitch)])?;
                        }
                        self.mob_post_attack(
                            target,
                            source.attacker,
                            None,
                            Some(super::creatures::damage_type(source.kind, false)),
                        );
                    }
                }
                MobEvent::SonicBoom {
                    target,
                    attacker,
                    from,
                    direction,
                } => {
                    // SonicBoom: the push only follows a hit that landed, scaled
                    // by the target's KNOCKBACK_RESISTANCE.
                    let (hurt, resistance) = match target {
                        Who::Player => {
                            let hurt = self.mob_hits_player(10.0, from, "minecraft:sonic_boom");
                            let resistance = self.player.as_mut().map_or(0.0, |p| {
                                p.attribute("minecraft:knockback_resistance").unwrap_or(0.0)
                            });
                            (hurt, resistance)
                        }
                        Who::Mob(id) => {
                            let source = Damage {
                                kind: DamageKind::SonicBoom,
                                attacker: Some(Who::Mob(attacker)),
                                from: Some(from),
                                attacker_species: Some(Species::Warden),
                            };
                            let hurt = self.mob_hits_mob(id, 10.0, source);
                            let resistance = self
                                .ecs
                                .attribute(
                                    id,
                                    "minecraft:knockback_resistance",
                                    horizon_world::attribute_map::ValueKind::Computed,
                                )
                                .unwrap_or(0.0);
                            (hurt, resistance)
                        }
                    };
                    if hurt {
                        let horizontal = 2.5 * (1.0 - resistance);
                        let vertical = 0.5 * (1.0 - resistance);
                        let velocity = [
                            direction[0] * horizontal,
                            direction[1] * vertical,
                            direction[2] * horizontal,
                        ];
                        self.handle_mob_events(vec![MobEvent::Push { target, velocity }])?;
                    }
                }
                MobEvent::Tamed { .. }
                | MobEvent::Buck { .. }
                | MobEvent::StabDismount { .. }
                | MobEvent::EjectPassengers { .. } => self.mount_event(&e),
                MobEvent::IronGolemHit {
                    target,
                    amount,
                    source,
                } => {
                    // IronGolem.doHurtTarget: the toss only follows a hit that
                    // landed, scaled by the target's KNOCKBACK_RESISTANCE.
                    let hurt = match target {
                        Who::Player => {
                            let from = source.from.unwrap_or_default();
                            self.mob_hits_player(amount, from, "minecraft:mob_attack")
                        }
                        Who::Mob(id) => self.mob_hits_mob(id, amount, source),
                    };
                    if hurt {
                        let resistance = match target {
                            Who::Player => self.player.as_mut().map_or(0.0, |p| {
                                p.attribute("minecraft:knockback_resistance").unwrap_or(0.0)
                            }),
                            Who::Mob(id) => self
                                .ecs
                                .attribute(id, "minecraft:knockback_resistance", horizon_world::attribute_map::ValueKind::Computed)
                                .unwrap_or(0.0),
                        };
                        let up = f64::from(0.4f32) * (1.0 - resistance).max(0.0);
                        match target {
                            Who::Player => {
                                if let Some(p) = self.player.as_mut() {
                                    p.velocity.y += up;
                                }
                            }
                            Who::Mob(id) => {
                                if let (Some(e), Some(v)) = (self.ecs.entity_of(id), self.ecs.velocity(id)) {
                                    self.ecs.set_velocity(e, Vec3::new(v.x, v.y + up, v.z));
                                }
                            }
                        }
                        if let Who::Mob(id) = target {
                            self.mob_post_attack(
                                id,
                                source.attacker,
                                None,
                                Some("minecraft:mob_attack"),
                            );
                        }
                    }
                }
                MobEvent::RingBell { bell } => {
                    let pos = BlockPos::new(bell[0], bell[1], bell[2]);
                    let rang = self
                        .with_level(|b, level, ticks| b.bell_attempt_to_ring(level, ticks, pos));
                    if rang {
                        let _ = self.process_level_events();
                    }
                }
                MobEvent::ChestOpen { chest, open, mob } => self.golem_chest_open(chest, open, mob),
                MobEvent::ChestSlot { chest, slot, stack } => {
                    self.golem_chest_slot(chest, slot, stack)
                }
                MobEvent::BecomeStatue { mob } => self.golem_become_statue(mob),
                MobEvent::EnterHive { mob, hive } => self.bee_enter_hive(mob, hive),
                MobEvent::ParrotToShoulder { mob } => self.parrot_to_shoulder(mob),
                MobEvent::Infested { at, velocities } => self.spawn_infested(at, velocities),
                MobEvent::Gift { target, item } => self.receive_gift(target, item),
                // The snow golem's melting is the ECS rule `behaviours/snow_trail.rs`; the host event has nothing left to do.
                MobEvent::HurtSelf { .. } => {}
                MobEvent::SetTarget { mob, target } => {
                    let player = self
                        .lan
                        .as_ref()
                        .and_then(|host| host.mob_players.get(&mob).copied())
                        .or(self.lan_acting);
                    self.set_event_ai_target(mob, Some(target), player);
                }
                MobEvent::PiglinAlertTarget(alert) => self.piglin_alert_target(alert),
                MobEvent::HuntHoglin { mob, target, ticks } => {
                    let mut acquire = false;
                    // RESTE-SANS-ECS: `extra.ints[0]` (the hoglin's attack timer) has no bridge setter.
                    let _ = ticks;
                    acquire = self.ecs.ai_target(mob).is_none();
                    if acquire {
                        let player = self
                            .lan
                            .as_ref()
                            .and_then(|host| host.mob_players.get(&mob).copied())
                            .or(self.lan_acting);
                        self.set_event_ai_target(mob, Some(Who::Mob(target)), player);
                    }
                }
                MobEvent::Projectile(p) => self.launch_mob_projectile(p),
                MobEvent::BowShot {
                    mob,
                    weapon,
                    mut arrow,
                } => {
                    self.mob_bow_ammo(mob, &weapon, &mut arrow);
                    if let Some(state) = arrow.arrow.as_mut() {
                        state.fired_from_weapon = Some(weapon);
                        arrow.bonus.damage = 0.0;
                    }
                    self.launch_mob_projectile(arrow);
                }
                MobEvent::CrossbowShot { ammo, mut arrow } => {
                    if &*ammo.item == "minecraft:firework_rocket" {
                        // `CrossbowItem.createProjectile`: a rocket, from 0.15 under the eyes.
                        let mut at = arrow.position;
                        at.y -= f64::from(0.15f32) - 0.1;
                        self.launch_angled_rocket(at, &ammo, arrow.owner, |_| arrow.velocity);
                    } else {
                        Self::arrow_from_ammo(&ammo, &mut arrow);
                        self.launch_mob_projectile(arrow);
                    }
                }
                MobEvent::CrossbowLoad {
                    mob,
                    weapon,
                    at,
                    pitch,
                } => {
                    if self.mob_crossbow_ammo(mob, &weapon) {
                        self.audio.play(
                            "minecraft:item.crossbow.loading_end",
                            Some(at),
                            1.0,
                            pitch,
                        );
                    }
                }
                // RESTE-SANS-ECS: the built Mob's own state (extra, ai, equipment) is not carried over; the ECS spawns the kind at its position.
                MobEvent::Spawn(m) => {
                    let at = [m.position.x, m.position.y, m.position.z];
                    self.spawn_mob(&m.kind.id, at, false, SpawnSite::Raw);
                }
                MobEvent::SpawnKind { kind, at, setup } => {
                    if let Some(id) = self.spawn_mob(
                        &kind,
                        at,
                        setup.persistent,
                        if setup.skip_finalize {
                            SpawnSite::Raw
                        } else {
                            SpawnSite::Finalizing(
                                horizon_client::mobs::spawn_rules::Reason::MobSummoned,
                            )
                        },
                    ) {
                        self.apply_setup(id, &setup);
                    }
                }
                // The ECS converts through `HostEvent::ConversionAsk`; the host conversion is gone.
                MobEvent::Convert { .. } => {}
                MobEvent::ThrowItem {
                    mob,
                    stack,
                    at,
                    motion,
                    pickup_delay,
                } => {
                    // `thrownItem.setThrower(the mob)`.
                    if let Some(id) = self.spawn_item(stack, at, motion, pickup_delay) {
                        let thrower = self.mob_uuid(mob);
                        self.ecs.set_item_thrower(id, Some(thrower));
                    }
                }
                MobEvent::DropStack { mob, stack, at } => {
                    self.native_entity_drop_item(mob, &stack.item, stack.count as i32, at);
                    self.spawn_item_own(stack, at, 10);
                }
                MobEvent::DropItem {
                    mob,
                    item,
                    count,
                    at,
                } => {
                    self.native_entity_drop_item(mob, &item, count as i32, at);
                    // The ItemEntity constructor's motion, from the entity's
                    // own random (not the level's), so spawnAtLocation drops
                    // (a fox's berries, a panda's held food, a goat's horn)
                    // land apart.
                    self.spawn_item_own(
                        ItemStack {
                            item,
                            count,
                            damage: 0,
                            extra: None,
                        },
                        at,
                        10,
                    );
                }
                MobEvent::DropGoatHorn {
                    mob,
                    screaming,
                    at,
                    motion,
                } => {
                    let instrument =
                        horizon_client::mobs::goat::create_horn(self.mob_uuid(mob), screaming);
                    let mut horn = ItemStack::new("minecraft:goat_horn", 1);
                    horn.data_mut().set_other(
                        "minecraft:instrument",
                        Some(horizon_core::nbt::Tag::String(instrument.into())),
                    );
                    // Goat.dropHorn's ItemEntity constructor leaves pickupDelay at
                    // 0 (no setDefaultPickUpDelay).
                    self.spawn_item(horn, at, motion, 0);
                }
                MobEvent::DropGift { table, at } => {
                    let stacks =
                        self.res
                            .items()
                            .loot_table(table, false, None, &mut self.world.random);
                    for stack in stacks {
                        // The ItemEntity constructor's motion, from the
                        // entity's own random (not the level's).
                        self.spawn_item_own(stack, at, 10);
                    }
                }
                // `Level.setBlockAndUpdate`: neighbour and shape updates, so a crop
                // above trampled farmland breaks and drops.
                MobEvent::SetBlock { pos, state } => self.level_set(pos, state)?,
                MobEvent::EndermanPlaceBlock {
                    mob,
                    pos,
                    carried,
                    occupants,
                } => {
                    self.enderman_place_block(mob, pos, carried, &occupants)?;
                }
                MobEvent::PlaceBlock { mob, pos, state } => {
                    self.mob_place_block(mob, pos, state)?
                }
                MobEvent::CropRaided {
                    pos,
                    state,
                    old,
                    mob,
                } => {
                    self.level_set_with_flags(
                        pos,
                        state,
                        horizon_world::level::flags::UPDATE_CLIENTS,
                    )?;
                    if let Some(old) = old {
                        self.post_block_game_event(
                            "minecraft:block_change",
                            pos,
                            Some(Who::Mob(mob)),
                            None,
                        );
                        self.level_event_at(2001, pos, old.0 as i32);
                    }
                }
                MobEvent::CrackTurtleEgg { pos } => self.crack_turtle_egg(pos)?,
                MobEvent::BedOccupied { pos, occupied } => self.set_bed_occupied(pos, occupied),
                MobEvent::SetDoorOpen { pos, open } => {
                    let pos = BlockPos::new(pos[0], pos[1], pos[2]);
                    self.with_level(|b, level, ticks| b.set_door_open(level, ticks, pos, open));
                    self.process_level_events()?;
                }
                // The ECS enderman takes its block (`take_block`, through BlockChanges).
                MobEvent::TakeBlock { .. } => {}
                MobEvent::IfBroke {
                    mob,
                    effects,
                    otherwise,
                } => {
                    let follow = if broke.contains(&mob) {
                        effects
                    } else {
                        otherwise
                    };
                    actions.nested.push(NormalActionFrame {
                        events: follow.into(),
                        broke: Vec::new(),
                        actor: Some(mob),
                    });
                }
                MobEvent::Jump { mob } => {
                    self.ecs.jump(mob);
                }
                MobEvent::BreakDoor { mob, pos } => {
                    if !self.mob_may_destroy(mob, pos) {
                        continue;
                    }
                    // BreakDoorGoal.tick: removeBlock, then level events 1021
                    // and 2001 with the state that is there now.
                    let kind = self.ecs.kind(mob).unwrap_or_default().to_owned();
                    let air = self.world.air;
                    let at = BlockPos::new(pos[0], pos[1], pos[2]);
                    self.with_level(|b, level, ticks| b.set_block(level, ticks, at, air, 3));
                    self.process_level_events()?;
                    self.level_event_at(1021, pos, 0);
                    let left = self.world.block(pos[0], pos[1], pos[2]);
                    self.level_event_at(2001, pos, left.0 as i32);
                    self.native_entity_break_door(mob, &kind, pos);
                }
                MobEvent::DestroyProgress { mob, pos, progress } => {
                    self.server_destroy_block_progress(mob as i32, pos, progress);
                }
                MobEvent::LevelEvent { id, pos, data } => self.level_event_at(id, pos, data),
                MobEvent::SetBlockName {
                    mob,
                    pos,
                    name,
                    effect,
                    flags,
                } => {
                    if effect {
                        // EatBlockGoal passes the block's default state constant
                        // (Blocks.GRASS_BLOCK.defaultBlockState()), not the live state
                        // (which could be snowy).
                        let old = self.world.block(pos[0], pos[1], pos[2]);
                        let state = self
                            .pack
                            .assets
                            .registry
                            .default_state(&self.res.facts().get(old).name)
                            .unwrap_or(old);
                        self.level_event_at(2001, pos, state.0 as i32);
                    }
                    if let Some(state) = self.pack.assets.registry.default_state(name) {
                        let old_name = self
                            .res
                            .facts()
                            .get(self.world.block(pos[0], pos[1], pos[2]))
                            .name
                            .clone();
                        let mut cancel = false;
                        if let Some(mob) = mob
                            && let Some(kind) = self.ecs.kind(mob).map(str::to_owned)
                        {
                            let from = self.world.block(pos[0], pos[1], pos[2]);
                            self.native_entity_change_block_pre(
                                crate::game::native_hooks::EntityChangeBlockPreCall {
                                    player: None,
                                    entity: Some(mob),
                                    kind: &kind,
                                    pos,
                                    from,
                                    to: state,
                                    cancel: &mut cancel,
                                },
                            );
                        }
                        if !cancel {
                            self.level_set_with_flags(pos, state, flags)?;
                            if let Some(mob) = mob
                                && let Some(kind) = self.ecs.kind(mob).map(str::to_owned)
                            {
                                self.native_entity_change_block(mob, &kind, pos, &old_name, name);
                            }
                        }
                    }
                }
                MobEvent::Infest {
                    pos,
                    infested,
                    poof,
                } => {
                    let registry = &self.pack.assets.registry;
                    let old = self.world.block(pos[0], pos[1], pos[2]);
                    if let Some(mut state) = registry.state(old).cloned() {
                        let (from, to) = if infested {
                            ("minecraft:", "minecraft:infested_")
                        } else {
                            ("minecraft:infested_", "minecraft:")
                        };
                        state.name = state.name.replacen(from, to, 1);
                        if let Ok(id) = registry.resolve(&state) {
                            self.level_set(pos, id)?;
                        }
                    }
                    if let Some(mob) = poof {
                        self.mob_poof(mob);
                    }
                }
                MobEvent::BreakBlock {
                    pos,
                    drop,
                    by,
                    breaker,
                } => {
                    if let Some(mob) = by {
                        if !self.mob_may_destroy(mob, pos) {
                            continue;
                        }
                        broke.push(mob);
                    }
                    let old = self.world.block(pos[0], pos[1], pos[2]);
                    // Block.spawnDestroyParticles: fire breaks leave nothing.
                    if !self.res.facts().get(old).has_class("BaseFireBlock") {
                        self.level_event_at(2001, pos, old.0 as i32);
                    }
                    if drop {
                        let entity = by.or(breaker);
                        let mut drops = self.roll_block_loot(pos, old, entity, Default::default());
                        self.native_block_loot(pos, old, None, None, None, &mut drops);
                        let dropped = self.entity_block_dropped(pos, old, entity, drops);
                        let canceled = dropped.canceled;
                        self.spawn_block_drops(pos, dropped);
                        // InfestedBlock.spawnAfterBreak runs after the drops,
                        // unless BlockDropsEvent canceled it.
                        if !canceled
                            && self
                                .res
                                .facts()
                                .get(old)
                                .name
                                .starts_with("minecraft:infested_")
                            && self
                                .session
                                .rules
                                .bool_at(horizon_client::gamerules::vanilla::BLOCK_DROPS)
                        {
                            let at = [
                                f64::from(pos[0]) + 0.5,
                                f64::from(pos[1]),
                                f64::from(pos[2]) + 0.5,
                            ];
                            if let Some(id) =
                                self.spawn_mob("minecraft:silverfish", at, false, SpawnSite::Plain)
                            {
                                self.mob_poof(id);
                            }
                        }
                    }
                    // Level.destroyBlock: setBlock(fluidState.createLegacyBlock(), 3),
                    // so water stays water and a waterlogged block leaves its water.
                    let b = self.res.behaviours();
                    let left = b.legacy_block(b.fluid(old));
                    if left != old {
                        self.level_set(pos, left)?;
                    }
                }
                MobEvent::Particles { kind, at, count } => {
                    // Mob particle bursts (hearts, smoke, splashes) around a point.
                    if let Some(ty) = horizon_client::particles::ParticleType::from_name(kind) {
                        let options = horizon_client::particles::ParticleOptions::simple(ty);
                        if ty == horizon_client::particles::ParticleType::SonicBoom {
                            self.lan_sonic_boom(&options, at);
                        }
                        for _ in 0..count {
                            // The local burst's draws stay on the level random, in their order.
                            let r = &mut self.world.random;
                            let _ = [r.next_float(), r.next_float(), r.next_float()];
                            let _ = [r.next_float(), r.next_float(), r.next_float()];
                            let _ = [r.next_gaussian(), r.next_gaussian(), r.next_gaussian()];
                        }
                    }
                }
                MobEvent::AroundSelf {
                    mob, event, feet, ..
                } => {
                    self.lan_entity_event(mob as i32, event, feet);
                }
                MobEvent::ParticleAt { .. } | MobEvent::ParticleColored { .. } => {}
                MobEvent::Experience { at, amount } => self.add_orb(at, amount),
                MobEvent::AxolotlSupportPlayer { mob } => {
                    let previous = self
                        .potions
                        .effects
                        .get("regeneration")
                        .map_or(0, |e| e.duration);
                    if (0..=2399).contains(&previous) {
                        self.potions
                            .effects
                            .add(horizon_client::effects::EffectInstance::new(
                                "regeneration",
                                (previous + 100).min(2400),
                                0,
                            ));
                    }
                    self.potions
                        .effects
                        .list
                        .retain(|e| e.id != "mining_fatigue");
                    let source = self.ecs_host_view(mob);
                    self.trigger_effects_changed_by(source);
                }
                // The ECS asks for the treasure (`MiscEvent::TreasureAsk`) and sets it on the dolphin.
                MobEvent::DolphinTreasure { .. } => {}
                MobEvent::EffectPlayer {
                    effect,
                    ticks,
                    amplifier,
                    ambient,
                } => {
                    self.give_mob_effect(effect, ticks, amplifier, ambient);
                }
                MobEvent::EffectMob {
                    target,
                    effect,
                    ticks,
                    amplifier,
                } => {
                    self.ecs_add_effect(target, effect, ticks, amplifier);
                }
                MobEvent::Ignite(target, seconds) => self.ignite_entity(target, seconds)?,
                MobEvent::ZombieIgnite(target, seconds) if self.combat.hit_landed => {
                    self.ignite_entity(target, seconds)?
                }
                MobEvent::ZombieIgnite(..) => {}
                // The ECS gaze and retreat rules (`freeze_when_looked_at`, `retreat_on_hurt`) act on the mob.
                MobEvent::LockGaze { .. } | MobEvent::Retreat { .. } => {}
                // Dead producers only (old brains, frog, piglin): nothing to apply.
                MobEvent::RemoveKilled { .. }
                | MobEvent::HoglinAttackTarget { .. }
                | MobEvent::Breed { .. }
                | MobEvent::BreedTarget { .. }
                | MobEvent::FinishAdmiring { .. }
                | MobEvent::Bonemeal { .. }
                | MobEvent::VillagerMeet { .. }
                | MobEvent::VillagerBirth { .. }
                | MobEvent::WorkAtComposter { .. }
                | MobEvent::GolemCheck { .. }
                | MobEvent::PandaSneezed { .. }
                | MobEvent::VillagerTrade { .. } => {}
                MobEvent::StartRiding { rider, vehicle } => {
                    self.start_riding_mob(rider, vehicle);
                }
                // Applied after the fish's tick (`apply_brain_events`).
                MobEvent::StartFollowing { .. }
                | MobEvent::SchoolSize { .. }
                | MobEvent::PufferfishSting { .. } => {}
                e @ MobEvent::LeashBreak { at: Some(_), .. } => self.leash_break_event(e),
                MobEvent::LeashBreak { mob, drop, sound, .. } => {
                    self.tick_break_leash(mob, drop, sound)
                }
                // The ECS leash rule (`defend_leash_holder`) reads the holder.
                MobEvent::NotifyLeashHolder { .. } => {}
                MobEvent::Vibration {
                    from,
                    to,
                    ticks,
                    source,
                } => self.mob_vibration_particle(source, from, to, ticks),
                MobEvent::DigParticles { .. } => {}
                MobEvent::BlockBreakEffect { pos, block } => {
                    // Turtle.aiStep broadcasts LevelEvent 2001, not the
                    // particles themselves: the event carries the particles and
                    // the sand's break sound, and reaches the clients the same
                    // way a broken block's does.
                    let state = self.world.block(block[0], block[1], block[2]);
                    self.level_event_at(2001, pos, state.0 as i32);
                }
                MobEvent::Darkness { at, radius } => self.apply_darkness(at, radius),
                MobEvent::CreakingHurt {
                    home,
                    by_player,
                    id,
                } => self.creaking_hurt(home, id, by_player),
                MobEvent::CreakingTearDown { center, spread } => {
                    self.creaking_tear_down(center, spread)
                }
                MobEvent::CureZombieVillager { mob } => self.cure_zombie_villager(mob),
                MobEvent::Reinforcements { mob, target } => self.call_reinforcements(mob, target),
                MobEvent::VillagerOffers { mob } => self.merchant_offers_generated(mob),
                MobEvent::VillagerRestocked { mob } => self.merchant_restocked(mob),
                MobEvent::Firework { at, color, flight } => {
                    self.villager_firework(at, color, flight)
                }
                MobEvent::ThrowGift { table, from, to } => self.throw_gift(&table, from, to),
                // The ECS patrol rule (`long_distance_patrol`) picks the target.
                MobEvent::PatrolTarget { .. } => {}
                MobEvent::RaiderTarget {
                    mob,
                    target,
                    aggressive,
                } => {
                    let player = self
                        .lan
                        .as_ref()
                        .and_then(|host| host.mob_players.get(&mob).copied())
                        .or(self.lan_acting);
                    self.set_event_ai_target(mob, Some(target), player);
                    if aggressive {
                        self.ecs.set_aggressive(mob, true);
                    }
                }
                MobEvent::FallToPassengers {
                    mob,
                    distance,
                    modifier,
                } => self.fall_to_passengers(mob, distance, modifier),
                MobEvent::ShedScute { mob, at, pitch } => {
                    // The mob already checked `shouldDropLoot` (a baby, `mob_drops`).
                    let table = "minecraft:gameplay/armadillo_shed";
                    let drops = self.sequence_loot(table, table, None);
                    for d in drops {
                        self.spawn_at_location(Vec3::new(at[0], at[1], at[2]), 0.0, d);
                    }
                    self.audio.play(
                        "minecraft:entity.armadillo.scute_drop",
                        Some(at),
                        1.0,
                        pitch,
                    );
                    let ctx = self.who_context(Some(Who::Mob(mob)), None);
                    self.post_game_event("minecraft:entity_place", at, ctx);
                }
                MobEvent::LayEgg { mob, at, pitch } => {
                    // Chicken.aiStep: `dropFromGiftLootTable` says whether
                    // anything dropped; only then the sound and the game event.
                    if self.drop_gift_loot(mob, "minecraft:gameplay/chicken_lay", at) {
                        self.audio
                            .play("minecraft:entity.chicken.egg", Some(at), 1.0, pitch);
                        let ctx = self.who_context(Some(Who::Mob(mob)), None);
                        self.post_game_event("minecraft:entity_place", at, ctx);
                    }
                }
                MobEvent::GiftLoot { mob, table, at } => {
                    // Turtle.ageBoundaryReached and Panda.afterSneeze: only
                    // with mob loot on.
                    if matches!(
                        table,
                        "minecraft:gameplay/turtle_grow" | "minecraft:gameplay/panda_sneeze"
                    ) && !self
                        .session
                        .rules
                        .bool_at(horizon_client::gamerules::vanilla::MOB_DROPS)
                    {
                        continue;
                    }
                    self.drop_gift_loot(mob, table, at);
                }
            }
        }
    }
    /// `TurtleEggBlock.decreaseEggs` after an entity's step or landing won its
    /// roll (`destroyEgg`): the break sound and the remaining eggs (a stack of
    /// one is destroyed after it).
    pub(super) fn crack_turtle_egg(&mut self, pos: [i32; 3]) -> Result<()> {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let eggs = self
            .pack
            .assets
            .registry
            .state(state)
            .filter(|s| s.name == "minecraft:turtle_egg")
            .and_then(|s| s.properties.get("eggs"))
            .and_then(|e| e.parse::<i32>().ok());
        if let Some(eggs) = eggs {
            let at = BlockPos::new(pos[0], pos[1], pos[2]);
            self.with_level(|b, level, ticks| {
                b.turtle_egg_destroyed_by_player(level, ticks, at, state);
                if eggs <= 1 {
                    b.destroy_block(level, ticks, at, false);
                }
            });
            self.process_level_events()?;
        }
        Ok(())
    }
    /// `TurtleEggBlock.randomTick` at the last hatch stage: one baby turtle
    /// per egg, a day from grown, with the egg as its home (the sound, the
    /// crumbs and the block's removal came from the level).
    pub(super) fn hatch_turtles(
        &mut self,
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
    ) {
        let eggs = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("eggs"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(1);
        for i in 0..eggs {
            let at = [
                f64::from(pos[0]) + 0.3 + f64::from(i) * 0.2,
                f64::from(pos[1]),
                f64::from(pos[2]) + 0.3,
            ];
            if let Some(id) = self.spawn_mob_raw("minecraft:turtle", at, true) {
                // RESTE-SANS-ECS: `extra.home` (the nest) has no bridge setter.
                self.ecs.set_baby(id, true);
                if let Some(e) = self.ecs.entity_of(id) {
                    let pitch = self.ecs.pitch(id).unwrap_or(0.0);
                    self.ecs.set_rotation(e, 0.0, pitch);
                }
                self.ecs.set_head_yaw(id, 0.0);
            }
        }
    }
    fn apply_setup(&mut self, id: u32, setup: &SpawnSetup) {
        let Some(e) = self.ecs.entity_of(id) else {
            return;
        };
        if setup.baby {
            self.ecs.set_baby(id, true);
        }
        if setup.nausea {
            self.ecs_add_effect(id, "minecraft:nausea", 200, 0);
        }
        if let Some(owner) = setup.owner_mob {
            self.ecs.set_mob_owner(id, owner);
        }
        if let Some(home) = setup.home {
            self.ecs.set_bound_origin(id, home);
        }
        if let Some(life) = setup.life {
            self.ecs.set_limited_life(id, u64::try_from(life).unwrap_or(0));
        }
        // RESTE-SANS-ECS: size and variant have no bridge setter (the vex has neither).
        let pitch = self.ecs.pitch(id).unwrap_or(0.0);
        self.ecs.set_rotation(e, setup.yaw, pitch);
        self.ecs.set_head_yaw(id, setup.yaw);
        self.ecs.set_velocity(e, Vec3::new(setup.velocity[0], setup.velocity[1], setup.velocity[2]));
        if let Some(h) = setup.health {
            self.ecs.set_health(id, h);
        }
    }
    /// `ServerPlayer.tellNeutralMobsThatIDied`: the mobs within 32 blocks
    /// across and 10 up and down get `NeutralMob.playerDied`, when the
    /// `forgive_dead_players` rule is on. A mob's `Who::Player` is the guest
    /// it is bound to in `bindings` (game/network/lan_mobs.rs), else the host's
    /// player, so only the mobs whose binding is `conn` (`None` for the
    /// host's player) hear it.
    pub(in crate::game) fn tell_neutral_mobs_player_died(
        &mut self,
        feet: [f64; 3],
        conn: Option<u32>,
    ) {
        if !self.session.rules.bool_at(horizon_client::gamerules::vanilla::FORGIVE_DEAD_PLAYERS) {
            return;
        }
        let Some(player) = self.player_entity_id_by_connection(conn) else { return };
        let block = feet.map(|c| c.floor() as i32);
        self.ecs.forgive_dead_players_near(block, player as u32);
    }
    /// `MobEffectUtil.addEffectToPlayersAround` for an elder guardian:
    /// mining fatigue III for five minutes unless a stronger or longer one
    /// is on, with the curse sound and the elder's apparition.
    fn elder_curse(&mut self, at: [f64; 3], silent: bool) {
        let mut cursed = Vec::new();
        self.for_each_player(|g| {
            if g.elder_curse_player(at) {
                cursed.push(g.lan_acting);
            }
        });
        // A guest's client shows the apparition and plays the curse itself
        // (ClientboundGameEventPacket.GUARDIAN_ELDER_EFFECT), the sound only
        // for 1: `isSilent() ? 0.0F : 1.0F`.
        if let Some(host) = self.lan.as_ref() {
            for conn in cursed.into_iter().flatten() {
                host.hub.send_to(
                    conn,
                    &horizon_protocol::packets::play::Clientbound::GameEvent(
                        horizon_protocol::packets::play::GameEvent {
                            event: horizon_client::lan::hub::game_event::GUARDIAN_ELDER_EFFECT,
                            param: crate::mobs::guardian::elder_curse_param(silent),
                        },
                    ),
                );
            }
        }
    }
    /// The curse on the player in place; whether it took.
    fn elder_curse_player(&mut self, at: [f64; 3]) -> bool {
        if self.creative || self.spectator() {
            return false;
        }
        let Some(p) = self.player.as_ref() else {
            return false;
        };
        let d = p.position.subtract(Vec3::new(at[0], at[1], at[2]));
        if d.x * d.x + d.y * d.y + d.z * d.z >= 2500.0 || p.health.dead_or_dying() {
            return false;
        }
        // The player's effects are kept by their bare name. A curse as strong
        // that won't end within the next minute stays (endsWithin(1199)).
        let active = self.potions.effects.get("mining_fatigue");
        if active.is_some_and(|e| e.amplifier >= 2 && !e.ends_within(1199)) {
            return false;
        }
        self.give_player_effect("minecraft:mining_fatigue", 6000, 2);
        true
    }
    /// `hurtServer` on the player. `from` is the source's position (none: no
    /// direction), `known`: the causing mob when it is given (`/damage`),
    /// else the melee attacker is the mob nearest to `from`.
    pub(super) fn hurt_player_from(
        &mut self,
        amount: f32,
        from: Option<[f64; 3]>,
        damage_type: &str,
        known: Option<Option<u32>>,
        by_creative_player: bool,
    ) -> bool {
        let post_attack_if_alive = std::mem::take(&mut self.combat.post_attack_if_alive);
        let rocket = std::mem::take(&mut self.combat.rocket_hit);
        let pierce = std::mem::take(&mut self.combat.piercing_hit);
        let melee = self.combat.melee_mob.take();
        // A projectile entity dealt it: `#is_projectile`, or a firework
        // rocket (its `fireworks` type is not in the tag).
        let projectile = rocket
            || world_stations!(self)
                .enchantments
                .damage_tags
                .get("minecraft:is_projectile")
                .is_some_and(|t| t.contains(damage_type));
        let blocking = self.player_blocking(from, damage_type, pierce);
        let raised = blocking.is_some();
        // Melee: the attacker stands at `from` (another player's hit has no mob behind it).
        let attacker = match known {
            Some(known) => known,
            None if melee.is_some() => melee,
            None => from
                .filter(|_| !projectile && damage_type != "minecraft:player_attack")
                .and_then(|from| {
                    // The nearest living creature within 16 blocks (the ECS keeps no list of all mobs).
                    let d = |id: u32| {
                        self.ecs.position(id).map_or(f64::MAX, |p| {
                            (p[0] - from[0]).powi(2) + (p[2] - from[2]).powi(2)
                        })
                    };
                    self.ecs
                        .ids_near(from, 16.0)
                        .into_iter()
                        .filter(|&id| self.ecs.is_mob(id) && self.ecs.alive(id))
                        .min_by(|&a, &b| d(a).total_cmp(&d(b)))
                }),
        };
        // The causing entity for the death message.
        let owner = self.combat.projectile_owner.take();
        // The mob that caused the hit, else the attack the caller landed it in (a projectile's
        // owner, another player's blow): `combat.attack`, read through `current_attack`.
        let mut attack = match attacker.or(owner) {
            Some(id) => self.attack_of(id as i32),
            None => self.current_attack(),
        };
        attack.projectile = self.combat.attack().projectile;
        attack.direct = self.combat.attack().direct;
        attack.position = from;
        attack.projectile_weapon = self.combat.attack().projectile_weapon.clone();
        if let Some(a) = attacker.or(owner) {
            self.note_attacker(damage_type, a);
        }
        if let Some((id, name)) = self.combat.projectile_attacker.take() {
            self.note_named_attacker(damage_type, id, name);
        }
        // The projectile is the direct entity: named when it has no owner.
        if let Some(name) = self.combat.projectile_direct.take() {
            self.note_direct_entity(damage_type, name);
        }
        // DamageSource.scalesWithDifficulty: a mob (or its projectile's
        // owner) caused the hit, not a player.
        let living_cause = attacker.or(owner).is_some();
        // The native damage hook's causing entity: the one given, or the
        // owner of the projectile that hit (`projectile_owner` outlives a
        // projectile's other hits), not the guessed melee attacker.
        let owner = owner.filter(|_| projectile);
        let causing = known.map_or_else(
            || attacker.or(owner).map_or(attack.attacker, |id| id as i32),
            |known| known.map_or(0, |id| id as i32),
        );
        let mut outcome = horizon_client::health::BlockOutcome::default();
        // The hit's own push, held back until after the shield's (`hurtServer`
        // runs `applyItemBlocking` before `dealDefaultKnockback`).
        let mut default_push: Option<([f64; 2], HurtSource<'_>)> = None;
        let events_before = self.player.as_ref().map_or(0, |p| p.events.len());
        let damage_outcome = if let Some(id) = self.health_resources.damage.id(damage_type) {
            let weapon = self.player_armor_weapon(attacker, projectile);
            self.hurt_player_equipment(
                id,
                amount,
                blocking,
                living_cause,
                causing,
                by_creative_player,
                &attack,
                weapon,
            )
        } else {
            Ok(Default::default())
        };
        if let Ok(o) = damage_outcome
            && let Some(player) = self.player.as_mut()
            && {
                outcome = o;
                o.hurt
            }
        {
            // LivingEntity.hurtServer's dealDefaultKnockback: not inside the
            // hurt cooldown (a stronger hit there only adds its difference).
            if o.knockback {
                // No source position: a random direction.
                let (xd, zd) = from.map_or((0.0, 0.0), |from| {
                    (from[0] - player.position.x, from[2] - player.position.z)
                });
                // dealDefaultKnockback's `indicateDamage`, unless blocked.
                if o.blocked <= 0.0 {
                    player.health.indicate_damage(xd, zd, player.yaw);
                }
                let source = HurtSource {
                    damage_type,
                    causing,
                    direct: attack.projectile_direct(damage_type),
                    position: None,
                };
                default_push = Some(([xd, zd], source));
            }
            // The thorns hurt the entity that caused the hit, if there is one.
            // Thorns' own hit is no attack, so nothing answers it
            // (`doPostAttackEffects` runs from the attacks, not `hurtServer`).
            let killed = self.player.as_ref().is_some_and(|p| {
                super::wither::killed(true, p.health.dead_or_dying(), p.health.totem_used)
            });
            if let Some(a) = attacker.or(owner)
                && !(post_attack_if_alive && killed)
                && damage_type != "minecraft:thorns"
            {
                self.thorns(a);
            }
        }
        // LivingEntity.playSecondaryHurtSound: a thorns hit that landed whole
        // sounds where the player stands, for everyone around.
        if let Some(p) = self.player.as_ref()
            && p.events.get(events_before..).is_some_and(|new| {
                new.iter().any(|e| {
                    matches!(
                        e,
                        horizon_client::motion::MotionEvent::Health(HealthEvent::Phase(
                            "thorns-sound"
                        ))
                    )
                })
            })
        {
            let at = [p.position.x, p.position.y, p.position.z];
            self.audio
                .play("minecraft:enchant.thorns.hit", Some(at), 1.0, 1.0);
        }
        // LivingEntity.hurtServer: ENTITY_HURT_PLAYER, unless it returned
        // early (invulnerable, or inside the hurt cooldown).
        // A hit a native mod canceled records no damage (hurtServer never ran).
        self.combat.forget_noted(outcome.canceled);
        if outcome.hurt || outcome.sound {
            let source = attacker
                .or(owner)
                .and_then(|id| self.ecs_host_view(id));
            self.trigger_entity_hurt_player(&horizon_client::advancements::DamageView {
                kind: damage_type.to_owned(),
                // Arrows and the like name no direct entity here; a
                // rocket's burst names the rocket.
                direct: if rocket {
                    Some(horizon_client::advancements::EntityView {
                        kind: "minecraft:firework_rocket".to_owned(),
                        position: from.unwrap_or_default(),
                        ..Default::default()
                    })
                } else {
                    source.clone().filter(|_| !projectile)
                },
                is_direct: !projectile,
                source,
                original: amount,
                actual: outcome.dealt,
                blocked: outcome.blocked > 0.0,
            });
        }
        if raised {
            let source = HurtSource {
                damage_type,
                causing,
                direct: attack.projectile_direct(damage_type),
                position: None,
            };
            self.after_block(outcome, (attacker, projectile), source, &attack);
        }
        // `LivingEntity.dealDefaultKnockback`: after the item's block push, so
        // a half-blocked hit pushes the defender back the way vanilla does.
        if let Some(([xd, zd], source)) = default_push {
            self.knock_back_player(f64::from(0.4_f32), xd, zd, source, &attack, amount);
            if let Some(p) = self.player.as_mut()
                && let Some(r) = p.hurt_log.last_mut()
            {
                r.knockback = Some([p.velocity.x, p.velocity.y, p.velocity.z]);
            }
        }
        outcome.hurt
    }
    /// A status effect from a mob, arrow or splash potion (`addEffect`).
    pub(in crate::game) fn give_player_effect(
        &mut self,
        effect: &'static str,
        ticks: i32,
        amplifier: i32,
    ) {
        self.give_mob_effect(effect, ticks, amplifier, false);
    }
    /// A mob's effect on the player. Creative and spectator players shrug off
    /// the damaging effects here, but the nautilus's ambient Breath is plain
    /// `addEffect` and lands in every game mode.
    fn give_mob_effect(&mut self, effect: &'static str, ticks: i32, amplifier: i32, ambient: bool) {
        let immune = self.creative || self.spectator();
        if let Some(instance) = mob_effect_instance(effect, ticks, amplifier, ambient, immune) {
            self.potions.effects.add(instance);
        }
    }
    /// `WitherRoseBlock.entityInside` for the player: on a server level that
    /// isn't peaceful, `getBeeInteractionEffect` (Wither, 40 ticks) unless the
    /// player is invulnerable to wither damage (creative, spectator). A
    /// server's player gets it from the server.
    pub(super) fn wither_rose_contact(&mut self) {
        if self.session.difficulty == 0 {
            return;
        }
        self.give_player_effect("minecraft:wither", 40, 0);
    }
    pub(in crate::game) fn hurt_player_by(&mut self, damage_type: &str, amount: f32) {
        let mut canceled = false;
        let attack = self.current_attack();
        if self.player.is_some()
            && let Some(id) = self.health_resources.damage.id(damage_type)
            && let Ok(o) =
                self.hurt_player_equipment(id, amount, None, false, 0, false, &attack, None)
        {
            canceled = o.canceled;
        }
        self.combat.forget_noted(canceled);
    }
    /// The level as arrows see it.
    pub(super) fn arrow_scene<'s>(
        &'s self,
        rain: &'s dyn Fn(BlockPos) -> bool,
    ) -> horizon_client::arrows::ArrowScene<'s, 's> {
        horizon_client::arrows::ArrowScene {
            world: &self.world,
            registry: self.pack.assets.registry,
            facts: self.res.facts(),
            collision: self.collision,
            fluids: &self.motion.fluids,
            loop_context: &self.loop_context,
            raining: rain,
            mob_griefing: self
                .session
                .rules
                .bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING),
            tnt_explodes: self
                .session
                .rules
                .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES),
            mob_hooks: self
                .grief_hooked()
                .then_some(self as &dyn horizon_client::mobs::MobHooks),
            owners: self,
        }
    }
    /// The mob a normal interaction can reach: a packet's exact validated ID,
    /// or the crosshair's mob before its blocking shape.
    pub(in crate::game) fn mob_interaction_target(
        &self,
        target: Option<InteractTarget>,
    ) -> Option<usize> {
        // RESTE-SANS-ECS: its only caller (game.rs, the non-ECS branch of its item use) still asks for a list index.
        let _ = target;
        None
    }
    /// Right click on a mob (Mob.interact / mobInteract), on the mob under the
    /// crosshair or on the one a guest's interact packet named: returns true
    /// when consumed.
    pub(super) fn interact_mob(&mut self, target: Option<InteractTarget>) -> Result<bool> {
        // The server's creatures live in the ECS: leads, mounts and P's modules and rules answer there (`ecs_interact`).
        Ok(self.interact_mob_ecs(target))
    }
    /// `ItemStack.hurtAndBreak(1, ...)` on the held tool: Unbreaking, break sound and stat.
    pub(super) fn damage_held_tool(&mut self) {
        self.damage_held(1);
    }
    /// Throwables and spawn eggs (SnowballItem/EggItem/EnderpearlItem/SpawnEggItem.use).
    pub(super) fn throw_item(&mut self, stack: &ItemStack) -> Result<bool> {
        let item = &*stack.item;
        let kind = match item {
            "minecraft:snowball" => ProjectileKind::Snowball,
            "minecraft:egg" | "minecraft:blue_egg" | "minecraft:brown_egg" => ProjectileKind::Egg,
            "minecraft:ender_pearl" => ProjectileKind::EnderPearl,
            "minecraft:wind_charge" => {
                let thrown = self.throw_wind_charge(stack)?;
                if thrown {
                    self.apply_use_cooldown(stack);
                }
                return Ok(thrown);
            }
            _ if item.ends_with("_spawn_egg") => return self.use_spawn_egg(stack),
            _ => match horizon_client::projectiles::thrown_by_item(item) {
                Some(t) => ProjectileKind::Thrown(t),
                None => return Ok(false),
            },
        };
        let Some(eye) = self.eye_position() else {
            return Ok(false);
        };
        let Some(p) = self.player.as_ref() else {
            return Ok(false);
        };
        // Projectile.shootFromRotation adds getKnownMovement: what the thrower last moved.
        let on_ground = p.ground.on_ground;
        let velocity = self.known_player_movement();
        let start = Vec3::new(eye[0], eye[1] - 0.1, eye[2]);
        let mut proj = Projectile::new(kind.clone(), start, Some(Who::Player));
        proj.set_egg_item(stack);
        proj.shoot_from_rotation(self.pitch, self.yaw, 0.0, 1.5, 1.0, velocity, on_ground);
        self.spawn_projectile(proj, stack);
        // SnowballItem.use / EggItem.use / EnderpearlItem.use: awardStat(ITEM_USED).
        self.award_stat("used", item, 1);
        // ItemStack.use's applyAfterUseComponentSideEffects: the pearl's
        // second is the `use_cooldown` component, as for any thrown item.
        self.apply_use_cooldown(stack);
        let sound = match kind {
            ProjectileKind::Snowball | ProjectileKind::Thrown(_) => {
                "minecraft:entity.snowball.throw"
            }
            ProjectileKind::Egg => "minecraft:entity.egg.throw",
            _ => "minecraft:entity.ender_pearl.throw",
        };
        let pitch = 0.4 / (self.world.random.next_float() * 0.4 + 0.8);
        self.audio.play(sound, Some(eye), 0.5, pitch);
        self.swing_arm();
        if !self.creative
            && let Some(s) = self.held_mut().as_mut()
        {
            s.count -= 1;
            if s.count == 0 {
                *self.held_mut() = None;
            }
        }
        if kind == ProjectileKind::Egg
            && let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone())
        {
            self.native_player_egg_throw(player, &name);
        }
        Ok(true)
    }
    /// `SpawnEggItem.use`: the SOURCE_ONLY ray must hit a liquid block.
    pub(super) fn spawn_egg_fluid_hit(&self) -> Option<super::Hit> {
        let hit = self.bucket_hit()?;
        matches!(
            &*self.block_name(hit.pos),
            "minecraft:water" | "minecraft:lava"
        )
        .then_some(hit)
    }
    /// Shared peaceful-mode eligibility for spawn eggs.
    pub(super) fn spawn_egg_allowed(&self, item: &str) -> bool {
        let kind = item.trim_end_matches("_spawn_egg");
        self.session.difficulty != 0
            || self
                .res
                .mob_types()
                .get(kind)
                .is_none_or(|t| horizon_client::mobs::spawn_rules::allowed_in_peaceful(t.species))
    }
    /// `SpawnEggItem.useOn` for a block, or `use` for a fluid source.
    fn use_spawn_egg(&mut self, stack: &ItemStack) -> Result<bool> {
        let fluid = matches!(self.guest_use, Some(super::GuestUse::Item(_)));
        let Some(hit) = (if fluid {
            self.spawn_egg_fluid_hit()
        } else {
            self.pick()
        }) else {
            return Ok(false);
        };
        let kind = stack.item.trim_end_matches("_spawn_egg").to_owned();
        // ItemStack.useOn in adventure mode: an egg used on a block without a
        // matching `can_place_on` passes to `use`, whose fluid ray finds none.
        if !fluid && !self.may_use_on(hit.pos) {
            return Ok(false);
        }
        if !self.spawn_egg_allowed(&stack.item) {
            self.use_failed = true;
            return Ok(true);
        }
        if fluid
            && !self.may_build()
            && !self.can_place_on_in_adventure([
                hit.pos[0] - hit.face[0],
                hit.pos[1] - hit.face[1],
                hit.pos[2] - hit.face[2],
            ])
        {
            self.use_failed = true;
            return Ok(true);
        }
        // SpawnEggItem.useOn on a spawner sets its mob (and an op's egg data).
        let (spawner_id, data) = horizon_client::block_entity_item::spawn_egg_on_spawner(
            stack,
            self.acting_player_is_op(),
        );
        if let Some(done) = self.use_spawn_egg_on_spawner(hit.pos, &spawner_id, data.as_ref()) {
            if done {
                self.swing_arm();
                self.award_stat("used", &stack.item, 1);
                self.consume_held();
            }
            return Ok(true);
        }
        // useOn: a clicked block without a collision shape (vines, grass)
        // takes the mob itself, any other the one on the clicked face.
        let p = if fluid || self.collision_empty(hit.pos) {
            hit.pos
        } else {
            [
                hit.pos[0] + hit.face[0],
                hit.pos[1] + hit.face[1],
                hit.pos[2] + hit.face[2],
            ]
        };
        let at = [
            f64::from(p[0]) + 0.5,
            f64::from(p[1]),
            f64::from(p[2]) + 0.5,
        ];
        let Some(id) =
            self.spawn_mob_spawned(&kind, at, SpawnSite::Finalizing(SpawnReason::SpawnItemUse))
        else {
            return Ok(false);
        };
        // SpawnEggItem.spawnMob: `EntityType.create` finalizes the mob, then
        // `PostSpawnProcessor` puts the egg's components on it.
        self.apply_spawn_egg_entity_data(id, stack, true);
        self.post_block_game_event("minecraft:entity_place", p, Some(Who::Player), None);
        self.award_stat("used", &stack.item, 1);
        self.swing_arm();
        self.consume_held();
        Ok(true)
    }
    /// `MobBucketItem.spawn`: the bucket's components, then
    /// `EntityType.createDefaultStackConfig`.
    pub(in crate::game) fn apply_bucket_stack_config(&mut self, id: u32, stack: &ItemStack) {
        self.apply_spawn_egg_entity_data(id, stack, true);
    }
    /// `EntityType.createDefaultStackConfig` on the mob an egg or a bucket
    /// spawned: the item's custom name (the whole text, style included) and
    /// custom data (`Entity.applyImplicitComponent`, saved as `CustomName` and
    /// `data`), then, with `entity_data`, `EntityType.updateCustomEntityTag`:
    /// its `minecraft:entity_data`, once the guards let it through, merged
    /// over the mob's own compound as `TypedEntityData.loadInto` does and
    /// loaded back into the same mob (`/data merge entity`'s loader). An egg's
    /// baby (`spawnOffspringFromSpawnEgg`) takes the components only.
    pub(in crate::game) fn apply_spawn_egg_entity_data(
        &mut self,
        id: u32,
        stack: &ItemStack,
        entity_data: bool,
    ) {
        use horizon_core::nbt::Tag;
        let Some(kind) = self.ecs.kind(id) else {
            return;
        };
        let op = self.acting_player_is_op();
        let name = stack.data().and_then(|d| d.custom_name.clone());
        let custom = stack
            .data()
            .and_then(|d| d.other("minecraft:custom_data"))
            .cloned();
        let data = horizon_client::block_entity_item::spawn_egg_entity_data(stack, &kind, op)
            .filter(|_| entity_data);
        if name.is_none() && custom.is_none() && data.is_none() {
            return;
        }
        let Some(own) = self.ecs_mob_tree(id) else {
            return;
        };
        let mut own = Tag::Compound(own);
        if let Tag::Compound(map) = &mut own {
            if let Some(name) = name {
                map.insert("CustomName".into(), name);
            }
            if let Some(custom) = custom {
                map.insert("data".into(), custom);
            }
        }
        let tag = match data {
            Some(data) => horizon_client::data_cmd::merged(&own, data),
            None => own,
        };
        // Nothing to report: the mob's own compound always loads back.
        let _ = self.apply_entity_nbt(super::commands::Target::Mob(id), tag);
    }
}
/// `Bucketable.getBucketItemStack` and `getPickupSound` of a mob that a water
/// bucket picks up.
fn bucketable(species: Species) -> Option<(&'static str, &'static str)> {
    let fish = "minecraft:item.bucket.fill_fish";
    Some(match species {
        Species::Cod => ("minecraft:cod_bucket", fish),
        Species::Salmon => ("minecraft:salmon_bucket", fish),
        Species::Pufferfish => ("minecraft:pufferfish_bucket", fish),
        Species::TropicalFish => ("minecraft:tropical_fish_bucket", fish),
        Species::Tadpole => (
            "minecraft:tadpole_bucket",
            "minecraft:item.bucket.fill_tadpole",
        ),
        Species::Axolotl => (
            "minecraft:axolotl_bucket",
            "minecraft:item.bucket.fill_axolotl",
        ),
        _ => return None,
    })
}
pub(super) use horizon_client::health::{damage_type, melee, source_damage_type};
/// A Horizon item stack as a loot stack (id, count, damage, enchantments,
/// and its custom data as SNBT for `minecraft:custom_data` predicates).
fn to_loot(stack: &ItemStack) -> horizon_world::loot::LootStack {
    let components = stack
        .custom_data()
        .map(|data| {
            (
                "minecraft:custom_data".to_owned(),
                Some(horizon_core::nbt::Tag::Compound(data.clone())),
            )
        })
        .into_iter()
        .collect();
    horizon_world::loot::LootStack {
        item: stack.item.to_string(),
        count: stack.count as i32,
        damage: stack.damage as i32,
        enchantments: stack
            .all_enchantments()
            .iter()
            .map(|(e, l)| (e.to_string(), *l as i32))
            .collect(),
        potion: None,
        map: false,
        exploration: None,
        ominous_amplifier: None,
        stew_effects: Vec::new(),
        names: Vec::new(),
        components,
    }
}
/// A generated loot stack as a Horizon item stack.
pub(super) fn from_loot(stack: &horizon_world::loot::LootStack) -> ItemStack {
    let mut item = ItemStack {
        item: Arc::from(stack.item.as_str()),
        count: stack.count.max(1) as u32,
        damage: stack.damage.max(0) as u32,
        extra: None,
    };
    for (id, level) in &stack.enchantments {
        item.enchant(id, (*level).max(0) as u32);
    }
    if let Some(potion) = &stack.potion {
        item = item.with_potion(potion);
    }
    if let Some(amplifier) = stack.ominous_amplifier.filter(|a| *a > 0) {
        item.data_mut().ominous_amplifier = Some(amplifier as u8);
    }
    for (id, text) in &stack.names {
        item.data_mut().set_loot_name(id, text);
    }
    for (id, value) in &stack.components {
        item.data_mut().set_loot_component(id, value);
    }
    item
}
/// The instance a mob effect gives the player; `None` for a creative or
/// spectator player (`immune`) unless it is ambient (the nautilus's Breath).
fn mob_effect_instance(
    effect: &'static str,
    ticks: i32,
    amplifier: i32,
    ambient: bool,
    immune: bool,
) -> Option<horizon_client::effects::EffectInstance> {
    if immune && !ambient {
        return None;
    }
    let id = effect.strip_prefix("minecraft:").unwrap_or(effect);
    let mut instance = horizon_client::effects::EffectInstance::new(id, ticks, amplifier);
    instance.ambient = ambient;
    Some(instance)
}

include!("mob_query_hooks.rs");

impl Game<'_> {
    /// `EventHooks.canEntityGrief(level, entity)`: the `mob_griefing` rule,
    /// which a mod may change for `entity` (`EntityMobGriefingEvent`). No
    /// entity: the rule alone, as NeoForge.
    pub(super) fn can_entity_grief(&self, entity: Option<u32>) -> bool {
        let rule = self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING);
        entity.map_or(rule, |entity| self.entity_grief(entity, rule))
    }
    /// Whether a mod may answer `can_entity_grief` differently from the rule:
    /// the sites that skip their whole work while griefing is off skip it only
    /// when no mod listens.
    pub(super) fn grief_hooked(&self) -> bool {
        mod_api::__private::listened(mod_api::HookId::EntityMobGriefing)
    }
    /// [`Self::can_entity_grief`] for `entity`, with the rule already read.
    pub(super) fn entity_grief(&self, entity: u32, rule: bool) -> bool {
        if !self.grief_hooked() {
            return rule;
        }
        let Some(epoch) = self.native_lifecycle.epoch() else {
            return rule;
        };
        let mut can_grief = rule;
        self.native_entity_mob_griefing(mod_api::EntityRef::new(epoch, entity), &mut can_grief);
        can_grief
    }
}



#[cfg(test)]
mod tests {
    use super::mob_effect_instance;

    #[test]
    fn nautilus_breath_lands_in_creative_as_ambient() {
        // AbstractNautilus.applyEffects: MobEffectInstance(BREATH_OF_THE_NAUTILUS,
        // 60, 0, ambient true, visible true, showIcon true), no game-mode gate.
        let e = mob_effect_instance("minecraft:breath_of_the_nautilus", 60, 0, true, true)
            .expect("applies to a creative player");
        assert_eq!(
            (e.id, e.duration, e.amplifier),
            ("breath_of_the_nautilus", 60, 0)
        );
        assert!(e.ambient && e.visible);
        // Survival gets the same ambient instance.
        let e =
            mob_effect_instance("minecraft:breath_of_the_nautilus", 60, 0, true, false).unwrap();
        assert!(e.ambient);
    }

    #[test]
    fn other_mob_effects_stay_blocked_in_creative() {
        assert!(mob_effect_instance("minecraft:poison", 140, 0, false, true).is_none());
        let e = mob_effect_instance("minecraft:poison", 140, 0, false, false).unwrap();
        assert!(!e.ambient);
    }

}


#[cfg(test)]
mod mob_subscription_tests {
    #[test]
    #[ignore = "daily"]
    fn empty_mod_never_defers_taming_or_block_destruction() {
        use mod_api::{self, Hooks};
        struct Empty;
        #[mod_api::__subscriptions]
        impl Hooks for Empty {}
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new("empty", Box::new(Empty))].into();
        mod_platform::testing::with_mods(mods, || {
            super::super::native_entity_refs::tests::with_game(|game| {
                let world = game.mob_world_base(&[]);
                assert!(!world.asks_animal_tame && !world.asks_living_destroy_block);
                let mut cancel = false;
                game.native_animal_tame(1, 1, 2, "minecraft:horse", &mut cancel);
                game.native_living_destroy_block(
                    2,
                    "minecraft:ravager",
                    [0; 3],
                    game.world.air,
                    &mut cancel,
                );
                let world = game.mob_world_base(&[]);
                assert!(!world.asks_animal_tame && !world.asks_living_destroy_block);
                assert!(!cancel);
            });
        });
    }
}


#[cfg(test)]
mod damage_type_tests {
    use super::*;

    #[test]
    fn named_damage_keeps_its_registry_id_with_or_without_an_attacker() {
        let named = horizon_client::mobs::NamedDamage::new(
            "example:fan_fire",
            horizon_client::mobs::NamedDamage::IS_FIRE,
        );
        for attacker in [None, Some(Who::Mob(7)), Some(Who::Player)] {
            assert_eq!(
                source_damage_type(Damage {
                    kind: DamageKind::Named(named),
                    attacker,
                    from: None,
                    attacker_species: None,
                }),
                "example:fan_fire"
            );
        }
    }

    /// `DamageSources.explosion`: a blast with a causing entity is
    /// `player_explosion` whoever caused it, so an exploding creeper's too.
    #[test]
    fn a_creeper_blast_is_a_player_explosion() {
        let blast = |attacker| Damage {
            kind: DamageKind::Explosion,
            attacker,
            from: Some([0.0; 3]),
            attacker_species: None,
        };
        let creeper = blast(Some(Who::Mob(7)));
        assert_eq!(source_damage_type(creeper), "minecraft:player_explosion");
        // No causing entity.
        assert_eq!(source_damage_type(blast(None)), "minecraft:explosion");
        // The other kinds do not ask.
        let bite = Damage {
            kind: DamageKind::MobAttack,
            ..creeper
        };
        assert_eq!(source_damage_type(bite), "minecraft:mob_attack");
    }
}

#[cfg(test)]
mod knockback_tests {
    use super::*;
    use horizon_client::health::{DamageHooks, DamageLevel, Hurting, Knock};

    /// What the hook of these tests saw: the player, its source (as damage
    /// type, causing, direct) and the push.
    type Pushes = std::sync::Mutex<Vec<(i32, (String, i32, i32), Knock)>>;
    static PUSHES: Pushes = std::sync::Mutex::new(Vec::new());

    /// Sets the native knockback hook of these tests for players 9201 to 9203: it records the push, cancels
    /// 9201's and doubles 9202's strength and turns its direction around.
    fn hook(_: DamageLevel<'_>, pushed: Hurting<'_>, knock: &mut Knock) -> bool {
        let Hurting::Player(entity @ 9201..=9203, s) = pushed else {
            return false;
        };
        let source = (s.damage_type.to_owned(), s.causing, s.direct);
        PUSHES.lock().unwrap().push((entity, source, *knock));
        match entity {
            9201 => return true,
            9202 => {
                knock.strength *= 2.0;
                knock.z = -knock.z;
            }
            _ => {}
        }
        false
    }

    /// `LivingEntity.knockback` on a player: the hook runs before the
    /// knockback resistance and the motion changes; a mod's strength or
    /// direction pushes, a cancel pushes nothing, and without a mod in the way
    /// the push is vanilla's. Needs the prepared data in
    /// `.local/horizon-client/26.3`; skipped without it.
    #[test]
    fn the_native_knockback_hook_changes_or_cancels_a_players_push() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("movement-properties.nbt").exists() {
            return;
        }
        let assets = horizon_client::assets::Assets::load(&root).unwrap();
        let resources = horizon_client::motion::MotionResources::load(&root, &assets).unwrap();
        let hooks = DamageHooks {
            knockback: Some(hook),
            ..DamageHooks::NONE
        };
        let mut test_level = horizon_client::health::TestLevel::with_hooks(1, hooks);
        let hit = HurtSource {
            damage_type: "minecraft:player_attack",
            causing: 5,
            direct: 5,
            position: None,
        };
        let mut push = |entity, resistance: f64| {
            let mut p = horizon_client::motion::PlayerMotion::new(&resources, Vec3::ZERO).unwrap();
            p.entity_id = entity;
            p.ground.on_ground = true;
            if resistance != 0.0 {
                let attribute = "minecraft:knockback_resistance";
                let set = p.set_modifier(attribute, "horizon:test", resistance, "add_value", true);
                set.unwrap();
            }
            crate::game::shield::apply_player_knockback(
                &mut p,
                &mut test_level.level(),
                0.4,
                (0.0, 3.0),
                hit,
                false,
            );
            (p.velocity.x, p.velocity.y, p.velocity.z)
        };
        // No mod in the way: vanilla's push, away from z = 3.
        assert_eq!(push(9200, 0.0), (0.0, 0.4, -0.4));
        // Canceled: no motion. The hook saw the push and the hit.
        assert_eq!(push(9201, 0.0), (0.0, 0.0, 0.0));
        let made = Knock {
            strength: 0.4,
            x: 0.0,
            z: 3.0,
        };
        let named = ("minecraft:player_attack".to_owned(), 5, 5);
        let seen = |entity| -> Vec<_> {
            let seen = PUSHES.lock().unwrap();
            seen.iter().filter(|p| p.0 == entity).cloned().collect()
        };
        assert_eq!(seen(9201), [(9201, named, made)]);
        // Twice the strength, direction turned around.
        assert_eq!(push(9202, 0.0), (0.0, 0.4, 0.8));
        // Before the resistance (`LivingKnockBackEvent`): a 0.5 resistance still hands the hook
        // the raw strength, and the push that follows is halved.
        assert_eq!(push(9203, 0.5), (0.0, 0.2, -0.2));
        assert_eq!(seen(9203)[0].2.strength, 0.4);
    }
}

impl horizon_client::arrows::OwnerLookup for Game<'_> {
    fn non_player_entity(&self, uuid: [i32; 4]) -> bool {
        let found = self.target_by_uuid(horizon_client::lan::playerdata::ints_uuid(uuid));
        !matches!(
            found,
            None | Some(super::commands::Target::Player | super::commands::Target::Guest(_))
        )
    }
}

#[cfg(test)]
mod conversion_subscriber_tests {
    use super::*;
    use mod_api::{self, Hooks, LivingConversionPostEvent, ModEntry, ServerHost};
    use std::sync::Mutex;

    #[test]
    #[ignore = "daily"]
    fn post_only_listener_is_subscribed_without_a_pre_listener() {
        struct DefaultMod;
        #[mod_api::__subscriptions]
        impl Hooks for DefaultMod {}
        struct PostOnly(Arc<Mutex<Vec<(u32, u32)>>>);
        #[mod_api::__subscriptions]
        impl Hooks for PostOnly {
            fn living_conversion_post(
                &self,
                _: &mut dyn ServerHost,
                event: &mut LivingConversionPostEvent<'_>,
            ) {
                assert_eq!(event.kind, "minecraft:zombie");
                assert_eq!(event.converted_kind, "minecraft:drowned");
                self.0.lock().unwrap().push((event.entity, event.converted));
            }
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mods = vec![
            ModEntry::new("default", Box::new(DefaultMod)),
            ModEntry::new("post", Box::new(PostOnly(seen.clone()))),
        ]
        .into();
        mod_platform::testing::with_mods(mods, || {
            for _ in 0..2 {
                let at = [0.5, 64.0, 0.5];
                let (mut game, id) = crate::game::ecs::test_support::ecs_game_with("minecraft:zombie", at);
                let old = game.ecs.entity_of(id).unwrap();
                let mut ecs = std::mem::replace(&mut game.ecs, crate::ecs_bridge::EcsLevel::inert());
                let new = {
                    let mw = game.mob_world_base(&[]);
                    ecs.spawn(&mw, "minecraft:drowned", at, None).unwrap()
                };
                game.ecs = ecs;
                let converted = game.ecs_converted(id, new);
                game.ecs.despawn(old);
                assert_eq!(seen.lock().unwrap().last(), Some(&(id, converted)));
                assert_eq!(game.ecs_kind(converted).as_deref(), Some("minecraft:drowned"));
                assert!(!mod_api::__private::listened(
                    mod_api::HookId::LivingConversionPre
                ));
            }
            assert_eq!(seen.lock().unwrap().len(), 2);
        });
    }
}

#[cfg(test)]
mod living_drops_tests {
    use super::*;
    use mod_api::ServerHost;

    #[test]
    fn a_projectile_blow_has_no_known_direct_entity_and_a_melee_one_is_its_causer() {
        let arrow = death_hurt_source("minecraft:arrow", 7);
        assert_eq!((arrow.causing, arrow.direct), (7, 0));
        let melee = death_hurt_source("minecraft:mob_attack", 7);
        assert_eq!((melee.causing, melee.direct), (7, 7));
        let none = death_hurt_source("minecraft:generic", 0);
        assert_eq!((none.causing, none.direct), (0, 0));
    }

}

#[cfg(test)]
mod armor_destroy_tests;

#[cfg(test)]
mod mob_pearl_hook_tests;


#[cfg(test)]
#[path = "rabbit_colour_tests.rs"]
mod rabbit_colour_tests;

#[cfg(test)]
mod chorus_projectile_drop_tests;

#[cfg(test)]
mod loaded_arrow_shot_tests;

/// `ServerLevel.getCurrentDifficultyAt` in `world`: the difficulty, the level's clock, and
/// the inhabited time and moon of the chunk at `at` (both 0 when it isn't loaded).
pub(in crate::game) fn local_difficulty_in(
    world: &horizon_client::world::World,
    at: [f64; 3],
) -> spawning::LocalDifficulty {
    spawning::LocalDifficulty::at(world, at)
}

#[cfg(test)]
mod death_loot_context_tests;

#[cfg(test)]
#[path = "frame_projectile_tests.rs"]
mod frame_projectile_tests;
