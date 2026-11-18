//! One tick of a cow, as a chain of steps in vanilla's order (`ai::tick_until_consumption` and what
//! it calls, for a cow with no mod, rider, effect or player): the prelude, `baseTick`, `aiStep`
//! (goals, navigation, controls, travel, block effects, pushes, age, love), `finish_tick`.
use super::{event::CowEvent, goals::Cx, view::CowItem};
use crate::{
    breathing::{Breath, breathe, drown},
    health::DamageLevel,
    mobs::{Damage, DamageKind, MobWorld, Who},
};
use horizon_core::{
    geometry::{Aabb, ShapeOp, Vec3, VoxelShape},
    random::RandomSource,
};

impl CowItem<'_, '_> {
    /// `Animal.finalizeSpawnChildFromBreeding`: the experience, only with mob drops on.
    pub fn breeding_experience(&mut self, mob_drops: bool) -> Option<u32> {
        mob_drops.then(|| 1 + self.random.next_int_bound(7).unwrap_or(0) as u32)
    }

    /// The whole tick; false when the cow leaves the level.
    pub fn tick(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, events: &mut Vec<CowEvent>) -> bool {
        let Some(was_on_fire) = self.tick_head(w, level, events) else {
            return false;
        };
        self.ai_step(w, level, events);
        self.effects_from_blocks(w, level, events);
        self.tick_tail(w, level, was_on_fire);
        true
    }

    /// The start of the tick up to `ai_step`: None when the cow leaves the level, else whether it was on fire.
    pub fn tick_head(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, events: &mut Vec<CowEvent>) -> Option<bool> {
        if self.vitals.removed {
            return None;
        }
        if !self.check_despawn(w) {
            self.vitals.removed = true;
            return None;
        }
        if self.vitals.invulnerable_time > 0 {
            self.vitals.invulnerable_time -= 1;
        }
        self.motion.previous = self.motion.position;
        self.motion.previous_yaw = self.motion.yaw;
        self.motion.previous_body_yaw = self.motion.body_yaw;
        self.motion.previous_head_yaw = self.motion.head_yaw;
        self.motion.previous_head_pitch = self.motion.head_pitch;
        self.clk.previous_swell = self.clk.swell;
        self.anim.begin_tick();
        self.clk.age += 1;
        self.ai.seen_cache.clear();
        if self.ai.last_hurt_by_player > 0 {
            self.ai.last_hurt_by_player -= 1;
        }
        let was_on_fire = self.base_tick(w, level, events);
        if self.vitals.impulse_grace_time > 0 {
            self.vitals.impulse_grace_time -= 1;
        }
        if !self.alive() && self.vitals.death_time >= 20 {
            return None;
        }
        Some(was_on_fire)
    }

    /// `Mob.checkDespawn` of a cow before its tick (a player is the only despawn reference).
    fn check_despawn(&mut self, w: &MobWorld) -> bool {
        if w.difficulty.effective == 0.0 && !crate::mobs::spawn_rules::allowed_in_peaceful(self.species()) {
            return false;
        }
        let Some(p) = w.nearest_player else {
            return true;
        };
        let d = self.distance_sqr(p);
        let persistent = self.clk.persistent;
        // An animal of the creature category is never removed when far.
        let removable = !(self.id.kind.category == "creature" && self.id.kind.flags.animal);
        if !persistent && d > 128.0 * 128.0 && removable {
            return false;
        }
        if !persistent
            && self.ai.no_action_time > 600
            && self.random.next_int_bound(800).unwrap_or(1) == 0
            && d > 32.0 * 32.0
            && removable
        {
            return false;
        }
        if d < 32.0 * 32.0 {
            self.ai.no_action_time = 0;
        }
        true
    }

    /// The part of the base tick that reads the blocks: fluids, fire contact, void, wall, air. Returns the
    /// fire contact, whether the cow touches lava, and where they were measured.
    pub fn base_world(
        &mut self,
        w: &MobWorld,
        level: &mut DamageLevel<'_>,
        events: &mut Vec<CowEvent>,
    ) -> (f32, bool, Vec3) {
        self.env.was_eye_in_water = self.fluids.eye_in_water;
        self.update_fluids(w, events);
        let fire = crate::motion::environment::fire_contact(w.world, w.registry, self.bounds());
        let lava = self.fluids.in_lava;
        let contact_at = self.motion.position;
        if fire > 0.0 {
            self.env.ticks_frozen = 0;
        }
        if self.env.fire_ticks > 0 {
            if self.id.kind.flags.fire_immune {
                self.env.fire_ticks = 0;
            } else {
                if self.env.fire_ticks % 20 == 0 && !self.fluids.in_lava {
                    self.damage(level, 1.0, Damage::of(DamageKind::OnFire));
                }
                self.env.fire_ticks -= 1;
            }
        }
        if self.motion.position.y < f64::from(w.world.bottom_y() - 64) {
            self.damage(level, 4.0, Damage::of(DamageKind::OutOfWorld));
        }
        self.env.first_tick = false;
        if self.alive() {
            let mut scene =
                crate::collisions::SceneCollisions { world: w.world, binding: w.collision, subject: None };
            if crate::mobs::ai::in_wall(
                self.width(),
                [self.motion.position.x, self.eye_y(), self.motion.position.z],
                |x, y, z, eye_bounds| {
                    let p = [x, y, z];
                    let state = w.world.block(x, y, z);
                    if !w.suffocation.is_suffocating(&mut scene, state, p).unwrap_or(false) {
                        return false;
                    }
                    w.collision
                        .collision_shape(&mut scene, state, p)
                        .ok()
                        .is_some_and(|shape| {
                            let Ok(eye_shape) = VoxelShape::from_box(*eye_bounds) else {
                                return false;
                            };
                            shape
                                .moved(Vec3::new(f64::from(x), f64::from(y), f64::from(z)))
                                .join_is_not_empty(&eye_shape, ShapeOp::AND)
                                .unwrap_or(false)
                        })
                },
            ) {
                self.damage(level, 1.0, Damage::of(DamageKind::InWall));
            }
            let submerged = self.fluids.eye_in_water
                && w.block_name(
                    self.motion.position.x.floor() as i32,
                    self.eye_y().floor() as i32,
                    self.motion.position.z.floor() as i32,
                ) != "minecraft:bubble_column";
            let drowns = submerged && !self.id.kind.flags.breathes_water;
            let max = 300;
            let vanilla = Breath {
                can_breathe: !drowns,
                consume: 1,
                refill: if drowns { 0 } else { 4 },
            };
            let breath = breathe(w.loop_context, self.id.id, &self.id.kind.id, vanilla);
            if let Some(drowning) = drown(
                w.loop_context,
                self.id.id,
                &self.id.kind.id,
                &mut self.env.air,
                max,
                breath,
                true,
            ) && let Some(damage) = drowning.damage
            {
                self.damage(level, damage, Damage::of(DamageKind::Drown));
            }
        }
        (fire, lava, contact_at)
    }

    /// `ai::base_tick`: fluids, fire, wall, air, hurt timers, burning and the ambient sound.
    fn base_tick(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, events: &mut Vec<CowEvent>) -> bool {
        let (fire, lava, contact_at) = self.base_world_or_rest(w, level, events);
        // `hurt_by::base_tick`
        if self.vitals.hurt_time > 0 {
            self.vitals.hurt_time -= 1;
        }
        if self.vitals.invulnerable > 0 {
            self.vitals.invulnerable -= 1;
        }
        if let Some(who) = self.ai.last_hurt_by
            && let Some(by) = w.target_info(who)
            && (!by.alive || (self.clk.age as i32).wrapping_sub(self.ai.last_hurt_by_time) > 100)
        {
            let age = self.clk.age as i32;
            self.ai.forget_hurt_by(age);
        }
        let was_on_fire = self.env.fire_ticks > 0;
        let (fire, lava) = if self.motion.position == contact_at {
            (fire, lava)
        } else {
            (
                crate::motion::environment::fire_contact(w.world, w.registry, self.bounds()),
                self.touches_lava(w),
            )
        };
        self.block_contact_damage(level, fire, lava);
        if !self.alive() {
            self.vitals.death_time += 1;
        }
        if self.alive() {
            let roll = self.random.next_int_bound(1000).unwrap_or(999);
            let t = self.clk.ambient_timer;
            self.clk.ambient_timer += 1;
            if roll < t {
                self.clk.ambient_timer = -self.ambient_interval();
                self.voice();
            }
        }
        was_on_fire
    }

    fn touches_lava(&self, w: &MobWorld) -> bool {
        let b = self.bounds().deflate(Vec3::new(1e-5, 1e-5, 1e-5));
        let (min, max) = (b.min_position(), b.max_position());
        for x in min.x.floor() as i32..=max.x.floor() as i32 {
            for y in min.y.floor() as i32..=max.y.floor() as i32 {
                for z in min.z.floor() as i32..=max.z.floor() as i32 {
                    if &*w.facts.get(w.world.block(x, y, z)).name == "minecraft:lava" {
                        return true;
                    }
                }
            }
        }
        false
    }
    fn block_contact_damage(&mut self, level: &mut DamageLevel<'_>, fire: f32, lava: bool) {
        if fire > 0.0 && !self.id.kind.flags.fire_immune {
            let multiplier = self.burning_time();
            self.env.fire_ticks =
                crate::motion::environment::ignite_contact(self.env.fire_ticks, false, &mut self.random.0, multiplier);
            self.damage(level, fire, Damage::of(DamageKind::InFire));
        }
        if lava && !self.id.kind.flags.fire_immune {
            self.ignite_for_ticks(300);
            self.damage(level, 4.0, Damage::of(DamageKind::Lava));
        }
    }

    /// `LivingEntity.aiStep` of a cow.
    pub fn ai_step(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, events: &mut Vec<CowEvent>) {
        if self.ai.no_jump_delay > 0 {
            self.ai.no_jump_delay -= 1;
        }
        self.drop_tiny_motion();
        self.ai.xxa *= 0.98;
        self.ai.zza *= 0.98;
        if self.alive() {
            self.server_ai_step(w, level, events);
        } else {
            self.ai.jumping = false;
            self.ai.xxa = 0.0;
            self.ai.zza = 0.0;
        }
        self.handle_jumping(w);
        let input = Vec3::new(f64::from(self.ai.xxa), f64::from(self.ai.yya), f64::from(self.ai.zza));
        self.travel_or_rest(w, level, input, events);
        self.apply_effects_from_blocks(w);
        self.push_entities(w, level, events);
        self.ageable_step(w);
        self.animal_step();
    }
    fn drop_tiny_motion(&mut self) {
        let v = self.motion.velocity;
        self.motion.velocity = Vec3::new(
            if v.x.abs() < 0.003 { 0.0 } else { v.x },
            if v.y.abs() < 0.003 { 0.0 } else { v.y },
            if v.z.abs() < 0.003 { 0.0 } else { v.z },
        );
    }

    /// `Mob.serverAiStep`.
    fn server_ai_step(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, events: &mut Vec<CowEvent>) {
        self.ai.no_action_time = self.ai.no_action_time.wrapping_add(1);
        let tick_count = i64::from(self.clk.age);
        let id_based = tick_count + i64::from(self.id.id);
        {
            let mut goals = std::mem::take(&mut self.goals.0);
            let mut cx = Cx { w, level, events };
            if id_based % 2 != 0 && tick_count > 1 {
                goals.tick_running(self, &mut cx, false);
            } else {
                goals.tick(self, &mut cx);
            }
            self.goals.0 = goals;
        }
        self.nav_tick(w);
        self.move_control_tick(w);
        self.look_control_tick();
        self.jump_control_tick();
    }

    /// `LivingEntity.pushEntities`: the cows overlapping this one are pushed apart.
    fn push_entities(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, events: &mut Vec<CowEvent>) {
        self.motion.on_climbable = self.on_climbable(w);
        let bounds = self.bounds();
        let self_pushable = self.pushable();
        let mut targets = crate::mobs::world::take_push_targets();
        for o in w.others_near(bounds) {
            if o.id != self.id.id && o.alive && o.pushable && o.bounds.intersects(bounds) {
                targets.push((Who::Mob(o.id), o.position, o.passenger, false, o.is_vehicle));
            }
        }
        let own = w.teams.of(Who::Mob(self.id.id));
        targets.retain(|(who, ..)| crate::scoreboard::pushable_by(own, w.teams.of(*who), true));
        let limit = w.max_entity_cramming;
        if limit > 0
            && targets.len() as i64 > i64::from(limit) - 1
            && self.random.next_int_bound(4).unwrap_or(1) == 0
            && targets.iter().filter(|t| !t.2).count() as i64 > i64::from(limit) - 1
        {
            self.damage(level, 6.0, Damage::of(DamageKind::Cramming));
        }
        for &(who, at, _, same, vehicle) in &targets {
            let sleeping = match who {
                Who::Player => false,
                Who::Mob(id) => w.seen(id).is_some_and(|o| o.sleeping),
            };
            if sleeping || same {
                continue;
            }
            let (mut xa, mut za) = (self.motion.position.x - at.x, self.motion.position.z - at.z);
            let mut dd = xa.abs().max(za.abs());
            if dd < f64::from(0.01_f32) {
                continue;
            }
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let pow = (1.0 / dd).min(1.0);
            xa *= pow;
            za *= pow;
            xa *= f64::from(0.05_f32);
            za *= f64::from(0.05_f32);
            if !vehicle {
                events.push(CowEvent::Push { target: who, velocity: [-xa, 0.0, -za] });
            }
            if self_pushable {
                let v = self.motion.velocity;
                self.motion.velocity = Vec3::new(v.x + xa, v.y, v.z + za);
            }
        }
        crate::mobs::world::give_push_targets(targets);
    }

    /// `AgeableMob.aiStep` of a cow: a baby grows, a breeding pause counts down.
    fn ageable_step(&mut self, w: &MobWorld) {
        if self.extra.forced_age_timer > 0 {
            if self.extra.forced_age_timer % 4 == 0 {
                let (width, height) = (f64::from(self.width()), f64::from(self.height()));
                let r = &mut self.random;
                let _ = (r.next_double(), r.next_double(), r.next_double());
                let _ = (width, height);
            }
            self.extra.forced_age_timer -= 1;
        }
        if self.alive() {
            if self.is_baby() && !self.extra.age_locked {
                let (bw, bh, _) = self.dims();
                self.extra.age += 1;
                if self.extra.age >= 0 {
                    self.fudge_position_after_size_change(w, bw, bh);
                }
            } else if self.extra.age > 0 {
                self.extra.age -= 1;
            }
        }
    }
    /// `Animal.aiStep`: age ends love, love counts down (the heart's draws are made).
    fn animal_step(&mut self) {
        if self.alive() {
            if self.extra.age != 0 {
                self.extra.in_love = 0;
            }
            if self.extra.in_love > 0 {
                self.extra.in_love -= 1;
                if self.extra.in_love % 10 == 0 {
                    let (w, h) = (f64::from(self.width()), f64::from(self.height()));
                    let r = &mut self.random;
                    let _velocity = [r.next_gaussian() * 0.02, r.next_gaussian() * 0.02, r.next_gaussian() * 0.02];
                    let _x = self.motion.position.x + w * (2.0 * r.next_double() - 1.0);
                    let _y = self.motion.position.y + h * r.next_double() + 0.5;
                    let _z = self.motion.position.z + w * (2.0 * r.next_double() - 1.0);
                }
            }
        }
    }

    /// `Entity.refreshDimensions` after growing: `fudgePositionAfterSizeChange`.
    fn fudge_position_after_size_change(&mut self, w: &MobWorld, old_w: f64, old_h: f64) {
        let (new_w, new_h, _) = self.dims();
        let small = new_w <= 4.0 && new_h <= 4.0;
        if self.clk.age == 0 || !small || !(new_w > old_w || new_h > old_h) {
            return;
        }
        let p = self.motion.position;
        let center = Vec3::new(p.x, p.y + old_h / 2.0, p.z);
        let dw = (new_w - old_w).max(0.0) + 1.0e-6;
        let dh = (new_h - old_h).max(0.0) + 1.0e-6;
        let at = |allowed: Aabb, size_y: f64| crate::mobs::living::free_position(w, allowed, center, new_w, size_y);
        if let Some(q) = at(Aabb::of_size(center, Vec3::new(dw, dh, dw)), new_h) {
            self.motion.position = Vec3::new(q.x, q.y - new_h / 2.0, q.z);
            return;
        }
        if new_w > old_w
            && new_h > old_h
            && let Some(q) = at(Aabb::of_size(center, Vec3::new(dw, 1.0e-6, dw)), old_h)
        {
            self.motion.position = Vec3::new(q.x, q.y - old_h / 2.0 + 1.0e-6, q.z);
        }
    }

    /// The end of the tick after the effects of the blocks.
    pub fn tick_tail(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, was_on_fire: bool) {
        if self.alive() {
            self.freeze_tick(w, level);
        }
        self.extinguish_if_wet(w, was_on_fire);
        self.env.fire_ticks = self.env.fire_ticks.max(0);
        if self.alive() {
            self.body_rotation_tick();
        }
        self.update_walk_animation();
        let swinging = self.ai.swinging;
        self.anim.end_tick(swinging);
        self.tick_swing();
        self.flower_contact_effects(w);
    }
    fn tick_swing(&mut self) {
        let duration = if self.ai.swing_duration > 0 { self.ai.swing_duration } else { 6 };
        if self.ai.swinging {
            self.ai.swing_time += 1;
            if self.ai.swing_time >= duration {
                self.ai.swing_time = 0;
                self.ai.swinging = false;
                self.ai.swing_duration = 0;
            }
        } else {
            self.ai.swing_time = 0;
        }
        self.anim.attack_anim = self.ai.swing_time as f32 / duration as f32;
    }
    fn freeze_tick(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>) {
        let in_snow = self.in_powder_snow(w);
        self.env.in_powder_snow = in_snow;
        if !in_snow && self.env.ticks_frozen == 0 {
            return;
        }
        let can_freeze = !w.data.entity_has("minecraft:freeze_immune_entity_types", &self.id.kind.id);
        if in_snow && can_freeze {
            self.env.ticks_frozen =
                (self.env.ticks_frozen + 1).min(crate::motion::PlayerMotion::TICKS_REQUIRED_TO_FREEZE);
        } else {
            self.env.ticks_frozen = (self.env.ticks_frozen - 2).max(0);
        }
        if self.clk.age.is_multiple_of(40) && self.is_fully_frozen() && can_freeze {
            self.damage(level, 1.0, Damage::of(DamageKind::Freeze));
        }
    }
    fn is_fully_frozen(&self) -> bool {
        self.env.ticks_frozen >= crate::motion::PlayerMotion::TICKS_REQUIRED_TO_FREEZE
    }
    fn in_powder_snow(&self, w: &MobWorld) -> bool {
        let b = self.bounds().deflate(Vec3::new(1e-5, 1e-5, 1e-5));
        let (min, max) = (b.min_position(), b.max_position());
        for x in min.x.floor() as i32..=max.x.floor() as i32 {
            for y in min.y.floor() as i32..=max.y.floor() as i32 {
                for z in min.z.floor() as i32..=max.z.floor() as i32 {
                    if &*w.facts.get(w.world.block(x, y, z)).name == "minecraft:powder_snow" {
                        return true;
                    }
                }
            }
        }
        false
    }
    fn extinguish_if_wet(&mut self, w: &MobWorld, was_on_fire: bool) {
        if self.env.fire_ticks > 0 && (self.fluids.in_water || self.in_rain(w)) {
            self.env.fire_ticks = 0;
            if was_on_fire {
                let _pitch = 1.6 + (self.random.next_float() - self.random.next_float()) * 0.4;
            }
        }
    }
    fn in_rain(&self, w: &MobWorld) -> bool {
        let (x, y, z) = (
            self.motion.position.x.floor() as i32,
            self.motion.position.y.floor() as i32,
            self.motion.position.z.floor() as i32,
        );
        let top = (self.motion.position.y + f64::from(self.height())).floor() as i32;
        w.raining_at(x, y, z) || w.raining_at(x, top, z)
    }
    /// `Entity.applyEffectsFromBlocks` for the flowers: the walk over the blocks of the move. A cow
    /// takes nothing from a wither rose here (no effects), so only the sweep is made.
    fn flower_contact_effects(&mut self, w: &MobWorld) {
        if !self.alive() {
            return;
        }
        let inset = f64::from(1.0e-5_f32);
        let box_at_end = self.bounds_at(self.motion.position).deflate(Vec3::new(inset, inset, inset));
        let alive = self.alive();
        let _ = horizon_core::geometry::sweep_intersected_blocks(
            self.motion.previous,
            self.motion.position,
            box_at_end,
            1 << 22,
            |p, _| {
                let _ = w.block_name(p.x, p.y, p.z);
                alive
            },
        );
    }
}
