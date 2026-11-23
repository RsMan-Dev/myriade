//! Potions and status effects in the game: drinking (`PotionItem` via
//! `Consumable`), throwing splash/lingering potions (`ThrownSplashPotion`),
//! filling glass bottles (`BottleItem`), the per-tick effect application,
//! effect attribute modifiers.
#[path = "potions/cloud_particle.rs"]
mod cloud_particle;
use horizon_ecs_proto::{effect_cloud::CloudSpec, effects, targeting};
use super::{
    Game,
    splash_water::thrown_block_hit,
};
use horizon_client::health::DamageLevel;
use horizon_client::{
    brewing,
    effects::{EffectAction, EffectInstance, OmenTick, PlayerEffects},
    items::ItemStack,
    mobs::{Damage, DamageKind, NamedDamage, Who},
    particles::{ParticleOptions, ParticleType},
    projectiles::{Projectile, ProjectileKind},
    text::Text,
};
use horizon_core::nbt::Tag;
use horizon_core::{
    geometry::{Aabb, Axis, Direction, Vec3},
    random::{EntityRandom, LegacyRandom, RandomSource},
};
use mod_api::{EffectApplicable, EffectTickChange, EntityRef, LivingRef, PlayerFood};

/// Per-cloud settings; natural lingering clouds keep their creation defaults.
pub(super) struct CloudSettings {
    pub(super) wait_time: i32,
    pub(super) reapplication_delay: i32,
    pub(super) radius_on_use: f32,
    pub(super) duration_on_use: i32,
    pub(super) duration_scale: f32,
}

/// A thrown splash or lingering potion in flight.
#[derive(Clone, Debug)]
pub(super) struct ThrownPotion {
    /// Its entity id once a server's clients track it (0 until then).
    id: u32,
    stack: ItemStack,
    /// The position before the last tick, for drawing between ticks.
    previous: Vec3,
    position: Vec3,
    velocity: Vec3,
    age: u32,
    /// `Projectile.getOwner`'s display name: the player who threw it (none
    /// for a dispenser's).
    owner: Option<Text>,
    /// `Projectile.getOwner` as a target: the thrower (`Who::Player`, a
    /// guest's with `guest` set), none for a dispenser's or a server's.
    thrower: Option<Who>,
    /// On a server, the guest (by connection) who threw it.
    guest: Option<u32>,
    /// `Projectile.leftOwner`: until then it can't hit its thrower.
    left_owner: bool,
    /// `Projectile.lastDeflectedBy`: a mob deflects it once in a row.
    last_deflected_by: Option<u32>,
    /// `Entity.random`: `ProjectileDeflection.REVERSE` draws from the
    /// projectile's own source, not the level's.
    random: EntityRandom,
}
/// The player (not one of `skip`, the thrower and who rides with them before
/// it has left them) whose box, grown by `margin` (`ProjectileUtil.computeMargin`),
/// holds `p` (`Projectile.canHitEntity`).
fn thrown_player_at<'a>(
    players: &'a [(Who, Aabb)],
    skip: &[Who],
    p: Vec3,
    margin: f64,
) -> Option<&'a (Who, Aabb)> {
    players.iter().find(|(who, b)| {
        !skip.contains(who) && b.inflate(Vec3::new(margin, margin, margin)).contains(p)
    })
}
/// `ProjectileUtil.computeMargin`: how far the pick box is grown at `age`.
fn thrown_margin(age: u32) -> f64 {
    ((f64::from(age) - 2.0) / 20.0).clamp(0.0, 0.3)
}
/// `Projectile.isOutsideOwnerCollisionRange` (`checkLeftOwner`): none of the
/// boxes of the thrower's vehicle tree (`owner.getRootVehicle().getSelfAndPassengers()`,
/// here `ridden`) is within a block of the potion's (0.25 wide) grown by its
/// motion; true too when there is no thrower (`ridden` empty).
fn thrown_left_owner(ridden: &[Aabb], position: Vec3, velocity: Vec3) -> bool {
    let h = 0.125;
    let reach = Aabb::new(
        Vec3::new(position.x - h, position.y, position.z - h),
        Vec3::new(position.x + h, position.y + 2.0 * h, position.z + h),
    )
    .expand_towards(velocity)
    .inflate(Vec3::new(1.0, 1.0, 1.0));
    !ridden.iter().any(|b| b.intersects(reach))
}
/// `AreaEffectCloud` wait time set by `ThrownLingeringPotion.onHitAsPotion`.
pub(super) const LINGERING_WAIT: i32 = 10;
/// `AreaEffectCloud.DEFAULT_REAPPLICATION_DELAY`.
const REAPPLICATION_DELAY: i32 = 20;
pub(in crate::game) use cloud_particle::particle_options;
/// An entity a cloud has affected (`AreaEffectCloud.victims`).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Victim {
    Mob(u32),
}
/// A potion-bearing `AreaEffectCloud`: its effects, settings and recent victims.
pub(super) struct PotionCloud {
    pub(super) settings: CloudSettings,
    effects: Vec<(&'static str, i32, i32)>,
    /// The effects above that were added hidden (`MobEffectInstance.isVisible` false: no
    /// particles); the instances the cloud applies are copies and keep that.
    hidden: Vec<&'static str>,
    /// The effects above that were ambient (`MobEffectInstance.isAmbient`).
    ambient: Vec<&'static str>,
    /// `AreaEffectCloud.radiusPerTick`: natural clouds use `-radius / duration`.
    pub(super) radius_per_tick: f32,
    /// The potion's colour (`PotionContents.getColor`).
    color: u32,
    /// `AreaEffectCloud.customParticle`: the `custom_particle` compound as saved.
    custom_particle: Option<Tag>,
}
impl PotionCloud {
    /// `AreaEffectCloud.updateParticle`: the custom particle when one is set,
    /// else an entity effect in the potion's colour.
    pub(super) fn particle(&self) -> ParticleOptions {
        self.custom_particle
            .as_ref()
            .and_then(cloud_particle::custom_particle_options)
            .unwrap_or_else(|| {
                let c = |shift: u32| (self.color >> shift & 255) as f32 / 255.0;
                ParticleOptions::colored(ParticleType::EntityEffect, [c(16), c(8), c(0), 1.0])
            })
    }
}
/// `Projectile.getOwner` of a thrown potion: what its `indirectMagic` damage
/// is named after and who it counts against. Nothing for a dispenser's or a
/// summoned potion nobody owns, the potion itself then naming the death
/// message.
#[derive(Clone, Debug, Default)]
pub(super) struct PotionOwner {
    /// The thrower's name, when it has one.
    name: Option<Text>,
    /// The mob that threw it (a witch): the hurt is its kill credit, as
    /// `indirectMagic(potion, owner)` gives it. A player's throw is the
    /// potion's own and only `name`s the message.
    attacker: Option<u32>,
    /// A player threw it: the water damage `indirectMagic` deals is theirs.
    player: bool,
    /// On a server, the guest (by connection) who threw it: whose kill the mobs it hurts are
    /// (`LivingEntity.lastHurtByPlayer`).
    guest: Option<u32>,
}
impl PotionOwner {
    /// The owner as a damage source's causing entity.
    fn who(&self) -> Option<Who> {
        match self.attacker {
            Some(id) => Some(Who::Mob(id)),
            None => self.player.then_some(Who::Player),
        }
    }
}
/// A mob's effect ids carry the `minecraft:` namespace; a mod's has its own.
pub(in crate::game) fn mob_effect_id(id: &str) -> Option<&'static str> {
    brewing::effect(id).map(|e| brewing::namespaced(e.id))
}
/// `LivingEntity.canBeAffected` with the mob overrides (`WitherBoss` and
/// `WitherSkeleton` shrug off wither, `Spider` and `AbstractNautilus` poison,
/// `Parched` weakness): the immune-to-infested and immune-to-oozing tags
/// decide first, then the tag of mobs that ignore poison and regeneration.
/// `WitherBoss.addEffect` refusing everything is `Mob::add_effect`'s: a cloud
/// still takes the wither as a victim (`AreaEffectCloud` asks `canBeAffected`),
/// so its instant effects land and it shrinks.
pub(super) fn mob_can_be_affected(
    tags: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    kind: &str,
    effect: &str,
) -> bool {
    let effect = effect.trim_start_matches("minecraft:");
    let kind = kind.trim_start_matches("minecraft:");
    let overridden = match kind {
        "wither" | "wither_skeleton" => effect == "wither",
        "spider" | "cave_spider" | "nautilus" | "zombie_nautilus" => effect == "poison",
        "parched" => effect == "weakness",
        _ => false,
    };
    if overridden {
        return false;
    }
    let in_tag = |tag: &str| {
        tags.get(tag)
            .is_some_and(|t| t.contains(&format!("minecraft:{kind}")))
    };
    if in_tag("minecraft:immune_to_infested") {
        effect != "infested"
    } else if in_tag("minecraft:immune_to_oozing") {
        effect != "oozing"
    } else {
        !in_tag("minecraft:ignores_poison_and_regen")
            || (effect != "regeneration" && effect != "poison")
    }
}
impl Game<'_> {
    /// The dragon's breath (`AreaEffectCloud` with instant damage at
    /// `amplifier`) on a mob: what a cloud puts on it, `indirectMagic(cloud,
    /// dragon)` with no owner to credit (the dragon is not a mob here).
    pub(super) fn breath_affect_mob(&mut self, mob: u32, amplifier: i32, at: [f64; 3]) {
        let source = Text::translate("entity.minecraft.area_effect_cloud", &[]);
        let effect = [("instant_damage", 1, amplifier)];
        // DragonFireball.onHit and the flame phase: `setPotionDurationScale(0.25)`.
        self.cloud_affect(
            Victim::Mob(mob),
            &effect,
            (&[], &[], 0.25),
            at,
            &source,
            (None, None),
        );
    }
}
/// `ThrownSplashPotion.onHitAsPotion`: the squared distance from the potion's
/// box at the hit point to an entity's box grown by the hit margin
/// (`ProjectileUtil.computeMargin`), for entities that meet the potion's box
/// grown by (4, 2, 4); none when it is 16 or more.
pub(in crate::game) fn splash_distance_sqr(at: Vec3, age: u32, bounds: Aabb) -> Option<f64> {
    let potion = Aabb::new(
        Vec3::new(at.x - 0.125, at.y, at.z - 0.125),
        Vec3::new(at.x + 0.125, at.y + 0.25, at.z + 0.125),
    );
    if !potion.inflate(Vec3::new(4.0, 2.0, 4.0)).intersects(bounds) {
        return None;
    }
    let margin = ((f64::from(age) - 2.0) / 20.0).clamp(0.0, 0.3);
    let d2 = potion.distance_squared_to_box(bounds.inflate(Vec3::new(margin, margin, margin)));
    (d2 < 16.0).then_some(d2)
}
impl Game<'_> {
    /// `LivingEntity.triggerOnDeathMobEffects` (`onMobRemoved` with the reason
    /// KILLED) for what Oozing, Weaving and Wind Charged do when their bearer
    /// dies. `bearer` is the dying mob's id, none when a player dies; `silent`
    /// whether that source is heard, and `random` the dying mob's own
    /// (`mob.getRandom()`; the world's for the player).
    pub(super) fn effects_on_death(
        &mut self,
        effects: &[&str],
        bounds: Aabb,
        silent: bool,
        bearer: Option<u32>,
        mut random: Option<&mut LegacyRandom>,
    ) {
        let player = bearer.is_none();
        let feet = bounds.bottom_center();
        if effects.contains(&"oozing") && self.ecs.is_active() {
            // OozingMobEffect.numberOfSlimesToSpawn, on the ECS level.
            let limit = self
                .session
                .rules
                .int_at(horizon_client::gamerules::vanilla::MAX_ENTITY_CRAMMING);
            let count = if limit >= 1 {
                self.ecs_oozing_count(bounds, bearer.and_then(|b| self.ecs.entity_of(b)), limit)
            } else {
                2
            };
            let feet = [feet.x, feet.y, feet.z];
            for _ in 0..count {
                self.ecs_spawn_slime_offspring(feet);
            }
        }
        // `WeavingMobEffect.onMobRemoved`: `canEntityGrief` for the dying mob.
        if effects.contains(&"weaving") && (player || self.can_entity_grief(bearer)) {
            self.spawn_cobwebs_around(
                [
                    feet.x.floor() as i32,
                    feet.y.floor() as i32,
                    feet.z.floor() as i32,
                ],
                random.as_deref_mut(),
            );
        }
        if effects.contains(&"wind_charged") {
            // A burst centred at half the height, radius 3 to 5.
            let center = [feet.x, feet.y + bounds.size().y / 2.0, feet.z];
            let r = random.unwrap_or(&mut self.world.random);
            let radius = 3.0 + r.next_float() * 2.0;
            // WindChargedMobEffect: the bearer bursts, a living source.
            let bearer = if player {
                Some(self.own_entity_id() as u32)
            } else {
                bearer
            };
            let cause = super::wind::BurstCause {
                source: bearer,
                owner: bearer,
            };
            let _ = self.wind_burst(
                center,
                radius,
                1.0,
                super::wind::Source::wind_charged(player, silent).with_cause(cause),
            );
        }
    }
    /// `WeavingMobEffect.spawnCobwebsRandomlyAround`: two or three cobwebs in
    /// free spaces with a solid floor, picked among 15 random blocks within one
    /// block of `pos`.
    fn spawn_cobwebs_around(&mut self, pos: [i32; 3], mut random: Option<&mut LegacyRandom>) {
        let r = random.as_deref_mut().unwrap_or(&mut self.world.random);
        let wanted = 2 + r.next_int_bound(2).unwrap_or(0) as usize;
        let mut chosen: Vec<[i32; 3]> = Vec::new();
        for _ in 0..15 {
            let r = random.as_deref_mut().unwrap_or(&mut self.world.random);
            let at = [
                pos[0] - 1 + r.next_int_bound(3).unwrap_or(0),
                pos[1] - 1 + r.next_int_bound(3).unwrap_or(0),
                pos[2] - 1 + r.next_int_bound(3).unwrap_or(0),
            ];
            let facts = self.res.facts();
            let here = facts.get(self.world.block(at[0], at[1], at[2]));
            let below = facts.get(self.world.block(at[0], at[1] - 1, at[2]));
            if !chosen.contains(&at)
                && here.replaceable
                && below.sturdy_full & (1 << Direction::Up as u8) != 0
            {
                chosen.push(at);
                if chosen.len() >= wanted {
                    break;
                }
            }
        }
        for at in chosen {
            if self.set_state_named(at, "minecraft:cobweb", &[]).is_ok() {
                self.level_event_at(3018, at, 0);
            }
        }
    }
    /// `InfestedMobEffect.onMobHurt` for the player: from half its height up,
    /// along where it looks.
    pub(super) fn player_infested(&mut self) {
        if let Some(p) = self.player.as_ref() {
            let at = [
                p.position.x,
                p.position.y + f64::from(p.dimensions.height) / 2.0,
                p.position.z,
            ];
            let look = super::view_vector(self.yaw, self.pitch);
            // The player's draws come from the world's random.
            let velocities =
                horizon_client::mobs::infested_silverfish(&mut self.world.random, look);
            self.spawn_infested(at, velocities);
        }
    }
    /// `InfestedMobEffect.spawnSilverfish`: a silverfish at `at` (half the
    /// bearer's height up) for each of the velocities the bearer's random drew
    /// (`horizon_client::mobs::infested_silverfish`).
    pub(super) fn spawn_infested(&mut self, at: [f64; 3], velocities: Vec<Vec3>) {
        for velocity in velocities {
            if self.ecs.is_active() {
                self.ecs_spawn_silverfish(at, velocity);
            }
            self.audio
                .play("minecraft:entity.silverfish.hurt", Some(at), 1.0, 1.0);
        }
    }
}
/// A splash potion, lingering potion or experience bottle about to fly (the
/// player's throws and dispensers both make one): `ThrownSplashPotion`,
/// `ThrownLingeringPotion` or `ThrownExperienceBottle`, which
/// `level.addFreshEntity` makes a level entity like a summoned one, so
/// selectors and `/data` find it (#13201).
pub(super) fn thrown_projectile(stack: ItemStack, position: Vec3, by_player: bool) -> Projectile {
    let kind = match &*stack.item {
        "minecraft:experience_bottle" => ProjectileKind::ExperienceBottle,
        item => {
            let lingering = item == "minecraft:lingering_potion";
            ProjectileKind::Potion(Box::new(stack), lingering)
        }
    };
    Projectile::new(kind, position, by_player.then_some(Who::Player))
}
/// What a player's body was last given for its effects and enchantments:
/// a guest keeps their own while the host's are in `PotionState`.
#[derive(Default)]
pub(super) struct BodyPotions {
    pub(super) applied: Vec<(&'static str, &'static str, f64, &'static str)>,
    protection_for: Option<(u64, Vec<Option<ItemStack>>)>,
    enchant_modifiers: Vec<(String, String, f64, String)>,
}
/// Effects, thrown potions and the attribute modifiers last applied.
#[derive(Default)]
pub(super) struct PotionState {
    pub(super) effects: PlayerEffects,
    pub(super) thrown: Vec<ThrownPotion>,
    pub(super) applied: Vec<(&'static str, &'static str, f64, &'static str)>,
    /// Armor the protection table was computed for.
    protection_for: Option<(u64, Vec<Option<ItemStack>>)>,

    /// Enchantment attribute modifiers on the player: (attribute, id, amount, operation).
    pub(super) enchant_modifiers: Vec<(String, String, f64, String)>,
}

impl PotionState {
    /// Trade the player's own state (effects and what their body was given)
    /// with a guest's, around `as_guest`. Potions in flight stay.
    pub(super) fn swap_player(&mut self, effects: &mut PlayerEffects, body: &mut BodyPotions) {
        std::mem::swap(&mut self.effects, effects);
        std::mem::swap(&mut self.applied, &mut body.applied);
        std::mem::swap(&mut self.protection_for, &mut body.protection_for);
        std::mem::swap(&mut self.enchant_modifiers, &mut body.enchant_modifiers);
    }
    /// Their entity ids, for a server to number them.
    pub(super) fn thrown_ids(&mut self) -> impl Iterator<Item = &mut u32> {
        self.thrown.iter_mut().map(|t| &mut t.id)
    }
    /// The numbered ones as a server's clients track them: (id, entity
    /// type, item, position, motion).
    pub(super) fn thrown_tracked(
        &self,
    ) -> impl Iterator<Item = (u32, &'static str, &ItemStack, Vec3, Vec3)> {
        self.thrown.iter().filter(|t| t.id != 0).map(|t| {
            (
                t.id,
                thrown_kind(&t.stack),
                &t.stack,
                t.position,
                t.velocity,
            )
        })
    }
}

/// The entity type of a thrown potion or bottle.
fn thrown_kind(stack: &ItemStack) -> &'static str {
    match &*stack.item {
        "minecraft:lingering_potion" => "minecraft:lingering_potion",
        "minecraft:experience_bottle" => "minecraft:experience_bottle",
        _ => "minecraft:splash_potion",
    }
}

impl Game<'_> {
    /// Right-click with a potion, splash/lingering potion or glass bottle.
    pub(super) fn use_potion_item(&mut self, stack: &ItemStack) -> horizon_client::Result<bool> {
        match &*stack.item {
            "minecraft:potion" | "minecraft:ominous_bottle" => {
                self.start_using_item();
                Ok(true)
            }
            "minecraft:splash_potion"
            | "minecraft:lingering_potion"
            | "minecraft:experience_bottle" => {
                let Some(eye) = self.eye_position() else {
                    return Ok(true);
                };
                let pitch = 0.4 / (self.world.random.next_float() * 0.4 + 0.8);
                let (sound, power) = match &*stack.item {
                    "minecraft:splash_potion" => ("minecraft:entity.splash_potion.throw", 0.5),
                    "minecraft:experience_bottle" => {
                        ("minecraft:entity.experience_bottle.throw", 0.7)
                    }
                    _ => ("minecraft:entity.lingering_potion.throw", 0.5),
                };
                self.audio.play(sound, Some(eye), 0.5, pitch);
                // Projectile.shootFromRotation(xRot, yRot, -20, power, 1.0), with
                // what the thrower last moved (getKnownMovement).
                let velocity = self.known_player_movement();
                let on_ground = self.player.as_ref().is_some_and(|p| p.ground.on_ground);
                let one = ItemStack {
                    count: 1,
                    ..stack.clone()
                };
                let mut shot =
                    thrown_projectile(one, Vec3::new(eye[0], eye[1] - 0.1, eye[2]), true);
                shot.shoot_from_rotation(
                    self.pitch, self.yaw, -20.0, power, 1.0, velocity, on_ground,
                );
                self.ecs_launch_item(
                    shot.entity_type(),
                    [shot.position.x, shot.position.y, shot.position.z],
                    [shot.velocity.x, shot.velocity.y, shot.velocity.z],
                    self.ecs_owner(shot.owner),
                    shot.item.clone(),
                );
                // ThrowablePotionItem.use / ExperienceBottleItem.use: awardStat(ITEM_USED).
                self.award_stat("used", &stack.item, 1);
                self.swing_arm();
                self.consume_held();
                Ok(true)
            }
            "minecraft:glass_bottle" => {
                // `BottleItem.use`: before the water check, it looks for a living
                // AreaEffectCloud owned by an EnderDragon within the player's box
                // grown by 2. If one is found, it shrinks the cloud's radius by 0.5,
                // plays item.bottle.fill_dragonbreath at the player, emits
                // GameEvent.FLUID_PICKUP, fires PLAYER_INTERACTED_WITH_ENTITY on the
                // cloud, and turns one bottle into dragon_breath.
                let Some(hit) = self.pick_fluid() else {
                    return Ok(false);
                };
                let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
                if self.res.facts().get(state).fluid != horizon_client::blocks::Fluid::Water {
                    return Ok(false);
                }
                // The sound at the player, the game event at the water block.
                let Some(at) = self
                    .player
                    .as_ref()
                    .map(|p| [p.position.x, p.position.y, p.position.z])
                else {
                    return Ok(false);
                };
                self.audio
                    .play("minecraft:item.bottle.fill", Some(at), 1.0, 1.0);
                self.post_block_game_event(
                    "minecraft:fluid_pickup",
                    hit.pos,
                    Some(Who::Player),
                    None,
                );
                // BottleItem.turnBottleIntoItem.
                self.award_stat("used", &stack.item, 1);
                let water = ItemStack::new("minecraft:potion", 1).with_potion("minecraft:water");
                self.fill_held(water);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    /// `PotionItem.useOn`: a water bottle used on a block that turns to mud
    /// (`#convertible_to_mud`), not on its underside, is poured out and the
    /// block becomes mud.
    pub(super) fn use_water_bottle_on_block(
        &mut self,
        pos: [i32; 3],
        face: [i32; 3],
        stack: &ItemStack,
    ) -> horizon_client::Result<bool> {
        if &*stack.item != "minecraft:potion"
            || stack.potion() != Some("minecraft:water")
            || face == [0, -1, 0]
            || !self.named_block_in_tag("minecraft:convertible_to_mud", &self.block_name(pos))
        {
            return Ok(false);
        }
        let centre = pos.map(|v| f64::from(v) + 0.5);
        self.audio
            .play("minecraft:entity.generic.splash", Some(centre), 1.0, 1.0);
        self.fill_held(ItemStack::new("minecraft:glass_bottle", 1));
        // PotionItem.useOn: five ServerLevel.sendParticles(SPLASH). The level
        // random places each one (count 1, no spread, speed 1). The packet is
        // what the window draws; a window that runs this itself still adds it.
        for _ in 0..5 {
            let r = &mut self.world.random;
            let at = [
                f64::from(pos[0]) + r.next_double(),
                f64::from(pos[1] + 1),
                f64::from(pos[2]) + r.next_double(),
            ];
            let options = ParticleOptions::simple(ParticleType::Splash);
            self.lan_send_particles(super::lan_particles::QueuedParticle {
                dimension: self.dim.id(),
                options,
                state: None,
                at,
                spread: [0.0; 3],
                speed: [1.0; 3],
                count: 1,
                randomization: 0,
            });
        }
        self.audio
            .play("minecraft:item.bottle.empty", Some(centre), 1.0, 1.0);
        self.post_block_game_event("minecraft:fluid_place", pos, None, None);
        if let Some(mud) = self.pack.assets.registry.default_state("minecraft:mud") {
            self.level_set(pos, mud)?;
        }
        self.award_stat("used", &stack.item, 1);
        self.swing_arm();
        Ok(true)
    }
    /// `ItemUtils.createFilledResult`: replace one of the held stack.
    fn fill_held(&mut self, filled: ItemStack) {
        if self.creative {
            let _ = self.add_stack_to_inventory(filled);
            return;
        }
        self.consume_held();
        if self.held().is_none() {
            *self.held_mut() = Some(filled);
        } else if let Some(rest) = self.add_stack_to_inventory(filled)
            && let Some(eye) = self.eye_position()
        {
            // `player.drop(stack, false)`.
            self.throw_from_eyes(rest, eye, false, false);
        }
    }
    /// Drinking a potion while right-click is held; true when handled.
    pub(super) fn tick_drinking(&mut self, slot: usize, ticks: u32, stack: &ItemStack) -> bool {
        let ominous = &*stack.item == "minecraft:ominous_bottle";
        if &*stack.item != "minecraft:potion" && !ominous {
            return false;
        }
        // the duration the use started with (32, as a mod's Start or Tick left it).
        let duration = self.use_duration;
        let ticks = ticks + 1;
        let remaining = duration.saturating_sub_unsigned(ticks).max(0);
        let eye = self.eye_position().unwrap_or_default();
        // Consumable.emitParticlesAndSounds: the drink sound every 4 ticks near the end.
        if remaining <= 25 && remaining % 4 == 0 {
            let pitch = self.world.random.next_float() * 0.1 + 0.9;
            self.audio
                .play("minecraft:entity.generic.drink", Some(eye), 0.5, pitch);
        }
        // ItemStack.onUseTick: the item's own tick after the consumable's.
        if !self.item_on_use_tick(stack, ticks - 1) {
            return true;
        }
        if i64::from(ticks) < i64::from(duration) {
            self.using = Some((slot, ticks));
            return true;
        }
        self.finish_drinking(slot, stack);
        self.use_item_finished(stack);
        true
    }
    /// `PotionItem.finishUsingItem` (and `OminousBottleItem`'s) of `stack` in
    /// `slot`, the use complete: false for anything else.
    pub(super) fn finish_drinking(&mut self, slot: usize, stack: &ItemStack) -> bool {
        let ominous = &*stack.item == "minecraft:ominous_bottle";
        if &*stack.item != "minecraft:potion" && !ominous {
            return false;
        }
        let eye = self.eye_position().unwrap_or_default();
        self.using = None;
        self.send_use_item_complete();
        // Consumable.onConsume: the used statistic and the consume_item
        // trigger, with the stack as drunk (before it shrinks).
        self.award_stat("used", &stack.item, 1);
        self.trigger_item("minecraft:consume_item", stack);
        self.post_entity_event("minecraft:drink", horizon_client::mobs::Who::Player);
        if ominous {
            self.drink_ominous_bottle(slot, stack);
            return true;
        }
        // PotionContents.onConsume: the potion's and the custom effects, at the
        // stack's POTION_DURATION_SCALE.
        let (lasting, now) = PlayerEffects::contents_effects(
            &brewing::contents_effects(stack),
            stack.potion_duration_scale(),
        );
        for e in lasting {
            self.add_player_effect(e);
        }
        for a in now {
            self.apply_effect_action(a);
        }
        if !self.creative {
            if let Some(s) = self.stack_in_mut(slot).as_mut() {
                s.count -= 1;
                if s.count == 0 {
                    *self.stack_in_mut(slot) = None;
                }
            }
            let bottle = ItemStack::new("minecraft:glass_bottle", 1);
            if self.stack_in(slot).is_none() {
                *self.stack_in_mut(slot) = Some(bottle);
            } else if let Some(rest) = self.add_stack_to_inventory(bottle) {
                self.spawn_item(rest, eye, [0.0, 0.1, 0.0], 40);
            }
        }
        true
    }
    pub(super) fn apply_effect_action(&mut self, action: EffectAction) {
        let action = match action {
            EffectAction::Heal(mut amount) => {
                if let Some(player) = self.player.as_ref() {
                    let mut cancel = false;
                    horizon_client::hunger::native_living_heal(
                        &self.loop_context,
                        player.entity_id as u32,
                        &mut amount,
                        &mut cancel,
                    );
                    if cancel {
                        return;
                    }
                }
                EffectAction::Heal(amount)
            }
            action => action,
        };
        let rules = self.world.health_rules;
        let Some(p) = self.player.as_mut() else {
            self.combat.forget_noted(false);
            return;
        };
        match action {
            EffectAction::Heal(amount) => {
                let max = p.attribute("minecraft:max_health").unwrap_or(20.0) as f32;
                let h = p.health.health();
                if h > 0.0 {
                    let _ = p.set_health((h + amount).min(max), false);
                }
            }
            EffectAction::Hurt(source, amount) => {
                let mut canceled = false;
                if let Some(id) = self.motion.health.damage.id(source)
                    && let Ok(o) = p.hurt_blocking(
                        crate::game::entity::damage_level_of!(self, scoped),
                        &self.motion.health,
                        rules,
                        id,
                        amount,
                        None,
                    )
                {
                    canceled = o.canceled;
                }

                self.combat.forget_noted(canceled);
            }
            EffectAction::Poison => {
                if p.health.health() > 1.0
                    && let Some(id) = self.health_resources.damage.id("minecraft:magic")
                {
                    let _ = p.hurt(
                        crate::game::entity::damage_level_of!(self, scoped),
                        &self.motion.health,
                        rules,
                        id,
                        1.0,
                    );
                }
            }
            EffectAction::Exhaust(amount) => {
                p.cause_food_exhaustion(amount);
            }
            EffectAction::Eat(nutrition, saturation) => {
                let food = &mut p.health.food;
                food.food = (food.food + nutrition).clamp(0, 20);
                food.saturation = (food.saturation + nutrition as f32 * saturation * 2.0)
                    .clamp(0.0, food.food as f32);
            }
        }
    }
    /// The local player's effects (`tick_mob_effects`), then the potions
    /// in flight, which a server without a local player still has. A frozen
    /// tick (`/tick freeze`) leaves the potions where they are
    /// (`TickRateManager.isEntityFrozen`); players' effects still run.
    pub(super) fn tick_effects(&mut self, running: bool) {
        self.tick_mob_effects();
        if running {
            self.tick_thrown_potions();
        }
    }
    /// `LivingEntity.tickEffects` (server side) for `self.player`, the local
    /// player or a guest acting (`as_guest`), then its attribute modifiers
    /// and status flags.
    pub(super) fn tick_mob_effects(&mut self) {
        let Some(player) = self.player.as_ref() else {
            return;
        };
        if player.health.dead_or_dying() {
            // LivingEntity.die → removeAllEffects. Vanilla runs the effects
            // that act on death (`onMobRemoved`) when the dead player is
            // removed, up to 20 ticks later; here it is as the effects go.
            let w = f64::from(player.dimensions.width) / 2.0;
            let bounds = Aabb::new(
                Vec3::new(
                    player.position.x - w,
                    player.position.y,
                    player.position.z - w,
                ),
                Vec3::new(
                    player.position.x + w,
                    player.position.y + f64::from(player.dimensions.height),
                    player.position.z + w,
                ),
            );
            let ids: Vec<&'static str> = self.potions.effects.list.iter().map(|e| e.id).collect();
            self.potions.effects.clear();
            if !ids.is_empty() {
                self.effects_on_death(&ids, bounds, self.player_silent(), None, None);
            }
        }
        // The omens act from inside the effect tick (BadOmenMobEffect,
        // RaidOmenMobEffect), as a content mod's effects do.
        let absorption_empty = self
            .player
            .as_ref()
            .is_some_and(|p| p.health.absorption() <= 0.0);
        let mut effects = std::mem::take(&mut self.potions.effects);
        // `MobEffectEvent.Expired`: ask before the pass, for the effects at
        // zero ticks and the ones reaching it in this pass (the last tick of
        // their countdown); a canceled expiry keeps the effect at no ticks,
        // asking again next tick. Only on the server, where the effects tick.
        let keep_expired: Vec<&'static str> = {
            let player = self.effect_living_ref(None);
            effects
                .list
                .iter()
                .filter(|e| matches!(e.duration, 0 | 1))
                .filter(|e| {
                    let Some(player) = player else {
                        return false;
                    };
                    let mut cancel = false;
                    self.native_mob_effect_expired(
                        player,
                        mod_api::__private::effect_key_by_index(
                            e.registry_index.expect("active effect registry index"),
                        )
                        .expect("effects resolved at load"),
                        e.amplifier,
                        &mut cancel,
                    );
                    cancel
                })
                .map(|e| e.id)
                .collect()
        };
        let (actions, raid_omen_added) = effects.tick_with(
            &mut |e, t, actions, dispatch| {
                for action in actions.drain(..) {
                    self.apply_effect_action(action);
                }
                if !dispatch {
                    return OmenTick::Continue;
                }
                if !brewing::is_mod_effect(e.id) {
                    self.omen_tick(e)
                } else {
                    let Some(player) = self.player.as_mut() else {
                        return OmenTick::Remove;
                    };
                    let Some(epoch) = self.native_lifecycle.epoch() else {
                        return OmenTick::Continue;
                    };
                    let holder = LivingRef::new(EntityRef::new(epoch, player.entity_id as u32));
                    let health = player.health.health();
                    let max_health =
                        player.attribute("minecraft:max_health").unwrap_or(20.0) as f32;
                    let food = &player.health.food;
                    let food = Some(PlayerFood::from(mod_api::__private::built::PlayerFood {
                        level: food.food,
                        saturation: food.saturation,
                        exhaustion: food.exhaustion,
                    }));
                    let (keep, changes) = self.mod_effect_tick(
                        mod_api::__private::effect_key_by_index(
                            e.registry_index.expect("active effect registry index"),
                        )
                        .expect("effects resolved at load"),
                        None,
                        t,
                        e.amplifier,
                        holder,
                        health,
                        max_health,
                        food,
                    );
                    let registry = &self.motion.health.damage;
                    actions.extend(changes.into_iter().filter_map(|change| {
                        match change {
                            EffectTickChange::Heal(amount) => Some(EffectAction::Heal(amount)),
                            // An unregistered damage type hurts nothing.
                            EffectTickChange::Hurt(source, amount) => {
                                NamedDamage::registered(registry, &source)
                                    .map(|damage| EffectAction::Hurt(damage.name, amount))
                            }
                            EffectTickChange::AddExhaustion(amount) => {
                                Some(EffectAction::Exhaust(amount))
                            }
                            EffectTickChange::Eat(nutrition, saturation) => {
                                Some(EffectAction::Eat(nutrition, saturation))
                            }
                            _ => unreachable!("EffectTickChange: unhandled variant"),
                        }
                    }));
                    if keep {
                        OmenTick::Continue
                    } else {
                        OmenTick::Remove
                    }
                }
            },
            absorption_empty,
            &keep_expired,
        );
        self.potions.effects = effects;
        if raid_omen_added {
            self.raid_omen_added();
        }
        for action in actions {
            self.apply_effect_action(action);
        }
        self.turtle_helmet_tick();
        self.apply_effect_status();
    }
    /// `Player.tick` -> `turtleHelmetTick`: with a turtle helmet on the head
    /// and the eyes out of the water, Water Breathing for 200 ticks (no
    /// particles, an icon).
    fn turtle_helmet_tick(&mut self) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let eye_y = p.position.y + f64::from(p.dimensions.eye_height);
        if !p.health.dead_or_dying()
            && self.armor[0]
                .as_ref()
                .is_some_and(|s| &*s.item == "minecraft:turtle_helmet")
            && !self.eye_in_water(p.position.x, eye_y, p.position.z)
        {
            let mut effect = EffectInstance::new("water_breathing", 200, 0);
            effect.visible = false;
            self.potions.effects.add(effect);
        }
    }
    /// Add an active effect to the player and update attributes / absorption.
    /// `LivingEntity.canBeAffected` with the `mob_effect_applicable` hook:
    /// the event fires first, for every effect, and `Allow` or `Deny`
    /// decides; unanswered, the vanilla tag check runs. `entity` is a mob's
    /// `Entity.getId`, None for the player.
    pub(in crate::game) fn can_be_affected(
        &mut self,
        entity: Option<i32>,
        kind: &str,
        effect: &str,
    ) -> bool {
        let key = brewing::effect(effect)
            .and_then(|def| mod_api::__private::effect_key_by_index(def.index));
        self.can_be_affected_resolved(entity, kind, effect, key)
    }

    /// The validated registry path, also used by native operations without parsing a name again.
    pub(super) fn can_be_affected_resolved(
        &mut self,
        entity: Option<i32>,
        kind: &str,
        effect: &str,
        key: Option<mod_api::EffectKey>,
    ) -> bool {
        let mut answered = None;
        if let Some(living) = self.effect_living_ref(entity)
            && let Some(key) = key
        {
            self.native_mob_effect_applicable(living, key, &mut answered);
        }
        match answered {
            Some(EffectApplicable::Allow) => true,
            Some(EffectApplicable::Deny) => false,
            // An unanswered hook, or a future variant, defers to the
            // vanilla tag check.
            _ => mob_can_be_affected(&self.res.stations().enchantments.entity_tags, kind, effect),
        }
    }
    pub(super) fn add_player_effect(&mut self, effect: EffectInstance) -> bool {
        // `LivingEntity.addEffect` refuses what cannot affect the entity;
        // players pass the tag check, but the `mob_effect_applicable` hook
        // is asked for every add.
        let key = effect
            .registry_index
            .and_then(mod_api::__private::effect_key_by_index);
        if !self.can_be_affected_resolved(None, "minecraft:player", effect.id, key) {
            return false;
        }
        let id = effect.id;
        let amp = effect.amplifier;
        let new = self.potions.effects.get(id).is_none();
        let changed = self.potions.effects.add(effect);
        self.apply_effect_status();
        // MobEffect.onEffectAdded: an omen's sound when it is new, at the
        // player's feet (#11373).
        if new
            && changed
            && let Some(sound) = sound_on_added(id)
            && let Some(p) = self.player.as_ref()
        {
            let at = [p.position.x, p.position.y, p.position.z];
            self.audio.play(sound, Some(at), 1.0, 1.0);
        }
        if id == "absorption"
            && let Some(p) = self.player.as_mut()
        {
            let target = 4.0 * (amp + 1) as f32;
            let current = p.health.absorption();
            let _ = p.set_health(current.max(target), true);
        }
        if let Some(key) = key {
            self.mod_effect_added_resolved(key, None, amp, new);
        }
        changed
    }
    /// Removes one effect, or all effects, and refreshes the player's derived state.
    pub(super) fn remove_player_effects(&mut self, effect: Option<usize>) -> bool {
        let before = self.potions.effects.list.len();
        let mut index = 0;
        while index < self.potions.effects.list.len() {
            let current = &self.potions.effects.list[index];
            // The one being removed, or all of them when `effect` is None.
            if effect.is_some_and(|index| current.registry_index != Some(index)) {
                index += 1;
                continue;
            }
            let amplifier = current.amplifier;
            // `MobEffectEvent.Remove`: refusing the removal keeps it, and the
            // next one is asked with this one still in place, as in vanilla.
            let mut cancel = false;
            if let Some(living) = self.effect_living_ref(None) {
                self.native_mob_effect_remove(
                    living,
                    mod_api::__private::effect_key_by_index(
                        current
                            .registry_index
                            .expect("active effect registry index"),
                    )
                    .expect("effects resolved at load"),
                    amplifier,
                    &mut cancel,
                );
            }
            if cancel {
                index += 1;
            } else {
                self.potions.effects.list.remove(index);
            }
        }
        let changed = self.potions.effects.list.len() != before;
        if changed {
            self.apply_effect_status();
        }
        changed
    }
    /// `LivingEntity.addEffect` for a content mod's effect: its
    /// `onEffectAdded` when it is new, then its `onEffectStarted`, on a
    /// server. `entity` is a mob's id, None for the player.
    pub(in crate::game) fn mod_effect_added(
        &mut self,
        effect: &str,
        entity: Option<i32>,
        amplifier: i32,
        new: bool,
    ) {
        if let Some(effect) = brewing::effect(effect)
            .and_then(|def| mod_api::__private::effect_key_by_index(def.index))
        {
            self.mod_effect_added_resolved(effect, entity, amplifier, new);
        }
    }
    /// Dispatch an already resolved effect's added notices, without a registry name lookup.
    pub(super) fn mod_effect_added_resolved(
        &mut self,
        effect: mod_api::EffectKey,
        entity: Option<i32>,
        amplifier: i32,
        new: bool,
    ) {
        if true
            && mod_api::__private::effect_index(effect)
                .and_then(horizon_client_core::potions::effect_by_index)
                .is_some_and(|def| def.index >= brewing::EFFECTS.len())
        {
            if new {
                self.native_on_effect_added(effect, entity, amplifier);
            }
            self.native_mob_effect_added(effect, entity, amplifier);
        }
    }
    /// `MobEffectInstance.tickServer` for a content mod's effect: its
    /// `shouldApplyEffectTickThisTick`, then its `applyEffectTick`, which
    /// may end it. Whether it stays.
    fn mod_effect_tick(
        &self,
        effect: mod_api::EffectKey,
        entity: Option<i32>,
        tick_count: i32,
        amplifier: i32,
        holder: LivingRef,
        health: f32,
        max_health: f32,
        food: Option<PlayerFood>,
    ) -> (bool, Vec<EffectTickChange>) {
        let mut apply = false;
        self.native_should_apply_effect_tick_this_tick(effect, tick_count, amplifier, &mut apply);
        let mut keep = true;
        let mut changes = Vec::new();
        if apply {
            self.native_apply_effect_tick(mod_api::__private::ApplyEffectTickCall {
                effect,
                dimension: self.dim.id(),
                entity,
                amplifier,
                holder,
                health,
                max_health,
                food,
                changes: &mut changes,
                keep: &mut keep,
            });
        }
        (keep, changes)
    }
    /// The effects' attribute modifiers, armor protection and status flags
    /// on `self.player`.
    pub(super) fn apply_effect_status(&mut self) {
        // LivingEntity.onEffectAdded, onEffectUpdated and onEffectsRemoved
        // change the attributes on a server only. A server's client has the
        // modifiers its ClientboundUpdateAttributesPacket sends, which comes a
        // tick after the effect's own packet when the effect starts or ends in
        // the player's tick (ServerPlayer.doTick runs after the tick's
        // sendDirtyEntityData). Its own would slow it a tick before the
        // server's player when Speed runs out (#12040).
        let modifiers = self.potions.effects.modifiers();
        if true
            && modifiers != self.potions.applied
            && let Some(p) = self.player.as_mut()
        {
            for (attr, id, _, op) in &self.potions.applied {
                let _ = p.set_modifier_unclamped(attr, id, 0.0, op, false);
            }
            for (attr, id, amount, op) in &modifiers {
                let _ = p.set_modifier_unclamped(attr, id, *amount, op, true);
            }
            // onAttributeUpdated, once the batch settled: a lost health_boost.
            let _ = p.clamp_health_to_max();
            self.potions.applied = modifiers;
        }
        if let Some(p) = self.player.as_mut()
            && self.potions.effects.amplifier("absorption").is_none()
            && p.health.absorption() > 0.0
        {
            let _ = p.set_health(0.0, true);
        }
        self.refresh_armor_protection();
        let mut status = self.potions.effects.status();
        if let Some(p) = self.player.as_mut() {
            status.protection = std::mem::take(&mut p.status.protection);
            p.status = status;
        }
    }
    /// `EnchantmentHelper.getDamageProtection` after equipment changes. This
    /// does not tick effects or clear absorption while a hit is in progress.
    pub(in crate::game) fn refresh_armor_protection(&mut self) {
        // EnchantmentHelper.getDamageProtection per damage type, recomputed on armor change.
        let armor: Vec<Option<ItemStack>> = self.armor.to_vec();
        if !self
            .potions
            .protection_for
            .as_ref()
            .is_some_and(|(generation, cached)| {
                *generation == self.damage_tag_generation && cached == &armor
            })
        {
            let mut table = Vec::new();
            let e = &self.stations().enchantments;
            let mut random = horizon_core::random::LegacyRandom::new(0);
            for (_, def) in self.health_resources.damage.entries() {
                let ctx = horizon_client::enchant::EffectContext {
                    items: Some(self.items()),
                    damage_type: Some(&def.name),
                    this: Some("minecraft:player"),
                    ..Default::default()
                };
                let total: f32 = armor
                    .iter()
                    .flatten()
                    .map(|s| {
                        e.modify_value(
                            Some(s),
                            "minecraft:damage_protection",
                            &ctx,
                            &mut random,
                            0.0,
                        )
                    })
                    .sum();
                if total > 0.0 {
                    table.push((def.name.clone(), total));
                }
            }
            self.potions.protection_for = Some((self.damage_tag_generation, armor));
            if let Some(p) = self.player.as_mut() {
                p.status.protection = table;
            }
        }
    }
    /// The players a thrown potion can hit (`Projectile.canHitEntity`): the
    /// host and the guests, but not a spectator or a dead one.
    fn thrown_player_targets(&self) -> Vec<(Who, Aabb)> {
        let mut targets = Vec::new();
        if let Some(p) = self.player_view()
            && p.alive
            && !self.spectator()
        {
            targets.push((Who::Player, p.bounds));
        }
        targets.extend(self.lan_projectile_targets());
        targets
    }
    /// The thrower's vehicle tree as `Projectile` sees it
    /// (`owner.getRootVehicle().getSelfAndPassengers()`): the players in it (the
    /// thrower and who shares their vehicle), the mob that is the vehicle, and
    /// the boxes of all of them. Until the potion has left it, none can be hit
    /// (`canHitEntity`: `isPassengerOfSameVehicle`) and any one near keeps it
    /// from leaving (`isOutsideOwnerCollisionRange`).
    pub(super) fn thrown_ridden(
        &self,
        owner: Option<Who>,
        players: &[(Who, Aabb)],
    ) -> (Vec<Who>, Option<u32>, Vec<Aabb>) {
        let Some(owner) = owner else {
            return (Vec::new(), None, Vec::new());
        };
        let vehicle_of = |who: Who| match who {
            Who::Player => self.riding,
            Who::Mob(id) => self
                .lan
                .as_ref()
                .and_then(|h| h.hub.guests.iter().find(|g| g.entity_id as u32 == id))
                .and_then(|g| g.vehicle),
        };
        let vehicle = vehicle_of(owner);
        let mut who = vec![owner];
        let mut boxes = Vec::new();
        for (w, b) in players {
            if *w == owner || (vehicle.is_some() && vehicle_of(*w) == vehicle) {
                if *w != owner {
                    who.push(*w);
                }
                boxes.push(*b);
            }
        }
        if let Some(v) = vehicle {
            boxes.extend(self.ecs_mob_bounds(v));
            if self.is_boat_id(v)
                && let Some(b) = self.ecs.bounds(v)
            {
                boxes.push(b);
            }
        }
        (who, vehicle, boxes)
    }
    /// `ThrowableProjectile.tick` for the potions in flight: the motion, then
    /// the first block, mob or player the step meets
    /// (`ProjectileUtil.getHitResultOnMoveVector`), where the potion splashes
    /// (`Projectile.onHit`). Its thrower is not met until it has left them
    /// (`Projectile.leftOwner`, checked before the move like `Projectile.tick`),
    /// nor anything else in their vehicle tree.
    fn tick_thrown_potions(&mut self) {
        let mut thrown = std::mem::take(&mut self.potions.thrown);
        let players = if thrown.is_empty() {
            Vec::new()
        } else {
            self.thrown_player_targets()
        };
        let mut landed = Vec::new();
        let mut deflected: Vec<u32> = Vec::new();
        // ServerLevel.tick: only entities in entity-ticking chunks tick.
        let ticking = self.entity_ticking_area();
        thrown.retain_mut(|t| {
            if !ticking(t.position.x.floor() as i32, t.position.z.floor() as i32) {
                t.previous = t.position;
                return true;
            }
            t.age += 1;
            // Projectile.tick: whether it has left its thrower's vehicle tree,
            // before it moves.
            let owner = self.shooter_who(t.thrower, t.guest);
            let (ridden, vehicle, boxes) = self.thrown_ridden(owner, &players);
            if !t.left_owner {
                t.left_owner = thrown_left_owner(&boxes, t.position, t.velocity);
            }
            let skip: &[Who] = if t.left_owner { &[] } else { &ridden };
            t.previous = t.position;
            self.thrown_potion_motion(t);
            let next = Vec3::new(
                t.position.x + t.velocity.x,
                t.position.y + t.velocity.y,
                t.position.z + t.velocity.z,
            );
            // Hit a block along the step (sampled), a mob, a player, or fell out
            // of the world.
            let margin = thrown_margin(t.age);
            let steps = 4;
            let mut hit = None;
            let mut deflect_stop = false;
            for i in 1..=steps {
                let f = f64::from(i) / f64::from(steps);
                let p = Vec3::new(
                    t.position.x + t.velocity.x * f,
                    t.position.y + t.velocity.y * f,
                    t.position.z + t.velocity.z * f,
                );
                let state =
                    self.world
                        .block(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
                // (id, feet, is a breeze) of the creature the step meets.
                let mob = self.ecs_thrown_mob_at(p, vehicle, t.left_owner);
                // A player (`canHitEntity`: not the thrower before it has left
                // them), the box grown by the 0.3 pick margin.
                let player = thrown_player_at(&players, skip, p, margin);
                if self.res.facts().get(state).full_collision || mob.is_some() || player.is_some() {
                    // Projectile.hitTargetOrDeflectSelf: a breeze's deflection
                    // (`Breeze.deflection`) turns the potion back at half
                    // speed instead of `onHit`, and it flies on.
                    if let Some((mid, _, true)) = mob {
                        if t.last_deflected_by != Some(mid) {
                            // ProjectileDeflection.REVERSE.
                            let _ = t.random.next_float();
                            t.velocity = Vec3::new(
                                t.velocity.x * -0.5,
                                t.velocity.y * -0.5,
                                t.velocity.z * -0.5,
                            );
                            t.last_deflected_by = Some(mid);
                            deflected.push(mid);
                        }
                        // ThrowableProjectile.tick: `setPos(hitResult.getLocation())`
                        // before the deflection, so it does not step on past the
                        // breeze this tick.
                        t.position = p;
                        deflect_stop = true;
                        break;
                    }
                    // A block hit has the face it came in by
                    // (`onHitBlock`); an entity hit has none.
                    let block = (mob.is_none() && player.is_none())
                        .then(|| thrown_block_hit(t.position, p, t.velocity));
                    let at = mob.map(|(_, at, _)| at).or_else(|| {
                        player.map(|(_, b)| {
                            let c = b.center();
                            Vec3::new(c.x, b.min(Axis::Y), c.z)
                        })
                    });
                    hit = Some((p, at, block));
                    break;
                }
            }
            if let Some((p, mob, block)) = hit {
                landed.push((
                    t.id,
                    t.stack.clone(),
                    p,
                    mob,
                    block,
                    t.age,
                    t.owner.clone(),
                    t.guest,
                    self.entity_silent(t.id),
                    t.velocity,
                ));
                return false;
            }
            if deflect_stop {
                return t.age < 1200 && t.position.y >= self.world.below_world_y();
            }
            t.position = next;
            let bounds = Aabb::new(
                next.add_vector(Vec3::new(-0.125, 0.0, -0.125)),
                next.add_vector(Vec3::new(0.125, 0.25, 0.125)),
            );
            self.entity_powder_snow_particles(t.previous, next, bounds, t.velocity, false);
            t.age < 1200 && t.position.y >= self.world.below_world_y()
        });
        self.potions.thrown = thrown;
        for id in deflected {
            self.ecs_mob_sound(id, "minecraft:entity.breeze.deflect", 5, 1.0, 1.0, false);
        }
        for (id, stack, at, mob, block, age, owner, guest, silent, motion) in landed {
            // Projectile.onHit (super.onHit of AbstractThrownPotion): a block
            // is heard at its centre, an entity at the hit.
            let heard = if mob.is_some() {
                [at.x, at.y, at.z]
            } else {
                [at.x.floor() + 0.5, at.y.floor() + 0.5, at.z.floor() + 0.5]
            };
            let thrower = owner.is_some().then_some(Who::Player);
            self.projectile_landed_event(id, thrower, heard);
            let item = stack.item.to_string();
            // A potion a dispenser threw has nobody to name.
            let owner = PotionOwner {
                player: owner.is_some(),
                name: owner,
                attacker: None,
                guest,
            };
            self.splash(&stack, at, mob, block, age, &owner, silent, motion);
            if item != "minecraft:experience_bottle" {
                self.native_potion_splash(&item, [at.x, at.y, at.z]);
            }
        }
    }
    /// `AreaEffectCloud.readAdditionalSaveData`: loaded clouds join the same
    /// simulation as lingering potions, with their own timing and potion data.
    /// The caller retains their UUID and original compound for command queries.
    pub(super) fn load_potion_cloud(&mut self, tag: &Tag) -> Option<u32> {
        self.load_potion_cloud_transferred(tag, None)
    }
    pub(super) fn load_potion_cloud_transferred(
        &mut self,
        tag: &Tag,
        transferred: Option<super::entity_initialization::TransferredEntityState>,
    ) -> Option<u32> {
        let Tag::Compound(c) = tag else { return None };
        let e = self.ecs_with(|ecs, mw| ecs.load_entity(mw, c))?;
        let id = self.ecs_id_of(e);
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        Some(id)
    }
    /// A lingering potion cloud in the ECS: `spawn_cloud` takes its spec whole (brews, flags, source, owner); P doses and ends it.
    pub(super) fn spawn_potion_cloud(&mut self, at: [f64; 3], spec: CloudSpec) -> u32 {
        let mut next = self.next_entity;
        let id = self.ecs_with(|ecs, mw| {
            ecs.spawn_cloud(mw, at, spec, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        id
    }
    /// `Creeper.spawnLingeringCloud`: an `AreaEffectCloud` of radius 2.5, wait time 10, duration 300,
    /// shrinking by `-radius / duration` a tick and by 0.5 for each entity it affects, applying
    /// the creeper's effects at 0.25 of their remaining duration.
    pub(super) fn spawn_creeper_cloud(
        &mut self,
        at: [f64; 3],
        effects: Vec<(&'static str, i32, i32)>,
        hidden: Vec<&'static str>,
        ambient: Vec<&'static str>,
        color: u32,
    ) {
        // The creeper's effect list has no potion id: the brews are its custom effects, with their flags.
        let mut spec = CloudSpec::new(2.5, "", None);
        spec.custom = Self::ecs_brews(&effects);
        spec.hidden = hidden.iter().filter_map(|n| horizon_ecs_proto::effects::kind_of(n)).collect();
        spec.ambient = ambient.iter().filter_map(|n| horizon_ecs_proto::effects::kind_of(n)).collect();
        spec.radius_per_tick = -2.5 / 300.0;
        spec.duration = 300;
        spec.wait = LINGERING_WAIT;
        spec.reapplication = REAPPLICATION_DELAY;
        spec.radius_on_use = -0.5;
        spec.duration_scale = 0.25;
        spec.color = Some(color as i32);
        self.spawn_potion_cloud(at, spec);
    }
    /// What a cloud puts on one entity: instant effects at half strength
    /// (`applyInstantaneousEffect` with 0.5), the others at the cloud's duration
    /// scale (`PotionContents.forEachEffect`).
    fn cloud_affect(
        &mut self,
        victim: Victim,
        effects: &[(&'static str, i32, i32)],
        (hidden, ambient, duration_scale): (&[&'static str], &[&'static str], f32),
        at: [f64; 3],
        source: &Text,
        (owner, guest): (Option<Who>, Option<u32>),
    ) {
        // `indirectMagic(cloud, owner)` names the cloud in the death message
        // (it gives no kill credit).
        let cloud_owner = PotionOwner {
            name: Some(source.clone()),
            ..Default::default()
        };
        // MobEffectInstance.withScaledDuration: floor(duration * scale), at least 1; an infinite
        // effect stays infinite.
        let scaled = |duration| horizon_client::effects::scaled_duration(duration, duration_scale);
        let Victim::Mob(mob) = victim;
        let scaled_effects: Vec<(&'static str, i32, i32)> =
            effects.iter().map(|&(id, duration, amp)| (id, scaled(duration), amp)).collect();
        let amount = |base: i32, amp: i32| (0.5 * f64::from(base.wrapping_shl(amp as u32)) + 0.5) as i32 as f32;
        self.ecs_mob_effects(mob, &scaled_effects, hidden, ambient, &amount);
    }
    /// `ThrowableProjectile.tick`'s `applyGravity` and `applyInertia`, which
    /// come before the move: gravity 0.05 (0.07 for an experience bottle),
    /// then drag 0.99 (0.8 in water).
    fn thrown_potion_motion(&self, t: &mut ThrownPotion) {
        let gravity = if &*t.stack.item == "minecraft:experience_bottle" {
            0.07
        } else {
            0.05
        };
        let at = t.position;
        let water = self
            .res
            .facts()
            .get(self.world.block(
                at.x.floor() as i32,
                at.y.floor() as i32,
                at.z.floor() as i32,
            ))
            .fluid
            == horizon_client::blocks::Fluid::Water;
        let inertia = if water { 0.8 } else { 0.99 };
        t.velocity = Vec3::new(
            t.velocity.x * inertia,
            (t.velocity.y - gravity) * inertia,
            t.velocity.z * inertia,
        );
    }
    /// `HealOrHarmMobEffect.applyInstantaneousEffect` with a source (a
    /// potion or a cloud): indirectMagic, not plain magic, credited to the
    /// mob that threw it when there is one.
    fn apply_indirect_effect_action(&mut self, a: EffectAction, owner: &PotionOwner) {
        let a = match a {
            EffectAction::Hurt("minecraft:magic", amount) => {
                match (owner.attacker, &owner.name) {
                    (Some(id), Some(name)) => {
                        self.note_named_attacker("minecraft:indirect_magic", id, name.clone())
                    }
                    _ => {
                        let name = owner.name.clone().unwrap_or_else(|| {
                            Text::translate("entity.minecraft.splash_potion", &[])
                        });
                        self.note_direct_entity("minecraft:indirect_magic", name);
                    }
                }
                EffectAction::Hurt("minecraft:indirect_magic", amount)
            }
            a => a,
        };
        self.apply_effect_action(a);
    }
    /// A splash potion's effects on the player in place, by their distance,
    /// for the stack's `potion_duration_scale`.
    fn splash_on_player(
        &mut self,
        effects: &[(&'static str, i32, i32)],
        at: Vec3,
        age: u32,
        owner: &PotionOwner,
        duration_scale: f64,
    ) {
        if let Some(p) = self.player.as_ref() {
            let w = f64::from(p.dimensions.width) / 2.0;
            let bounds = Aabb::new(
                Vec3::new(p.position.x - w, p.position.y, p.position.z - w),
                Vec3::new(
                    p.position.x + w,
                    p.position.y + f64::from(p.dimensions.height),
                    p.position.z + w,
                ),
            );
            if let Some(d2) = splash_distance_sqr(at, age, bounds) {
                let intensity = 1.0 - d2.sqrt() / 4.0;
                for (id, duration, amp) in effects {
                    let instant = brewing::effect(id).is_some_and(|e| e.instant);
                    if instant {
                        if let Some(a) = horizon_client::effects::instant(id, *amp, intensity) {
                            self.apply_indirect_effect_action(a, owner);
                        }
                    } else {
                        let d = (intensity * f64::from(*duration) * duration_scale + 0.5) as i32;
                        if d > 20
                            && let Some(e) = brewing::effect(id)
                        {
                            self.add_player_effect(EffectInstance::new(e.id, d, *amp));
                        }
                    }
                }
            }
        }
    }
    /// The ECS owner of a cloud a thrower made: the acting player's entity (a guest's by its connection), or the ECS entity of the
    /// mob that threw it; None for nobody's.
    fn cloud_owner(&self, owner: &PotionOwner) -> Option<targeting::Who> {
        if owner.player {
            let id = match owner.guest {
                Some(conn) => self.lan.as_ref()?.hub.guests.iter().find(|g| g.conn == conn)?.entity_id as u32,
                None => self.player.as_ref()?.entity_id as u32,
            };
            return Some(targeting::Who::Player(id));
        }
        self.ecs.entity_of(owner.attacker?).map(targeting::Who::Entity)
    }
    /// `AbstractThrownPotion.onHit` with `ThrownSplashPotion.onHitAsPotion`, or
    /// `ThrownLingeringPotion.onHitAsPotion` for a lingering potion, read from
    /// the stack's `potion_contents`; `mob` is where it hit a mob, `hit` the
    /// block and face it hit, `age` its age, `silent` its Silent flag, `motion` its motion as it hit.
    pub(super) fn splash(
        &mut self,
        stack: &ItemStack,
        at: Vec3,
        mob: Option<Vec3>,
        hit: Option<([i32; 3], Direction)>,
        age: u32,
        owner: &PotionOwner,
        silent: bool,
        motion: Vec3,
    ) {
        let block = [at.x, at.y, at.z].map(|v| v.floor() as i32);
        if &*stack.item == "minecraft:experience_bottle" {
            // ThrownExperienceBottle.onHit: the water-coloured splash (level
            // event 2002, its sound 1053) and 3 to 11 experience.
            self.level_event_at(2002, block, 0xFF38_5DC6u32 as i32);
            if !silent {
                self.level_event_at(1053, block, 0);
            }
            let r = &mut self.world.random;
            let value = 3 + r.next_int_bound(5).unwrap_or(0) + r.next_int_bound(5).unwrap_or(0);
            // awardWithDirection: away from the face it hit, or back along
            // its own motion when it hit an entity (#11388).
            let roughly = match hit {
                Some((_, face)) => face.normal().map(f64::from),
                None => [-motion.x, -motion.y, -motion.z],
            };
            self.spawn_orbs_toward([at.x, at.y, at.z], roughly, value as u32);
            self.native_exp_bottle([at.x, at.y, at.z], value as u32);
            return;
        }
        // Projectile.onHit calls onHitBlock first, then the potion's
        // affectEntitiesAround; both read the stack's `potion_contents`.
        if let Some((cell, face)) = hit {
            // `AbstractThrownPotion.douseFire` passes `getOwner()` to the
            // campfire's `douse`, which is where the game event's source is.
            let who = if owner.player {
                Some(Who::Player)
            } else {
                owner.attacker.map(Who::Mob)
            };
            self.potion_hit_block(stack, cell, face, who);
        }
        self.affect_entities_around(stack, at, owner.player);
        let lingering = &*stack.item == "minecraft:lingering_potion";
        // AbstractThrownPotion.onHit: level event 2007 and sound 1054 for a
        // potion with instant effects, 2002 and 1053 otherwise.
        let potion = stack.potion().and_then(brewing::potion);
        let instant = potion.is_some_and(|d| {
            d.effects
                .iter()
                .any(|(id, ..)| brewing::effect(id).is_some_and(|e| e.instant))
        });
        // `PotionContents.getColor`: an explicit `custom_color` wins over the
        // amplifier-weighted mix of the potion's and the stack's custom effects.
        let color = brewing::contents_color(stack);
        self.level_event_at(if instant { 2007 } else { 2002 }, block, color as i32);
        if !silent {
            self.level_event_at(if instant { 1054 } else { 1053 }, block, 0);
        }
        // `PotionContents.hasEffects`: the potion's effects and the stack's
        // custom ones; without either `onHitAsPotion` is not called at all.
        let effects = brewing::contents_effects(stack);
        if effects.is_empty() {
            return;
        }
        if lingering {
            // The cloud sits at the mob it hit (its feet), else the impact;
            // radius 3, wait 10, 600 ticks, shrinking to nothing, with the
            // stack's `potion_contents` and `potion_duration_scale`.
            let at = mob.unwrap_or(at);
            // The named potion's brews come from its id; the stack's custom effects are the cloud's own.
            let potion = effects::potion_name(stack.potion().unwrap_or("")).unwrap_or("");
            let mut spec = CloudSpec::new(3.0, potion, self.cloud_owner(owner));
            spec.radius_per_tick = -3.0 / 600.0;
            spec.duration = 600;
            spec.wait = LINGERING_WAIT;
            spec.reapplication = REAPPLICATION_DELAY;
            spec.radius_on_use = -0.5;
            spec.duration_scale = stack.potion_duration_scale();
            spec.color = Some(color as i32);
            spec.custom = effects::custom_brews(stack);
            self.spawn_potion_cloud([at.x, at.y, at.z], spec);
            return;
        }
        let duration_scale = f64::from(stack.potion_duration_scale());
        // Every player in range (`ThrownSplashPotion.applySplash` over
        // `LivingEntity`s, the players among them).
        self.for_each_player(|g| g.splash_on_player(&effects, at, age, owner, duration_scale));
        // The ECS creatures take their share in `ecs_splash_mobs` (reaction included).
        if self.ecs.is_active() {
            self.ecs_splash_mobs(at, age, &effects, duration_scale as f32);
        }
    }
    /// Effects saved in level.dat (`Player.active_effects`).
    pub(super) fn restore_effects(&mut self) {
        self.potions = PotionState::default();
        self.potions.effects = PlayerEffects::from_saved(&self.level.active_effects);
    }
}

/// A mob's content mod effects tick in its own tick (`ai::tick_effects`).
impl horizon_client::mobs::ModEffectTicks for Game<'_> {
    fn effect_expired(&self, mob: u32, effect: usize, amplifier: i32, cancel: &mut bool) {
        if let Some(living) = self.effect_living_ref(Some(mob as i32))
            && let Some(effect) = mod_api::__private::effect_key_by_index(effect)
        {
            self.native_mob_effect_expired(living, effect, amplifier, cancel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client::motion::PlayerMotion;
    fn effect_test_mod() {
        use mod_api::{self, *};
        struct TestMod;
        #[mod_api::__subscriptions]
        impl Hooks for TestMod {
            fn should_apply_effect_tick_this_tick(
                &self,
                _: &dyn Host,
                event: &mut ShouldApplyEffectTickThisTickEvent,
            ) {
                event.apply = true;
            }
            fn apply_effect_tick(&self, _: &dyn Host, event: &mut ApplyEffectTickEvent<'_>) {
                match event.amplifier {
                    0 => {
                        event.heal(10.0);
                        event.eat(4, 0.5);
                        event.add_exhaustion(1.5);
                    }
                    1 => event.hurt("minecraft:magic", 3.0),
                    3 if event.health > 1.0 => event.hurt("minecraft:magic", 1.0),
                    3 => {}
                    _ => event.hurt("minecraft:wither", 100.0),
                }
            }
        }
        mod_platform::__private::install(std::sync::Arc::from([mod_api::ModEntry::new(
            "test-potions",
            Box::new(TestMod),
        )]));
        assert!(brewing::register_mod_effects([(
            "test:effect".into(),
            0,
            brewing::Category::Neutral,
            None,
        )]));
    }

    #[test]
    #[ignore = "daily"]
    fn mod_tick_heals_and_feeds_player() {
        // Mod effects are frozen process-wide: another fixture may have registered first.
        if super::super::native_entity_refs::tests::run_fresh_cases(
            "game::potions::tests::mod_tick_heals_and_feeds_player",
            "HORIZON_POTION_EFFECT_CHILD",
            &["effect"],
        ) {
            return;
        }
        effect_test_mod();
        super::super::native_entity_refs::tests::with_game(|game| {
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            let player = game.player.as_mut().unwrap();
            player.set_health(18.0, false).unwrap();
            player.health.food.food = 14;
            player.health.food.saturation = 2.0;
            let id = brewing::effect("test:effect").unwrap().id;
            game.potions.effects.add(EffectInstance::new(id, 10, 0));
            game.tick_mob_effects();
            let player = game.player.as_ref().unwrap();
            assert_eq!(player.health.health(), 20.0);
            assert_eq!(player.health.food.food, 18);
            assert_eq!(player.health.food.saturation, 6.0);
            assert_eq!(player.health.food.exhaustion, 1.5);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn mod_tick_magic_respects_player_hit_frames() {
        // Mod effects are frozen process-wide: another fixture may have registered first.
        if super::super::native_entity_refs::tests::run_fresh_cases(
            "game::potions::tests::mod_tick_magic_respects_player_hit_frames",
            "HORIZON_POTION_EFFECT_CHILD",
            &["effect"],
        ) {
            return;
        }
        effect_test_mod();
        super::super::native_entity_refs::tests::with_game(|game| {
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            let id = brewing::effect("test:effect").unwrap().id;
            game.potions.effects.add(EffectInstance::new(id, 10, 1));
            game.tick_mob_effects();
            assert_eq!(game.player.as_ref().unwrap().health.health(), 17.0);
            game.tick_mob_effects();
            assert_eq!(game.player.as_ref().unwrap().health.health(), 17.0);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn mod_tick_sees_prior_regeneration() {
        // Mod effects are frozen process-wide: another fixture may have registered first.
        if super::super::native_entity_refs::tests::run_fresh_cases(
            "game::potions::tests::mod_tick_sees_prior_regeneration",
            "HORIZON_POTION_EFFECT_CHILD",
            &["effect"],
        ) {
            return;
        }
        effect_test_mod();
        super::super::native_entity_refs::tests::with_game(|game| {
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            game.player
                .as_mut()
                .unwrap()
                .set_health(1.0, false)
                .unwrap();
            game.potions
                .effects
                .add(EffectInstance::new("regeneration", 50, 0));
            let id = brewing::effect("test:effect").unwrap().id;
            game.potions.effects.add(EffectInstance::new(id, 10, 3));
            game.tick_mob_effects();
            assert_eq!(game.player.as_ref().unwrap().health.health(), 1.0);
        });
    }

    /// A mod effect's damage type keeps its registry id and tags whether it is
    /// vanilla's or a datapack's, and one the registry lacks is refused.
    #[test]
    fn effect_damage_sources_retain_their_ids() {
        let type_json = |id: &str| {
            serde_json::json!({
                "id": id, "message_id": "m", "exhaustion": "00000000",
                "scaling": "never", "effects": "hurt", "death_message_type": "default"
            })
        };
        let catalog = horizon_world::damage_source::Catalog::from_json(
            &serde_json::json!({
                "data_version": horizon_world::DATA_VERSION,
                "types": [type_json("minecraft:magic"), type_json("demo:scorch")],
            }),
            &serde_json::json!({
                "minecraft:is_fire": "{\"values\":[\"demo:scorch\"]}",
                "minecraft:bypasses_armor": "{\"values\":[\"demo:scorch\",\"minecraft:magic\"]}",
            }),
        )
        .unwrap();
        for (id, tags) in [
            ("minecraft:magic", NamedDamage::BYPASSES_ARMOR),
            (
                "demo:scorch",
                NamedDamage::IS_FIRE | NamedDamage::BYPASSES_ARMOR,
            ),
        ] {
            let named = NamedDamage::registered(&catalog, id).unwrap();
            assert_eq!((named.name, named.tags), (id, tags));
            let kind = Game::damage_kind(&catalog, id);
            assert_eq!(
                super::super::creatures::source_damage_type(Damage::of(kind)),
                id
            );
        }
        assert_eq!(NamedDamage::registered(&catalog, "demo:missing"), None);
    }

    /// `WitherBoss.canBeAffected` refuses only the wither effect: its `addEffect`
    /// refuses the rest, but a cloud of strength or healing still counts the boss
    /// as a victim (`AreaEffectCloud.serverTick` asks `canBeAffected`).
    #[test]
    fn a_cloud_still_counts_the_wither_as_a_victim() {
        let tags = std::collections::HashMap::new();
        for effect in ["strength", "instant_health"] {
            assert!(
                mob_can_be_affected(&tags, "minecraft:wither", effect),
                "{effect}"
            );
        }
        assert!(!mob_can_be_affected(&tags, "minecraft:wither", "wither"));
    }

    /// #5876: a potion thrown straight up from inside its thrower's box does
    /// not hit them while it climbs out, then hits them on the way down;
    /// another player in its path is hit at once.
    #[test]
    fn a_thrown_potion_hits_its_thrower_only_after_leaving_them() {
        // A player's box: 0.6 wide, 1.8 high, feet at the origin.
        let body = |x: f64| Aabb::new(Vec3::new(x - 0.3, 0.0, -0.3), Vec3::new(x + 0.3, 1.8, 0.3));
        let thrower = Who::Mob(1);
        let players = [(thrower, body(0.0)), (Who::Mob(2), body(5.0))];
        let ridden = [body(0.0)];
        let eye = Vec3::new(0.0, 1.52, 0.0);
        let up = Vec3::new(0.0, 0.5, 0.0);
        let m = thrown_margin(10);
        assert!((m - 0.3).abs() < 1e-12 && thrown_margin(1) == 0.0);
        // Thrown from the eye: still within a block of its thrower.
        assert!(!thrown_left_owner(&ridden, eye, up));
        assert!(thrown_player_at(&players, &[thrower], eye, m).is_none());
        // Without the gate the thrower's box does hold it.
        assert_eq!(
            thrown_player_at(&players, &[], eye, m).map(|t| t.0),
            Some(thrower)
        );
        // Four blocks up and rising: clear of the box grown by a block.
        let high = Vec3::new(0.0, 5.0, 0.0);
        assert!(thrown_left_owner(&ridden, high, up));
        // Falling back (left_owner now set), the thrower is hit at their head.
        let head = Vec3::new(0.0, 2.0, 0.0);
        assert_eq!(
            thrown_player_at(&players, &[], head, m).map(|t| t.0),
            Some(thrower)
        );
        // Another player is hit though the thrower has not left; a miss is none.
        let beside = Vec3::new(5.2, 1.0, 0.0);
        assert_eq!(
            thrown_player_at(&players, &[thrower], beside, m).map(|t| t.0),
            Some(Who::Mob(2))
        );
        assert!(thrown_player_at(&players, &[thrower], Vec3::new(2.5, 1.0, 0.0), m).is_none());
        // A passenger sharing the thrower's vehicle is skipped like the thrower.
        assert!(thrown_player_at(&players, &[thrower, Who::Mob(2)], beside, m).is_none());
        // The vehicle's box keeps it from leaving: a mount 2 blocks wide.
        let mount = Aabb::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let beside_mount = Vec3::new(1.9, 0.0, 0.0);
        assert!(thrown_left_owner(&[body(0.0)], beside_mount, up));
        assert!(!thrown_left_owner(&[body(0.0), mount], beside_mount, up));
        // No thrower (a dispenser's potion): it has left them already.
        assert!(thrown_left_owner(&[], eye, up));
    }

}

/// `MobEffect.withSoundOnAdded`: the sound `onEffectAdded` plays when the
/// effect is new on an entity (`MobEffects`: the three omens).
pub(super) fn sound_on_added(effect: &str) -> Option<&'static str> {
    match effect.strip_prefix("minecraft:").unwrap_or(effect) {
        "bad_omen" => Some("minecraft:event.mob_effect.bad_omen"),
        "trial_omen" => Some("minecraft:event.mob_effect.trial_omen"),
        "raid_omen" => Some("minecraft:event.mob_effect.raid_omen"),
        _ => None,
    }
}

#[cfg(test)]
mod sound_on_added_tests {
    use horizon_client::effects::EffectInstance;

    /// `LivingEntity.addEffect` → `MobEffect.onEffectAdded`: Bad Omen, Trial
    /// Omen and Raid Omen sound once when they are new on the player, at its
    /// feet; a renewal and any other effect are silent (#11373).
    #[test]
    #[ignore = "daily"]
    fn a_new_omen_sounds_once_at_the_feet() {
        super::super::native_entity_refs::tests::with_game(|game| {
            game.player = Some(
                horizon_client::motion::PlayerMotion::new(
                    game.motion,
                    horizon_core::geometry::Vec3::new(10.5, 64.0, -3.25),
                )
                .unwrap(),
            );
            game.audio.capture = Some(Vec::new());
            for (id, amplifier) in [
                ("bad_omen", 0),
                ("bad_omen", 1),
                ("speed", 0),
                ("trial_omen", 0),
                ("raid_omen", 0),
            ] {
                game.add_player_effect(EffectInstance::new(id, 2400, amplifier));
            }
            let played: Vec<(String, [f64; 3])> = game
                .audio
                .capture
                .take()
                .unwrap()
                .into_iter()
                .map(|s| (s.event, s.position))
                .collect();
            let feet = [10.5, 64.0, -3.25];
            assert_eq!(
                played,
                [
                    ("minecraft:event.mob_effect.bad_omen".to_owned(), feet),
                    ("minecraft:event.mob_effect.trial_omen".to_owned(), feet),
                    ("minecraft:event.mob_effect.raid_omen".to_owned(), feet),
                ]
            );
        });
    }
}

#[cfg(test)]
mod xp_bottle_tests {
    use super::*;

    /// `ThrownExperienceBottle.onHit` uses `awardWithDirection`: every fresh
    /// orb starts 0.25 out from the face it hit, or back along the bottle's
    /// motion after an entity hit, moving that way (#11388).
    #[test]
    #[ignore = "daily"]
    fn bottle_orbs_start_away_from_what_it_hit() {
        super::super::native_entity_refs::tests::with_game(|game| {
            let bottle = ItemStack::new("minecraft:experience_bottle", 1);
            let at = Vec3::new(8.5, 70.5, 8.5);
            // (hit face, the bottle's motion, the axis and sign the orbs leave by)
            for (face, motion, axis, sign) in [
                (
                    Some(([9, 70, 8], Direction::West)),
                    Vec3::new(1.0, 0.0, 0.0),
                    0,
                    -1.0,
                ),
                (None, Vec3::new(0.0, 0.0, 1.0), 2, -1.0),
            ] {
                for o in game.ecs.orb_views() {
                    if let Some(e) = game.ecs.entity_of(o.id) {
                        game.ecs.despawn(e);
                    }
                }
                game.splash(
                    &bottle,
                    at,
                    None,
                    face,
                    0,
                    &PotionOwner::default(),
                    false,
                    motion,
                );
                assert!(!game.ecs.orb_views().is_empty());
                let at = [at.x, at.y, at.z];
                for o in game.ecs.orb_views().iter() {
                    let p = [o.position.x, o.position.y, o.position.z];
                    let v = [o.velocity.x, o.velocity.y, o.velocity.z];
                    assert!((p[axis] - (at[axis] + sign * 0.25)).abs() < 1e-9, "{p:?}");
                    assert!(v[axis] * sign >= 0.0, "{v:?}");
                }
            }
        });
    }
}

#[cfg(test)]
#[path = "potions/potion_contents_tests.rs"]
mod potion_contents_tests;

#[cfg(test)]
mod arrow_contents {
    //! Actual in-memory Game potion paths. No run loop, world load or server startup.
    use super::*;
    use crate::game::{Options, Progress, Settings, execution_role, load_resources};
    use horizon_client::chunk::Chunk;
    use serde_json::{Value, json};
    use std::{path::PathBuf, sync::Arc, time::Instant};

    fn stack(row: &Value) -> ItemStack {
        let mut stack = ItemStack::new("minecraft:tipped_arrow", 1);
        if let Some(potion) = row["potion"].as_str() {
            stack = stack.with_potion(&format!("minecraft:{potion}"));
        }
        let effects = row["custom"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                Tag::compound([
                    (
                        "id",
                        Tag::String(
                            format!("minecraft:{}", e["id"].as_str().unwrap())
                                .as_str()
                                .into(),
                        ),
                    ),
                    ("duration", Tag::Int(e["duration"].as_i64().unwrap() as i32)),
                    ("amplifier", Tag::Int(e["amp"].as_i64().unwrap() as i32)),
                    (
                        "ambient",
                        Tag::Byte(i8::from(e["ambient"].as_bool().unwrap())),
                    ),
                    (
                        "show_particles",
                        Tag::Byte(i8::from(e["visible"].as_bool().unwrap())),
                    ),
                ])
            })
            .collect();
        stack
            .data_mut()
            .potion_extra
            .push(("custom_effects".into(), Tag::List(effects)));
        if let Some(color) = row["color"].as_i64() {
            stack
                .data_mut()
                .potion_extra
                .push(("custom_color".into(), Tag::Int(color as i32)));
        }
        if let Some(name) = row["name"].as_str() {
            stack
                .data_mut()
                .potion_extra
                .push(("custom_name".into(), Tag::String(name.into())));
        }
        stack
    }
    #[test]
    #[ignore = "explicit HORIZON_TEST_RESOURCES and isolated potion oracle input/output"]
    fn actual_arrow_contents_match_pinned_methods() {
        let root =
            PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("explicit resources"));
        let input =
            PathBuf::from(std::env::var_os("HORIZON_POTION_INPUT").expect("explicit input"));
        let output =
            PathBuf::from(std::env::var_os("HORIZON_POTION_OUTPUT").expect("explicit output"));
        let expected: Vec<Value> = serde_json::from_slice(
            &std::fs::read(
                std::env::var_os("HORIZON_POTION_EXPECTED").expect("pinned method output"),
            )
            .unwrap(),
        )
        .unwrap();
        let rows: Vec<Value> = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.game_dir = Some(std::env::current_dir().unwrap());
        options.execution_role = execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        game.world.set(0, 64, 0, stone).unwrap();
        assert_eq!(rows.len(), 5, "all five arrow cases must execute");
        let mut results = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let id = row["id"].clone();
            let mut arrow = Projectile::new(
                ProjectileKind::Arrow {
                    base_damage: 2.,
                    critical: false,
                    effect: None,
                },
                Vec3::new(0.5, 64.5, 0.5),
                None,
            );
            let st = arrow.arrow.as_mut().unwrap();
            st.pickup_item = stack(row);
            st.in_ground = true;
            st.in_ground_time = 598;
            st.last_state = Some(stone);
            arrow.stuck = true;
            let rain = |_| false;
            let scene = game.arrow_scene(&rain);
            horizon_client::arrows::tick(&mut arrow, &scene, &[]);
            let st = arrow.arrow.as_ref().unwrap();
            let item599 = st
                .pickup_item
                .item
                .trim_start_matches("minecraft:")
                .to_owned();
            let color599 = st.potion_color().map_or(-1, |v| v as i32);
            horizon_client::arrows::tick(&mut arrow, &scene, &[]);
            let st = arrow.arrow.as_ref().unwrap();
            results.push(json!({"id":id,"item599":item599,"color599":color599,"item600":st.pickup_item.item.trim_start_matches("minecraft:"),"color600":st.potion_color().map_or(-1,|v|v as i32)}));
            assert_eq!(
                results.last().unwrap(),
                &expected[index],
                "actual arrow tick: {}",
                id
            );
        }
        std::fs::write(output, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
    }
}
