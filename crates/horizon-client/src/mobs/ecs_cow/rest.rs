//! Rest of a cow: a tick that starts from the very state the previous one started from, in unchanged
//! surroundings, ends where it ended (79 % of the ticks of a cow on flat ground). The step that
//! moves it (`travel`: the physics, the collisions, the fluids, the fall) is then not made again: its
//! outputs, kept from the tick that made them, are put back. Invisible by construction of the key and
//! by the equality test: the key holds everything `travel` reads, and a tick that drew a random, raised an
//! event or hurt the cow is never kept. It is a comparison of values (the same on a plain struct): the
//! bevy change ticks are not usable here, every step of the tick writes its fields.
use super::{event::CowEvent, view::CowItem};
use crate::{health::DamageLevel, mobs::MobWorld};
use horizon_client_render::world::RegionStamp;
use horizon_core::geometry::Vec3;

#[derive(Clone, Debug, Default)]
pub struct RestState {
    pub enabled: bool,
    pub memo: Option<Box<RestMemo>>,
    pub hits: u64,
    pub asks: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestKey {
    position: [u64; 3],
    velocity: [u64; 3],
    yaw: u32,
    flags: [bool; 8],
    stuck: [u64; 3],
    supporting: Option<[i32; 3]>,
    fall: u64,
    input: [u32; 4],
    steps: [u32; 3],
    fluids: [u64; 2],
    fluid_flags: [bool; 4],
    baby: bool,
    stamp: RegionStamp,
    border: Option<[u64; 4]>,
}
#[derive(Clone, Debug)]
pub struct RestOut {
    position: Vec3,
    velocity: Vec3,
    flags: [bool; 4],
    supporting: Option<[i32; 3]>,
    stuck: Vec3,
    fall: f64,
    steps: [f32; 3],
    fluids: crate::mobs::living::Fluids,
}
#[derive(Clone, Debug)]
pub struct RestMemo {
    key: RestKey,
    out: RestOut,
}

/// The mutation of the test: the key holds only the blocks around the cow (the test must then fail).
thread_local!(pub static MUTATE_KEY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });

fn bits(v: Vec3) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

impl CowItem<'_, '_> {
    /// What `travel` reads, or None when it may read something this key does not hold.
    fn rest_key(&self, w: &MobWorld, input: Vec3) -> Option<RestKey> {
        let v = self.motion.velocity;
        if self.motion.no_physics
            || v.x.abs() > 1.0
            || v.y.abs() > 1.0
            || v.z.abs() > 1.0
            || !w.world.moving_pistons.is_empty()
            || !w.world.shulkers.is_empty()
            || w.others_solid.is_some_and(|s| !s.is_empty())
        {
            return None;
        }
        let b = self.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let stamp = w.world.region_stamp(
            [lo.x.floor() as i32 - 2, lo.y.floor() as i32 - 2, lo.z.floor() as i32 - 2],
            [hi.x.floor() as i32 + 2, hi.y.floor() as i32 + 2, hi.z.floor() as i32 + 2],
        )?;
        let f = &self.fluids;
        let mut key = RestKey {
            position: bits(self.motion.position),
            velocity: bits(v),
            yaw: self.motion.yaw.to_bits(),
            flags: [
                self.motion.on_ground,
                self.motion.horizontal_collision,
                self.motion.vertical_collision_below,
                self.motion.no_gravity,
                self.ai.jumping,
                self.ai.nav.can_float,
                self.env.first_tick,
                self.motion.sync_position,
            ],
            stuck: bits(self.motion.stuck_multiplier),
            supporting: self.motion.supporting,
            fall: self.motion.fall_distance.to_bits(),
            input: [
                (input.x as f32).to_bits(),
                (input.y as f32).to_bits(),
                (input.z as f32).to_bits(),
                self.ai.speed.to_bits(),
            ],
            steps: [
                self.ai.steps.move_dist.to_bits(),
                self.ai.steps.next_step.to_bits(),
                self.ai.steps.fly_dist.to_bits(),
            ],
            fluids: [f.water.to_bits(), f.lava.to_bits()],
            fluid_flags: [f.in_water, f.in_lava, f.eye_in_water, f.eye_in_lava],
            baby: self.is_baby(),
            stamp,
            border: w.border.map(|b| {
                let bb = b.bounds(0.0);
                bb.map(f64::to_bits)
            }),
        };
        if MUTATE_KEY.with(std::cell::Cell::get) {
            // The mutation: a key that holds the blocks around the cow and nothing of the cow.
            key = RestKey {
                position: [0; 3],
                velocity: [0; 3],
                yaw: 0,
                flags: [false; 8],
                stuck: [0; 3],
                supporting: None,
                fall: 0,
                input: [0; 4],
                steps: [0; 3],
                fluids: [0; 2],
                fluid_flags: [false; 4],
                baby: false,
                stamp: key.stamp,
                border: None,
            };
        }
        Some(key)
    }

    /// `LivingEntity.travel`, or its kept outputs when the cow rests.
    pub fn travel_or_rest(&mut self, w: &MobWorld, level: &mut DamageLevel<'_>, input: Vec3, events: &mut Vec<CowEvent>) {
        if !self.rest.enabled {
            return self.travel(w, level, input, events);
        }
        let key = self.rest_key(w, input);
        self.rest.asks += 1;
        if let (Some(k), Some(memo)) = (&key, &self.rest.memo)
            && memo.key == *k
        {
            let out = memo.out.clone();
            self.rest.hits += 1;
            self.motion.position = out.position;
            self.motion.velocity = out.velocity;
            self.motion.on_ground = out.flags[0];
            self.motion.horizontal_collision = out.flags[1];
            self.motion.vertical_collision_below = out.flags[2];
            self.motion.sync_position = out.flags[3];
            self.motion.supporting = out.supporting;
            self.motion.stuck_multiplier = out.stuck;
            self.motion.fall_distance = out.fall;
            self.ai.steps.move_dist = out.steps[0];
            self.ai.steps.next_step = out.steps[1];
            self.ai.steps.fly_dist = out.steps[2];
            self.fluids.0 = out.fluids;
            return;
        }
        let (random, n, health) = (self.random.state(), events.len(), self.vitals.health);
        self.travel(w, level, input, events);
        self.rest.memo = key
            .filter(|_| self.random.state() == random && events.len() == n && self.vitals.health == health)
            .map(|key| {
                Box::new(RestMemo {
                    key,
                    out: RestOut {
                        position: self.motion.position,
                        velocity: self.motion.velocity,
                        flags: [
                            self.motion.on_ground,
                            self.motion.horizontal_collision,
                            self.motion.vertical_collision_below,
                            self.motion.sync_position,
                        ],
                        supporting: self.motion.supporting,
                        stuck: self.motion.stuck_multiplier,
                        fall: self.motion.fall_distance,
                        steps: [self.ai.steps.move_dist, self.ai.steps.next_step, self.ai.steps.fly_dist],
                        fluids: self.fluids.0,
                    },
                })
            });
    }
}
