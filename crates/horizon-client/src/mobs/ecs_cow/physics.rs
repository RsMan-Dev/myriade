//! `LivingEntity` movement of a cow on components: travel in air and fluid, `Entity.move` and its
//! collisions, restitution, fall damage, fluid interaction and jumping. Ported from `mobs/living.rs`
//! with the species that are not a cow, the mods' hooks, effects, riders and vehicles left out; the
//! kernels that do not take a `Mob` (collision scan, fluids, friction tables) are the existing ones.
use super::{event::CowEvent, view::CowItem};
use crate::{
    blocks::Fluid,
    collisions::SceneCollisions,
    health::DamageLevel,
    mobs::{Damage, DamageKind, MobWorld, living::Fluids},
};
use horizon_core::{
    geometry::{Aabb, Vec3, trig},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::{
    block_landing::LandingEntityKind,
    entity_move::MoverType,
    move_gate::MoveGate,
    movement_restitution::{RestitutionInput, RestitutionWorld, restitute_movement},
    registry::BlockStateId,
};

impl CowItem<'_, '_> {
    pub fn fluid_jump_threshold(&self) -> f64 {
        if self.eye_height() < 0.4 { 0.0 } else { 0.4 }
    }
    pub fn is_in_liquid(&self) -> bool {
        self.fluids.in_water || self.fluids.in_lava
    }
    /// `FloatGoal.canUse`: water deeper than the jump threshold, or lava.
    pub fn floats(&self) -> bool {
        self.fluids.water > self.fluid_jump_threshold() || self.fluids.in_lava
    }
    /// `Entity.moveRelative`.
    pub fn move_relative(&mut self, speed: f32, input: Vec3) {
        let len_sq = input.x * input.x + input.y * input.y + input.z * input.z;
        if len_sq < 1.0e-7 {
            let v = self.motion.velocity;
            self.motion.velocity = Vec3::new(v.x + 0.0, v.y + 0.0, v.z + 0.0);
            return;
        }
        let v = if len_sq > 1.0 {
            let l = len_sq.sqrt();
            if l < f64::from(1.0e-5_f32) {
                Vec3::ZERO
            } else {
                Vec3::new(input.x / l, input.y / l, input.z / l)
            }
        } else {
            input
        };
        let v = Vec3::new(v.x * f64::from(speed), v.y * f64::from(speed), v.z * f64::from(speed));
        let angle = f64::from(self.motion.yaw * trig::DEG_TO_RAD);
        let (s, c) = (f64::from(trig::sin(angle)), f64::from(trig::cos(angle)));
        let u = self.motion.velocity;
        self.motion.velocity = Vec3::new(u.x + (v.x * c - v.z * s), u.y + v.y, u.z + (v.z * c + v.x * s));
    }
    /// The boxes of the mobs a cow cannot pass through (none in a world of cows: a cow blocks nobody).
    fn entity_collisions(&self, w: &MobWorld, movement: Vec3) -> Vec<Aabb> {
        let area = self.bounds().expand_towards(movement);
        if area.average_size() < 1.0e-7 {
            return Vec::new();
        }
        let area = area.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        let mover = self.species();
        let me = self.id.id;
        let blocks = |s: &&crate::mobs::Seen| {
            s.id != me && s.blocks_mob(mover) && s.bounds.intersects(area) && s.vehicle != Some(me)
        };
        match w.others_solid {
            Some(slots) => slots
                .iter()
                .map(|&i| &w.others[i as usize])
                .filter(blocks)
                .map(|s| s.bounds)
                .collect(),
            None => w.others.iter().filter(blocks).map(|s| s.bounds).collect(),
        }
    }
    fn collision_subject(&self) -> crate::collisions::PowderSnowSubject<'_> {
        use horizon_world::collision_context::CollisionItem;
        crate::collisions::PowderSnowSubject {
            fall_distance: self.motion.fall_distance,
            walkable_type: self.id.kind.flags.powder_snow_walkable,
            leather_boots: false,
            main_hand: Some(CollisionItem::EMPTY),
            lava_walker: self.id.kind.flags.lava_walker,
            ..Default::default()
        }
    }
    /// `Entity.move` with block collisions and step-up; sets the collision flags and the fall.
    pub fn do_move(
        &mut self,
        w: &MobWorld,
        level: &mut DamageLevel<'_>,
        events: &mut Vec<CowEvent>,
        gate: &mut MoveGate,
    ) {
        crate::move_gate::arm(
            gate,
            w.loop_context,
            self.id.id,
            &self.id.kind.id,
            MoverType::SelfMovement,
            self.motion.velocity,
        );
        if gate.skipped() {
            return;
        }
        let mut wanted = gate.delta();
        if self.motion.no_physics {
            self.motion.position = self.motion.position.add_vector(wanted);
            self.motion.horizontal_collision = false;
            self.motion.vertical_collision_below = false;
            return;
        }
        if self.motion.stuck_multiplier.length_squared() > 1.0e-7 {
            wanted = wanted.multiply(self.motion.stuck_multiplier);
            self.motion.stuck_multiplier = Vec3::ZERO;
            self.motion.velocity = Vec3::ZERO;
        }
        let step = self.id.kind.step_height as f32;
        let scene = SceneCollisions {
            world: w.world,
            binding: w.collision,
            subject: Some(self.collision_subject()),
        };
        let border = w.border.map(|b| (b, [self.motion.position.x, self.motion.position.z]));
        let entities = self.entity_collisions(w, wanted);
        let moved = crate::move_gate::scene_resolve_in_border_with(
            gate,
            scene,
            self.bounds(),
            wanted,
            step,
            self.motion.on_ground,
            border,
            &entities,
        )
        .unwrap_or(Vec3::ZERO);
        let length = moved.x * moved.x + moved.y * moved.y + moved.z * moved.z;
        let wanted_length = wanted.x * wanted.x + wanted.y * wanted.y + wanted.z * wanted.z;
        if length > 1.0e-7 || wanted_length - length < 1.0e-7 {
            if self.motion.fall_distance != 0.0
                && length >= 1.0
                && crate::mobs::living::fall_reset_hit(w, self.motion.position, moved)
            {
                self.motion.fall_distance = 0.0;
            }
            let p = self.motion.position;
            self.motion.position = Vec3::new(p.x + moved.x, p.y + moved.y, p.z + moved.z);
        }
        let differs = |a: f64, b: f64| (b - a).abs() >= f64::from(1.0e-5_f32);
        let x_hit = differs(wanted.x, moved.x);
        let z_hit = differs(wanted.z, moved.z);
        self.motion.horizontal_collision = x_hit || z_hit;
        let vertical = wanted.y != moved.y;
        self.motion.vertical_collision_below = vertical && wanted.y < 0.0;
        self.motion.on_ground = self.motion.vertical_collision_below;
        self.motion.supporting = self.supporting_block(w, moved);
        self.check_fall_damage(w, level, moved, events);
        self.restitute_movement(w, level, wanted, moved);
        self.movement_emission(w, moved, events);
        let factor = crate::motion::block_speed_factor_at(
            w.world,
            w.movement,
            w.loop_context,
            self.motion.position,
            self.motion.supporting.map(|[x, y, z]| BlockPos::new(x, y, z)),
        );
        self.motion.velocity =
            horizon_world::entity_move::scale_by_block_speed(self.motion.velocity, factor);
    }
    /// `Entity.restituteMovementAfterCollisions`.
    pub fn restitute_movement(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, wanted: Vec3, moved: Vec3) {
        let differs = |a: f64, b: f64| (b - a).abs() >= f64::from(1.0e-5_f32);
        let x_hit = differs(wanted.x, moved.x);
        let z_hit = differs(wanted.z, moved.z);
        let vertical = wanted.y != moved.y;
        if wanted.y.abs() > 0.0 && vertical || x_hit || z_hit {
            let [x, y, z] = self.block_on_legacy(w);
            let on = (x, y, z);
            let input = RestitutionInput {
                block: w.world.block(on.0, on.1, on.2),
                collided_x: x_hit,
                collided_z: z_hit,
                vertical_collision: vertical,
                collision_below: vertical && wanted.y < 0.0,
                living: true,
                resolved_movement: moved,
                fall_distance: self.motion.fall_distance,
            };
            let _ = restitute_movement(
                &mut CowRestitution { cow: self, world: w, level, on: BlockPos::new(on.0, on.1, on.2) },
                input,
            );
        }
    }
    /// `Entity.applyEffectsFromBlocks` for the bubble columns (`LivingEntity.aiStep`, after `travel`).
    pub fn apply_effects_from_blocks(&mut self, w: &MobWorld) {
        let mut velocity = self.motion.velocity;
        if crate::motion::bubble_column_push_over(
            w.world,
            w.registry,
            w.facts,
            w.collision,
            self.bounds(),
            &mut velocity,
            |_| {},
        ) {
            self.motion.fall_distance = 0.0;
        }
        self.motion.velocity = velocity;
    }
    /// `LivingEntity.checkFallDamage`.
    fn check_fall_damage(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, moved: Vec3, events: &mut Vec<CowEvent>) {
        if !self.fluids.in_water {
            self.update_fluids(w, events);
        }
        if !self.fluids.in_water && moved.y < 0.0 {
            self.motion.fall_distance -= f64::from(moved.y as f32);
        }
        if self.motion.on_ground {
            if self.motion.fall_distance > 0.0 {
                self.fall_on_block(w, level, events);
            }
            self.motion.fall_distance = 0.0;
        }
    }
    fn fall_on_block(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, events: &mut Vec<CowEvent>) {
        let pos = self.block_on_legacy(w);
        let state = w.world.block(pos[0], pos[1], pos[2]);
        let distance = self.motion.fall_distance;
        let result = w.landing.fall_on(
            &mut super::landing::CowLanding { cow: self, w, level, events, pos },
            state,
            BlockPos::new(pos[0], pos[1], pos[2]),
            LandingEntityKind::Living,
            distance,
        );
        if result.is_err() {
            self.cause_fall_damage_at(w.loop_context, level, distance, 1.0, events, Some((w, pos)), DamageKind::Fall);
        }
    }
    /// `LivingEntity.causeFallDamage`: true when the fall hurt.
    pub fn cause_fall_damage_at(
        &mut self,
        _context: &horizon_world::native_load_context::LoopContext,
        level: &mut DamageLevel<'_>,
        distance: f64,
        modifier: f32,
        _events: &mut Vec<CowEvent>,
        contact: Option<(&MobWorld, [i32; 3])>,
        source: DamageKind,
    ) -> bool {
        let landed = distance;
        if self.id.kind.flags.fall_immune {
            self.mod_block_fall(level, contact, landed, 0.0);
            return false;
        }
        let power = distance + 1.0e-6 - self.id.kind.safe_fall_distance;
        let damage = (power * f64::from(modifier) * self.id.kind.fall_damage_multiplier).floor();
        let damage = f64::from(self.mod_block_fall(level, contact, landed, damage as f32));
        if damage > 0.0 {
            self.damage(level, damage as f32, Damage::of(source));
            return true;
        }
        false
    }
    fn mod_block_fall(&self, level: &mut DamageLevel<'_>, contact: Option<(&MobWorld, [i32; 3])>, distance: f64, damage: f32) -> f32 {
        contact.map_or(damage, |(w, p)| {
            crate::block_contact::fall(
                level,
                crate::block_contact::Contact {
                    entity: self.id.id as i32,
                    kind: &self.id.kind.id,
                    state: w.world.block(p[0], p[1], p[2]),
                    position: BlockPos::new(p[0], p[1], p[2]),
                },
                distance,
                damage,
            )
        })
    }
    fn fluid_interaction(&self, w: &MobWorld) -> Fluids {
        w.fluids(self.bounds(), self.eye_y())
    }
    /// `Entity.updateFluidInteraction`.
    pub fn update_fluids(&mut self, w: &MobWorld, events: &mut Vec<CowEvent>) {
        let was_touching_water = self.fluids.in_water;
        let fluids = self.fluid_interaction(w);
        self.fluids.0 = fluids;
        if self.fluids.in_water || self.fluids.in_lava {
            self.motion.velocity = w.pushed_by_currents(self.bounds(), self.motion.velocity);
        }
        if self.fluids.in_water {
            self.motion.fall_distance = 0.0;
            if !was_touching_water && !self.env.first_tick {
                let _ = horizon_world::water_splash::water_splash(
                    &mut CowSplash { cow: self, events },
                    1_000_000,
                );
            }
        }
    }
    /// `Entity.applyMovementEmissionAndPlaySound` of a cow on land: the distance walked, the next step.
    /// The step sound and the game event have no listener here; nothing is drawn for them.
    fn movement_emission(&mut self, w: &MobWorld, moved: Vec3, _events: &mut Vec<CowEvent>) {
        let full = (moved.length() * f64::from(0.6_f32)) as f32;
        let horizontal = ((moved.x * moved.x + moved.z * moved.z).sqrt() * f64::from(0.6_f32)) as f32;
        let supporting = self.motion.supporting;
        let on = self.supported_on_pos(w, supporting, 1.0e-5);
        let state = w.world.block(on[0], on[1], on[2]);
        let climbable = w.facts.get(state).name.as_ref() == "minecraft:powder_snow"
            || w.has_tag("minecraft:climbable", &w.facts.get(state).name);
        let steps = &mut self.ai.steps;
        steps.move_dist += if climbable { full } else { horizontal };
        steps.fly_dist += full;
        if steps.move_dist > steps.next_step && !w.facts.is_air(state) {
            let legacy = self.supported_on_pos(w, supporting, 0.2);
            let legacy_state = w.world.block(legacy[0], legacy[1], legacy[2]);
            let produced = !w.facts.is_air(legacy_state) && (self.motion.on_ground || climbable);
            if produced {
                self.ai.steps.next_step = self.next_step();
            } else if self.fluids.in_water {
                self.ai.steps.next_step = self.next_step();
                // `Entity.waterSwimSound`: a pitch is drawn, the sound has no listener.
                let _pitch = 1.0 + (self.random.next_float() - self.random.next_float()) * 0.4;
            }
        }
    }
    fn next_step(&self) -> f32 {
        let walked = self.ai.steps.move_dist;
        (walked as i32).wrapping_add(1) as f32
    }
    /// `Entity.checkSupportingBlock`.
    fn supporting_block(&self, w: &MobWorld, moved: Vec3) -> Option<[i32; 3]> {
        if !self.motion.on_ground {
            return None;
        }
        let b = self.bounds();
        use horizon_core::geometry::Axis;
        let area = Aabb::new(
            Vec3::new(b.min(Axis::X), b.min(Axis::Y) - 1.0e-6, b.min(Axis::Z)),
            Vec3::new(b.max(Axis::X), b.min(Axis::Y), b.max(Axis::Z)),
        );
        w.supporting_block(area, self.motion.position).or_else(|| {
            let back = Vec3::new(-moved.x, 0.0, -moved.z);
            let shifted = Aabb::new(
                Vec3::new(area.min(Axis::X) + back.x, area.min(Axis::Y), area.min(Axis::Z) + back.z),
                Vec3::new(area.max(Axis::X) + back.x, area.max(Axis::Y), area.max(Axis::Z) + back.z),
            );
            w.supporting_block(shifted, self.motion.position)
        })
    }
    pub fn block_on_legacy(&self, w: &MobWorld) -> [i32; 3] {
        self.supported_on_pos(w, self.motion.supporting, 0.2)
    }
    fn supported_on_pos(&self, w: &MobWorld, supporting: Option<[i32; 3]>, offset: f32) -> [i32; 3] {
        let Some(p) = supporting else {
            return self.on_pos(offset);
        };
        if offset <= 1.0e-5 {
            return p;
        }
        let name = &w.facts.get(w.world.block(p[0], p[1], p[2])).name;
        let below = !w.has_tag("minecraft:walls", name)
            && !w.has_tag("minecraft:fence_gates", name)
            && (offset > 0.5 || !w.has_tag("minecraft:fences", name));
        if below {
            [p[0], (self.motion.position.y - f64::from(offset)).floor() as i32, p[2]]
        } else {
            p
        }
    }
    pub fn on_pos(&self, offset: f32) -> [i32; 3] {
        let p = self.motion.position;
        [p.x.floor() as i32, (p.y - f64::from(offset)).floor() as i32, p.z.floor() as i32]
    }
    fn friction_speed(&self, friction: f32) -> f32 {
        if self.motion.on_ground {
            let speed = self.ai.speed;
            if friction > 0.6 {
                speed * (0.216_000_02 / (friction * friction * friction))
            } else {
                speed
            }
        } else {
            0.02
        }
    }
    pub fn gravity(&self) -> f64 {
        if self.motion.no_gravity { 0.0 } else { self.id.kind.gravity }
    }
    /// `LivingEntity.travel`.
    pub fn travel(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, input: Vec3, events: &mut Vec<CowEvent>) {
        if self.id.kind.flags.flying {
            // A cow does not fly.
            return;
        }
        let (x, y, z) = (
            self.motion.position.x.floor() as i32,
            self.motion.position.y.floor() as i32,
            self.motion.position.z.floor() as i32,
        );
        let stands_on_fluid = self.id.kind.flags.lava_walker && w.fluid(x, y, z) == Fluid::Lava;
        if self.is_in_liquid() && !stands_on_fluid {
            self.travel_in_fluid(w, level, input, events);
        } else {
            self.travel_in_air(w, level, input, events);
        }
    }
    fn block_friction(&self, w: &MobWorld) -> f32 {
        let at = self.supported_on_pos(w, self.motion.supporting, horizon_world::movement_support::MOVEMENT_SUPPORT_OFFSET);
        crate::motion::block_friction(
            w.loop_context,
            self.id.id,
            &self.id.kind.id,
            w.world,
            w.facts,
            BlockPos::new(at[0], at[1], at[2]),
        )
    }
    fn modified_friction(friction: f32, modifier: f32) -> f32 {
        (1.0 - (1.0 - friction) * modifier).clamp(0.0, 1.0)
    }
    /// `LivingEntity.getAirDrag`.
    pub fn air_drag(&self) -> f32 {
        let base = if self.id.kind.flags.omnidirectional { 0.91 } else { 0.98 };
        Self::modified_friction(base, 1.0)
    }
    fn climbable_velocity(v: Vec3) -> Vec3 {
        let limit = f64::from(0.15f32);
        let y = if v.y.is_nan() { v.y } else { v.y.max(-limit) };
        Vec3::new(v.x.clamp(-limit, limit), y, v.z.clamp(-limit, limit))
    }
    fn travel_in_air(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, input: Vec3, events: &mut Vec<CowEvent>) {
        let friction = if self.motion.on_ground {
            Self::modified_friction(self.block_friction(w), 1.0)
        } else {
            1.0
        };
        let speed = self.friction_speed(friction);
        self.move_relative(speed, input);
        if self.on_climbable(w) {
            self.motion.fall_distance = 0.0;
            self.motion.velocity = Self::climbable_velocity(self.motion.velocity);
        }
        self.do_move(w, level, events, &mut MoveGate::new());
        let mut movement = self.motion.velocity;
        if (self.motion.horizontal_collision || self.ai.jumping) && self.on_climbable(w) {
            movement.y = 0.2;
        }
        let y = movement.y - self.gravity();
        let drag = Self::modified_friction(0.91, 1.0);
        let f = friction * drag;
        let vertical = if self.id.kind.flags.omnidirectional { drag } else { Self::modified_friction(0.98, 1.0) };
        self.motion.velocity = Vec3::new(movement.x * f64::from(f), y * f64::from(vertical), movement.z * f64::from(f));
    }
    fn travel_in_fluid(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, input: Vec3, events: &mut Vec<CowEvent>) {
        let falling = self.motion.velocity.y <= 0.0;
        let old_y = self.motion.position.y;
        let gravity = self.gravity();
        if self.fluids.in_water {
            let mut slow = self.id.kind.water_slowdown;
            let mut speed = 0.02f32;
            let mut efficiency = self.id.kind.water_efficiency as f32;
            if !self.motion.on_ground {
                efficiency *= 0.5;
            }
            if efficiency > 0.0 {
                slow += (0.546_000_06 - slow) * efficiency;
                speed += (self.ai.speed - speed) * efficiency;
            }
            self.move_relative(speed, input);
            self.do_move(w, level, events, &mut MoveGate::new());
            let mut m = self.motion.velocity;
            if self.motion.horizontal_collision && self.on_climbable(w) {
                m.y = 0.2;
            }
            m = Vec3::new(m.x * f64::from(slow), m.y * f64::from(0.8_f32), m.z * f64::from(slow));
            self.motion.velocity = crate::mobs::living::fluid_falling(gravity, falling, false, m);
        } else {
            self.move_relative(0.02, input);
            self.do_move(w, level, events, &mut MoveGate::new());
            if self.fluids.lava <= self.fluid_jump_threshold() {
                let v = self.motion.velocity;
                self.motion.velocity = Vec3::new(v.x * 0.5, v.y * f64::from(0.8_f32), v.z * 0.5);
                self.motion.velocity = crate::mobs::living::fluid_falling(gravity, falling, false, self.motion.velocity);
            } else {
                let v = self.motion.velocity;
                self.motion.velocity = Vec3::new(v.x * 0.5, v.y * 0.5, v.z * 0.5);
            }
            if gravity != 0.0 {
                self.motion.velocity.y -= gravity / 4.0;
            }
        }
        let m = self.motion.velocity;
        let ahead = self
            .bounds()
            .move_by(Vec3::new(m.x, m.y + f64::from(0.6_f32) - self.motion.position.y + old_y, m.z));
        let wide = ahead.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        let me = self.id.id;
        if self.motion.horizontal_collision
            && w.no_collision(ahead)
            && !w.contains_any_liquid(ahead)
            && !w.others_near(wide).any(|s| {
                s.id != me && s.alive && s.species == crate::mobs::Species::Shulker && s.bounds.intersects(wide)
            })
        {
            self.motion.velocity.y = f64::from(0.3_f32);
        }
    }
    /// `LivingEntity.handleJumping` of a non-flying cow.
    pub fn handle_jumping(&mut self, w: &MobWorld) {
        if self.ai.jumping {
            let (fluid_height, in_water_with_height) = if self.fluids.in_lava {
                (self.fluids.lava, false)
            } else {
                (self.fluids.water, self.fluids.in_water && self.fluids.water > 0.0)
            };
            let threshold = self.fluid_jump_threshold();
            if !in_water_with_height || self.motion.on_ground && fluid_height <= threshold {
                if !self.fluids.in_lava || self.motion.on_ground && self.fluids.lava <= threshold {
                    if (self.motion.on_ground || in_water_with_height && fluid_height <= threshold)
                        && self.ai.no_jump_delay == 0
                    {
                        self.jump_from_ground(w);
                        self.ai.no_jump_delay = 10;
                    }
                } else {
                    self.jump_in_liquid();
                }
            } else {
                self.jump_in_liquid();
            }
        } else {
            self.ai.no_jump_delay = 0;
        }
    }
    fn jump_in_liquid(&mut self) {
        self.motion.velocity.y += if self.ai.nav.can_float { f64::from(0.04_f32) } else { 0.3 };
    }
    pub fn jump_from_ground(&mut self, w: &MobWorld) {
        self.motion.needs_sync = true;
        let power = self.jump_power(w);
        if power > 1.0e-5 {
            self.motion.velocity.y = f64::from(power).max(self.motion.velocity.y);
        }
    }
    pub fn jump_power(&self, w: &MobWorld) -> f32 {
        self.attrs.jump_strength as f32 * self.block_jump_factor(w)
    }
    pub fn block_jump_factor(&self, w: &MobWorld) -> f32 {
        crate::motion::block_jump_factor_at(
            w.world,
            w.movement,
            w.loop_context,
            self.motion.position,
            self.motion.supporting.map(|[x, y, z]| BlockPos::new(x, y, z)),
        )
    }
    /// `LivingEntity.updateWalkAnimation`.
    pub fn update_walk_animation(&mut self) {
        let dy = if self.id.kind.flags.omnidirectional {
            self.motion.position.y - self.motion.previous.y
        } else {
            0.0
        };
        let dx = self.motion.position.x - self.motion.previous.x;
        let dz = self.motion.position.z - self.motion.previous.z;
        let distance = (dx * dx + dy * dy + dz * dz).sqrt() as f32;
        self.motion.previous_limb_speed = self.motion.limb_speed;
        if self.alive() {
            let target = (distance * 4.0).min(1.0);
            self.motion.limb_speed += (target - self.motion.limb_speed) * 0.4;
            self.motion.limb_position += self.motion.limb_speed;
        } else {
            self.motion.limb_speed = 0.0;
        }
    }
}

/// `Entity.doWaterSplashEffect` for a cow: its own random, no listener for the sound or particles.
struct CowSplash<'a, 'b, 'c> {
    cow: &'a mut CowItem<'b, 'c>,
    #[allow(dead_code)]
    events: &'a mut Vec<CowEvent>,
}
impl horizon_world::water_splash::WaterSplashWorld for CowSplash<'_, '_, '_> {
    type Error = std::convert::Infallible;
    type Entity = ();
    type Sound = &'static str;
    fn this_entity(&self) {}
    fn controlling_passenger(&mut self) -> Result<Option<()>, Self::Error> {
        Ok(None)
    }
    fn velocity(&mut self, _: ()) -> Result<Vec3, Self::Error> {
        Ok(self.cow.motion.velocity)
    }
    fn splash_sound(&mut self, _high_speed: bool) -> Result<&'static str, Self::Error> {
        Ok("entity.generic.splash")
    }
    fn random_float(&mut self) -> Result<f32, Self::Error> {
        Ok(self.cow.random.next_float())
    }
    fn random_double(&mut self) -> Result<f64, Self::Error> {
        Ok(self.cow.random.next_double())
    }
    fn play_sound(&mut self, _: &'static str, _: f32, _: f32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn position(&self) -> Vec3 {
        self.cow.motion.position
    }
    fn width(&self) -> f32 {
        self.cow.width()
    }
    fn particle(
        &mut self,
        _: horizon_world::water_splash::SplashParticle,
        _: Vec3,
        _: Vec3,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn splash_event(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// `movement_restitution::restitute_movement` for a cow.
struct CowRestitution<'a, 'b, 'c, 'w, 'l> {
    cow: &'a mut CowItem<'b, 'c>,
    world: &'a MobWorld<'w>,
    level: &'a mut DamageLevel<'l>,
    on: BlockPos,
}
impl CowRestitution<'_, '_, '_, '_, '_> {
    fn movement(&self, state: BlockStateId) -> Result<horizon_world::movement_properties::MovementBlock, ()> {
        self.world.movement.get(state).map_err(|_| ())
    }
}
impl RestitutionWorld for CowRestitution<'_, '_, '_, '_, '_> {
    type Error = ();
    fn suppressing_bounce(&mut self) -> Result<bool, ()> {
        Ok(false)
    }
    fn entity_bounciness(&mut self) -> Result<f64, ()> {
        Ok(0.0)
    }
    fn velocity(&mut self) -> Result<Vec3, ()> {
        Ok(self.cow.motion.velocity)
    }
    fn effective_gravity(&mut self) -> Result<f64, ()> {
        Ok(self.cow.gravity())
    }
    fn block_suppresses_bounce(&mut self, state: BlockStateId) -> Result<bool, ()> {
        Ok(self.movement(state)?.suppresses_bounce)
    }
    fn block_bounce_restitution(&mut self, state: BlockStateId) -> Result<f32, ()> {
        let own = self.movement(state)?.coefficients.bounce_restitution;
        let client_side = self.world.loop_context.side() == horizon_world::native_load_context::LoopSide::Client;
        let contact = crate::block_contact::Contact {
            entity: self.cow.id.id as i32,
            kind: &self.cow.id.kind.id,
            state,
            position: self.on,
        };
        Ok(crate::block_contact::bounce_restitution(
            &mut self.level.reborrow().on_client(client_side),
            contact,
            own,
        ))
    }
    fn air_drag(&mut self) -> Result<f32, ()> {
        Ok(self.cow.air_drag())
    }
    fn movement_support_position(&mut self) -> Result<BlockPos, ()> {
        let position = self.cow.motion.position;
        let offset = f64::from(horizon_world::movement_support::MOVEMENT_SUPPORT_OFFSET);
        let Some(support) = self.cow.motion.supporting else {
            return Ok(BlockPos::new(
                position.x.floor() as i32,
                (position.y - offset).floor() as i32,
                position.z.floor() as i32,
            ));
        };
        let state = self.world.world.block(support[0], support[1], support[2]);
        let keeps_column = self.movement(state).is_ok_and(|b| b.wall || b.coefficients.fence_gate);
        Ok(if keeps_column {
            BlockPos::new(support[0], support[1], support[2])
        } else {
            BlockPos::new(support[0], (position.y - offset).floor() as i32, support[2])
        })
    }
    fn bounce_on(&mut self, state: BlockStateId, position: BlockPos, fall_distance: f64, movement: Vec3) -> Result<Vec3, ()> {
        Ok(crate::block_contact::bounce(
            self.level,
            crate::block_contact::Contact {
                entity: self.cow.id.id as i32,
                kind: &self.cow.id.kind.id,
                state,
                position,
            },
            fall_distance,
            movement,
        ))
    }
    fn bounce_event(&mut self) -> Result<(), ()> {
        Ok(())
    }
    fn mark_position_for_sync(&mut self) {
        self.cow.motion.sync_position = true;
    }
    fn set_velocity(&mut self, velocity: Vec3) -> Result<(), ()> {
        self.cow.motion.velocity = velocity;
        Ok(())
    }
}
