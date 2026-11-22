//! Spears (`KineticWeapon`): holding the use button charges the spear, and
//! from `delay_ticks` on every living entity along the view that the
//! player's speed allows is stabbed: the damage grows with the speed
//! relative to the target, the knockback and the dismount need the
//! attacker's own speed, and each condition runs out after its time
//! (`Item.use`, `ItemStack.onUseTick`, `KineticWeapon.damageEntities`,
//! `Player.stabAttack`). The hit's feedback (`onKineticHit`) reaches the
//! players around as entity event 2. A left click jabs instead
//! (`PiercingWeapon.attack`), which Lunge turns into a dash.
use super::{Game, view_vector};
pub(super) use horizon_client::spear::hits_in_reach;
use horizon_client::{attack_strength::delay, collisions::SceneCollisions, items::ItemStack, mobs::Damage, spear::{HIT_FEEDBACK_TICKS, Kinetic, REACH}};
use mod_api::client::first_person::Hand;
use crate::hand_use::SwingAnimation;
use horizon_core::{
    geometry::{Aabb, Vec3, traverse_blocks},
    position::BlockPos,
};

/// The key of this window's own player in `Game::kinetic_feedback`.
pub(super) const OWN: i32 = i32::MIN;

/// What a stab reaches along the view: a mob (its index in `mobs`) or
/// another player (its index in `stab_players`).
#[derive(Clone, Copy)]
enum Target {
    /// An ECS mob, by its network id.
    Ecs(u32),
    Player(usize),
}

impl Game<'_> {
    /// `Item.use` of a kinetic weapon: `startUsingItem` with a new
    /// `recentKineticEnemies`, and `KineticWeapon.makeSound` at the user.
    pub(super) fn use_spear(&mut self, stack: &ItemStack) -> bool {
        let Some(kinetic) = Kinetic::of(&stack.item) else {
            return false;
        };
        if self.start_using_item() {
            let actor = self.lan_acting;
            self.stabbed.retain(|(who, _), _| *who != actor);
            if let Some(p) = &self.player {
                let at = [p.position.x, p.position.y, p.position.z];
                self.audio.play(kinetic.sound, Some(at), 1.0, 1.0);
            }
        }
        true
    }

    /// `ServerPlayer.getKnownMovement` of the acting player: what they moved
    /// in the last tick. A guest's comes with their moves. A rider's is its
    /// mount's move (`getKnownSpeed`: the vehicle's, or the move the driver's
    /// client sent for it), so a charge on horseback carries the horse's speed.
    pub(super) fn known_player_movement(&self) -> Vec3 {
        if let Some(v) = self.riding {
            return self.mob_speed_ecs(v).scale(1.0 / 20.0);
        }
        if self.lan_acting.is_some() {
            return self.known_movement;
        }
        self.player
            .as_ref()
            .map_or(Vec3::ZERO, |p| p.position.subtract(self.previous_position))
    }

    /// `mob_speed` for the ECS mob `id`: the speed of its root vehicle, from its velocity component.
    fn mob_speed_ecs(&self, id: u32) -> Vec3 {
        let mut root = id;
        while let Some(v) = self.ecs_vehicle_of(root) {
            root = v;
        }
        self.ecs_mob_velocity(root).unwrap_or(Vec3::ZERO)
    }

    /// `Level.clip` with COLLIDER shapes and no fluids: where the segment
    /// first meets a block, if it does (tall grass lets a stab through).
    fn clip_collider(&self, from: Vec3, to: Vec3) -> Option<Vec3> {
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        traverse_blocks(from, to, 512, |p: BlockPos| {
            let state = self.world.block(p.x, p.y, p.z);
            if self.res.facts().is_air(state) {
                return None;
            }
            let shape = self
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?;
            shape.clip(from, to, p).map(|hit| hit.location)
        })
        .ok()
        .flatten()
    }

    /// `ProjectileUtil.getHitEntitiesAlong` with the spear's `AttackRange`:
    /// the mobs and the other players along the view, nearest first.
    fn stab_targets(&self, eye: Vec3, look: Vec3, movement: Vec3) -> Vec<Target> {
        // AttackRange.effectiveMinRange and effectiveMaxRange for a player.
        let (min, max) = if self.creative {
            (REACH.min_creative, REACH.max_creative)
        } else {
            (REACH.min, REACH.max)
        };
        let reach = (min, max, REACH.hitbox_margin);
        let clip = |from, to| self.clip_collider(from, to);
        let ids = self.ecs_stab_ids(eye, f64::from(reach.1));
        let players = ids.len();
        let mut boxes: Vec<(usize, Aabb)> =
            ids.iter().enumerate().filter_map(|(k, &id)| Some((k, self.mob_box(id)?))).collect();
        boxes.extend((self.stab_players.iter().enumerate()).map(|(k, p)| (players + k, p.bounds)));
        hits_in_reach(&boxes, eye, look, movement, reach, clip)
            .into_iter()
            .map(|(i, _)| if i < players { Target::Ecs(ids[i]) } else { Target::Player(i - players) })
            .collect()
    }

    /// The ECS mobs a spear may reach from `eye` within `max`: alive, and not the vehicle ridden, from the spatial index.
    fn ecs_stab_ids(&self, eye: Vec3, max: f64) -> Vec<u32> {
        if !self.ecs.is_active() {
            return Vec::new();
        }
        self.ecs
            .ids_near([eye.x, eye.y, eye.z], max + 8.0)
            .into_iter()
            .filter(|&id| self.riding != Some(id) && self.mob_alive(id))
            .collect()
    }

    /// `KineticWeapon.damageEntities`: one tick of the spear in use, `ticks`
    /// ticks into the use.
    pub(super) fn spear_tick(&mut self, stack: &ItemStack, ticks: u32) {
        let Some(kinetic) = Kinetic::of(&stack.item) else {
            return;
        };
        let used = ticks as i32 - kinetic.delay_ticks;
        let (Some(eye), Some(_)) = (
            self.eye_position(),
            self.player.as_ref().map(|p| p.position),
        ) else {
            return;
        };
        if used < 0 || self.spectator() {
            return;
        }
        let eye = Vec3::new(eye[0], eye[1], eye[2]);
        let look = view_vector(self.yaw, self.pitch);
        let look = Vec3::new(look[0], look[1], look[2]);
        let movement = self.known_player_movement();
        let attacker_speed = look.dot(movement.scale(20.0));
        let now = self.world.game_time;
        let actor = self.lan_acting;
        let mut affected = false;
        let mut on_players = Vec::new();
        for target in self.stab_targets(eye, look, movement) {
            let (entity, target_motion) = match target {
                Target::Ecs(id) => (id, self.mob_speed_ecs(id)),
                Target::Player(k) => {
                    let p = &self.stab_players[k];
                    (p.entity_id as u32, p.movement.scale(20.0))
                }
            };
            let key = (actor, entity);
            let recent = self
                .stabbed
                .get(&key)
                .is_some_and(|&t| now - t < i64::from(kinetic.contact_cooldown_ticks));
            if recent {
                continue;
            }
            self.stabbed.insert(key, now);
            let target_speed = look.dot(target_motion);
            let relative = (attacker_speed - target_speed).max(0.0);
            // An entity factor of 1 for a player.
            let dismounts = kinetic.dismount.test(used, attacker_speed, relative, 1.0);
            let pushes = kinetic.knockback.test(used, attacker_speed, relative, 1.0);
            let damages = kinetic.damage.test(used, attacker_speed, relative, 1.0);
            if dismounts || pushes || damages {
                // The attack damage attribute's base value (1) and the speed.
                let amount = 1.0 + (relative * f64::from(kinetic.damage_multiplier)).floor() as f32;
                match target {
                    Target::Ecs(id) => {
                        affected |= self.stab_mob_ecs(id, amount, damages, pushes, dismounts)
                    }
                    Target::Player(k) => on_players
                        .extend(self.queue_player_stab(k, amount, damages, pushes, dismounts)),
                }
            }
        }
        if affected {
            self.spear_hit_feedback(&kinetic);
        }
        self.keep_player_stabs(kinetic, true, affected, on_players);
    }

    /// What follows a charge tick that affected an entity:
    /// `Level.broadcastEntityEvent(player, 2)`, felt by the player too, and
    /// SPEAR_MOBS_TRIGGER with the living entities stabbed in this use.
    pub(super) fn spear_hit_feedback(&mut self, kinetic: &Kinetic) {
        let Some(feet) = self.player.as_ref().map(|p| p.position) else {
            return;
        };
        let actor = self.lan_acting;
        if self.audio.capture.is_some() {
            self.lan_kinetic_hits.push(actor);
        }
        let who = actor.map_or(OWN, |conn| conn as i32);
        self.kinetic_hit_feedback(who, kinetic, [feet.x, feet.y, feet.z]);
        let stabbed = self.stabbed.keys().filter(|(who, _)| *who == actor).count();
        self.trigger_spear_mobs(stabbed);
    }

    /// `Player.stabAttack` on the ECS mob `id`: the damage and the knock-back are asked of the bridge (its events come back next tick);
    /// the sounds, particles, statistics and wear stay here. Returns whether it was affected.
    fn stab_mob_ecs(&mut self, id: u32, base_damage: f32, deals_damage: bool, deals_knockback: bool, dismounts: bool) -> bool {
        let Some(kind) = self.ecs.kind(id) else { return false };
        let kind = format!("minecraft:{}", kind.trim_start_matches("minecraft:"));
        let Some(attacker) = self.player.as_ref().map(|p| p.entity_id) else {
            return false;
        };
        let magic_boost = self.enchant_damage_bonus(&kind, Some(id));
        let (base_damage, magic_boost) = if self.using.is_none() {
            let scale = self.attack_strength_scale();
            (base_damage * (0.2 + scale * scale * 0.8), magic_boost * scale)
        } else {
            (base_damage, magic_boost)
        };
        let total = if deals_damage { base_damage + magic_boost } else { 0.0 };
        if deals_damage {
            self.ecs_hurt(id, total, horizon_ecs_proto::bridge::host::HurtSource::Player(attacker as u32));
        }
        if deals_knockback {
            let (xd, zd) = self.extra_knockback_direction();
            self.ecs.knockback(id, 0.4, xd, zd);
            let (strength, _) = self.extra_knockback(&kind, 0.0);
            if strength > 0.0 {
                self.ecs.knockback(id, f64::from(strength), xd, zd);
            }
            self.stop_sprint_after_extra_knockback();
        }
        let dismounted = dismounts && self.ecs_vehicle_of(id).is_some();
        if dismounted {
            self.ecs_dismount(id);
        }
        if !deals_damage && !deals_knockback && !dismounted {
            return false;
        }
        let (Some(feet), Some(bounds)) = (self.ecs.position(id), self.mob_box(id)) else { return false };
        let (width, height) = (bounds.max_position().x - bounds.min_position().x, bounds.max_position().y - bounds.min_position().y);
        if magic_boost > 0.0 {
            self.critical_hit(self.lan_acting, id as i32, feet, width, height, true);
        }
        self.combat.last_hurt_mob = Some((id, self.world.game_time));
        if let Some(stack) = self.held().cloned() {
            self.award_stat("used", &stack.item, 1);
            if deals_damage {
                self.post_attack_ecs(id);
            }
            self.weapon_wear(&stack);
        }
        // The health lost is known next tick: the damage asked is what the statistic counts here.
        self.damage_stats_and_hearts(total, feet, height);
        if deals_damage {
            self.remember_mob_player_credit(id);
        }
        if let Some(p) = self.player.as_mut()
            && !self.creative
        {
            p.cause_food_exhaustion(0.1);
        }
        true
    }

    /// `Player.damageStatsAndHearts`: the damage dealt statistic, and the
    /// damage indicator particles over a target that lost more than 2 health
    /// (`feet` and `height`: the target's).
    pub(super) fn damage_stats_and_hearts(&mut self, actual: f32, feet: [f64; 3], height: f64) {
        self.award_custom_stat("damage_dealt", (actual * 10.0).round() as i32);
        if actual > 2.0 {
            let indicator = horizon_client::particles::ParticleOptions::simple(
                horizon_client::particles::ParticleType::DamageIndicator,
            );
            // The players a server serves are sent them too (the window of the server thread).
            self.lan_send_particles(super::lan_particles::QueuedParticle {
                dimension: self.dim.id(),
                options: indicator,
                state: None,
                at: [feet[0], feet[1] + height * 0.5, feet[2]],
                spread: [0.1, 0.0, 0.1],
                speed: [0.2; 3],
                count: (actual * 0.5) as u32 as i32,
                randomization: 0,
            });
        }
    }

    /// `Player.cannotAttackWithItem` for the held item: a spear needs its
    /// `minimum_attack_charge` of 1, counting `tolerance` ticks more than the
    /// `ticker` ticks charged.
    pub(super) fn cannot_attack_with_item(&self, ticker: i32, tolerance: i32) -> bool {
        self.held().is_some_and(|s| Kinetic::of(&s.item).is_some())
            && (ticker + tolerance) as f32 / delay(self.main_hand_attack_speed()) < 1.0
    }

    /// `PiercingWeapon.attack` of the spear in the main hand, for a left
    /// click: every mob along the view is stabbed, then `postPiercingAttack`
    /// (Lunge), the hit sound and the swing. Gives the sound of the jab
    /// (`makeSound`), which everyone but the user hears from the server.
    pub(super) fn jab(&mut self) -> Option<&'static str> {
        let kinetic = self.held().and_then(|s| Kinetic::of(&s.item))?;
        let eye = self.eye_position()?;
        let eye = Vec3::new(eye[0], eye[1], eye[2]);
        let look = view_vector(self.yaw, self.pitch);
        let look = Vec3::new(look[0], look[1], look[2]);
        // The attack damage attribute.
        let damage = self.attack_damage();
        let movement = self.known_player_movement();
        let mut hit = false;
        let mut on_players = Vec::new();
        for target in self.stab_targets(eye, look, movement) {
            match target {
                Target::Ecs(id) => hit |= self.stab_mob_ecs(id, damage, true, true, false),
                Target::Player(k) => {
                    on_players.extend(self.queue_player_stab(k, damage, true, true, false))
                }
            }
        }
        // Player.onAttack, then postPiercingAttack.
        self.attack_ticks = 0;
        self.post_piercing_attack();
        if hit {
            self.play_at_feet(kinetic.hit_sound);
        }
        self.keep_player_stabs(kinetic, false, hit, on_players);
        // swingAndResetAttackStrength (ServerPlayer): a swing that starts
        // resets the attack strength.
        let swings = self.swings_started;
        let a = kinetic.attack_animation();
        self.swing_hand(Hand::Main, SwingAnimation { kind: a.kind, duration: a.duration });
        if self.swings_started != swings {
            self.reset_attack_strength();
        }
        Some(kinetic.attack_sound)
    }

    /// A sound at the player's feet.
    pub(super) fn play_at_feet(&mut self, sound: &str) {
        if let Some(p) = &self.player {
            let at = [p.position.x, p.position.y, p.position.z];
            self.audio.play(sound, Some(at), 1.0, 1.0);
        }
    }

    /// `LivingEntity.onKineticHit` for `entity`: the hit feels (the spear
    /// moves back, `kinetic_hit_ticks`), at most once every ten
    /// ticks.
    pub(super) fn kinetic_hit_feedback(&mut self, entity: i32, _kinetic: &Kinetic, _at: [f64; 3]) {
        let now = self.world.game_time;
        if self
            .kinetic_feedback
            .get(&entity)
            .is_none_or(|&t| now - t > HIT_FEEDBACK_TICKS)
        {
            self.kinetic_feedback.insert(entity, now);
        }
    }

    /// `LivingEntity.handleEntityEvent(2)` on a server's client: the
    /// player `id` (this one, or another) was hit by the spear it uses.
    pub(super) fn remote_kinetic_hit(&mut self, id: i32) {
        let mob = id as u32;
        let item = self
            .ecs_using(mob)
            .and_then(|(off_hand, _)| self.ecs_hand_stack(mob, off_hand));
        let at = self.ecs_vec3(mob);
        if let (Some(kinetic), Some(at)) = (item.as_ref().and_then(|s| Kinetic::of(&s.item)), at) {
            self.kinetic_hit_feedback(id, &kinetic, at);
        }
    }


}
