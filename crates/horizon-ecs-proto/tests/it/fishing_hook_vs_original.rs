//! The fishing hook against the vanilla `FishingHook` (`game/items/fishing.rs`): the flight in the free air is the transcribed
//! reference tick by tick; the floating hook's events (bubbles, fish, splash, bite) are the vanilla draws in vanilla's order, tick
//! by tick; the reel (a catch in the bite, nothing outside it, a pull on a hooked creature) on the same ticks.
//! `HORIZON_TEST_RESOURCES` set. Rain is not set here: the ECS has no weather setter yet, so the rain variant waits for it.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::{trig, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::cow::{Herd, Pos};
use horizon_ecs_proto::fishing_hook::{cast, reel, Hook, Phase, Target};
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::host_out::HostEvent;

const SEED: i64 = 7;

/// The player who casts, standing at `at` (the owner's reach and the hook's owner are read from `Online`).
fn stand(h: &mut Herd, at: Vec3) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
}
fn drain(h: &mut Herd) -> Vec<HostEvent> {
    h.world.resource_mut::<Events>().host.drain().collect()
}
/// A pool of water over the hook's cell and the cells around it, from the floor to the surface.
fn lake(h: &mut Herd) {
    for x in 0..=1 {
        for y in 0..=3 {
            for z in 0..=1 {
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
}
/// The cell of the lake (water from x 0 to 1, y 0 to 3, z 0 to 1).
fn lake_cell(x: f64, y: f64, z: f64) -> bool {
    (0.0..2.0).contains(&x.floor()) && (0.0..4.0).contains(&y.floor()) && (0.0..2.0).contains(&z.floor())
}
fn is_splash(ev: &HostEvent) -> bool {
    matches!(ev, HostEvent::Sound { name, .. } if *name == "minecraft:entity.fishing_bobber.splash")
}
/// What a tick of the fishing emits, as the vanilla reference names it: the splash sound as `sound`, the particles by kind and count.
fn mark(ev: &HostEvent) -> Option<(&'static str, u32)> {
    match ev {
        HostEvent::Sound { name, .. } if *name == "minecraft:entity.fishing_bobber.splash" => Some(("sound", 0)),
        HostEvent::Particles { kind, count, .. } => Some((*kind, *count)),
        _ => None,
    }
}
/// Ticks until the splash of the bite (the first 800 ticks), or none.
fn until_bite(h: &mut Herd, at: Vec3) -> Option<u64> {
    for t in 1..=800 {
        stand(h, at);
        h.tick();
        if drain(h).iter().any(is_splash) {
            return Some(t);
        }
    }
    None
}

/// The vanilla draws of a floating hook, one tick: `bobbing` (the force's draw) then `catchingFish`, transcribed in vanilla's order.
/// It returns the events of the tick as `mark` names them.
struct Vanilla {
    rng: LegacyRandom,
    nibble: i32,
    lured: i32,
    hooked: i32,
    angle: f32,
}
impl Vanilla {
    fn new(seed: i64) -> Self {
        Vanilla { rng: LegacyRandom::new(seed), nibble: 0, lured: 0, hooked: 0, angle: 0.0 }
    }
    fn tick(&mut self, pos: Vec3, rain: bool, sky: bool, wet: bool, water: impl Fn([f64; 3]) -> bool) -> Vec<(&'static str, u32)> {
        let mut out = Vec::new();
        self.rng.next_float();
        if !wet {
            return out;
        }
        let mut speed = 1;
        if self.rng.next_float() < 0.25 && rain {
            speed += 1;
        }
        if self.rng.next_float() < 0.5 && !sky {
            speed -= 1;
        }
        if self.nibble > 0 {
            self.nibble -= 1;
            if self.nibble <= 0 {
                self.lured = 0;
                self.hooked = 0;
            }
        } else if self.hooked > 0 {
            self.hooked -= speed;
            if self.hooked > 0 {
                self.angle += self.rng.triangle(0.0, 9.188) as f32;
                let angle = self.angle * (std::f64::consts::PI / 180.0) as f32;
                let (sin, cos) = (trig::sin(f64::from(angle)), trig::cos(f64::from(angle)));
                let distance = self.hooked as f32 * 0.1;
                let fish = [pos.x + f64::from(sin * distance), f64::from(pos.y.floor() as f32 + 1.0), pos.z + f64::from(cos * distance)];
                if water(fish) {
                    if self.rng.next_float() < 0.15 {
                        out.push(("bubble", 1));
                    }
                    out.push(("fishing", 0));
                    out.push(("fishing", 0));
                }
            } else {
                let _pitch = 1.0 + (self.rng.next_float() - self.rng.next_float()) * 0.4;
                out.push(("sound", 0));
                out.push(("bubble", 6));
                out.push(("fishing", 6));
                self.nibble = 20 + self.rng.next_int_bound(21).unwrap_or(0);
            }
        } else if self.lured > 0 {
            self.lured -= speed;
            let t = self.lured;
            let mut tease = 0.15_f32;
            if t < 20 {
                tease += (20 - t) as f32 * 0.05;
            } else if t < 40 {
                tease += (40 - t) as f32 * 0.02;
            } else if t < 60 {
                tease += (60 - t) as f32 * 0.01;
            }
            if self.rng.next_float() < tease {
                let angle = (self.rng.next_float() * 360.0) * (std::f64::consts::PI / 180.0) as f32;
                let distance = self.rng.next_float() * 35.0 + 25.0;
                let fish = [pos.x + f64::from(trig::sin(f64::from(angle)) * distance) * 0.1, f64::from(pos.y.floor() as f32 + 1.0), pos.z + f64::from(trig::cos(f64::from(angle)) * distance) * 0.1];
                if water(fish) {
                    out.push(("splash", 2 + self.rng.next_int_bound(2).unwrap_or(0) as u32));
                }
            }
            if self.lured <= 0 {
                self.angle = self.rng.next_float() * 360.0;
                self.hooked = 20 + self.rng.next_int_bound(61).unwrap_or(0);
            }
        } else {
            self.lured = 100 + self.rng.next_int_bound(501).unwrap_or(0);
        }
        out
    }
}

/// The vanilla flight in the free air: the velocity loses the gravity 0.03 (float), the position moves by it, then the drag 0.92.
#[test]
fn flight_in_the_air_follows_the_vanilla_path_tick_by_tick() {
    if !have_data() {
        return;
    }
    let at = Vec3::new(0.5, 60.0, 0.5);
    let v = Vec3::new(0.4, 0.2, -0.1);
    let mut h = Herd::with_slab(16, true, false);
    stand(&mut h, at);
    let hook = cast(&mut h.world, 1, at, v, 0, 0, SEED);
    let (mut p, mut vel) = (at, v);
    for t in 1..=40 {
        stand(&mut h, at);
        h.tick();
        vel = Vec3::new(vel.x, vel.y - f64::from(0.03_f32), vel.z);
        p = Vec3::new(p.x + vel.x, p.y + vel.y, p.z + vel.z);
        vel = Vec3::new(vel.x * 0.92, vel.y * 0.92, vel.z * 0.92);
        let got = h.world.get::<Pos>(hook).unwrap().0;
        assert!(got.x == p.x && got.y == p.y && got.z == p.z, "tick {t}: hook {got:?}, vanilla {p:?}");
    }
}

/// A cast into a lake: each tick of the floating hook emits what the vanilla draws give, in order, for 800 ticks (the lure, the fish,
/// the splash and the bite window). The sky is clear, or closed by a block above the hook.
fn floating_follows_the_vanilla_draws(rain: bool, closed: bool) {
    let at = Vec3::new(0.5, 3.5, 0.5);
    let mut h = Herd::with_slab(16, true, false);
    lake(&mut h);
    h.set_raining(rain);
    if closed {
        h.set_block(0, 5, 0, "minecraft:stone");
    }
    stand(&mut h, at);
    let hook = cast(&mut h.world, 1, at, Vec3::ZERO, 0, 0, SEED);
    let mut vanilla = Vanilla::new(SEED);
    for t in 1..=800 {
        let before = h.world.get::<Hook>(hook).unwrap().phase;
        let pos = h.world.get::<Pos>(hook).unwrap().0;
        stand(&mut h, at);
        h.tick();
        let got: Vec<(&str, u32)> = drain(&mut h).iter().filter_map(mark).collect();
        let want = if before == Phase::Bobbing {
            vanilla.tick(pos, rain && !closed, !closed, lake_cell(pos.x, pos.y, pos.z), |f| lake_cell(f[0], f[1] - 1.0, f[2]))
        } else {
            Vec::new()
        };
        assert_eq!(got, want, "tick {t}, hook at {pos:?}");
    }
}
#[test]
fn floating_follows_the_vanilla_draws_under_a_clear_sky() {
    if !have_data() {
        return;
    }
    floating_follows_the_vanilla_draws(false, false);
}
#[test]
fn floating_follows_the_vanilla_draws_under_a_closed_sky() {
    if !have_data() {
        return;
    }
    floating_follows_the_vanilla_draws(false, true);
}
#[test]
fn floating_follows_the_vanilla_draws_in_the_rain_and_a_closed_sky() {
    if !have_data() {
        return;
    }
    floating_follows_the_vanilla_draws(true, false);
    floating_follows_the_vanilla_draws(true, true);
}

/// The reel during the bite is a catch (one `FishingCatch` for the owner, rod damage 1); the reel before it is nothing (no catch, rod
/// damage 0). The hook is taken back in both cases.
#[test]
fn reel_in_the_bite_catches_and_outside_it_catches_nothing() {
    if !have_data() {
        return;
    }
    let at = Vec3::new(0.5, 3.5, 0.5);
    let mut h = Herd::with_slab(16, true, false);
    lake(&mut h);
    stand(&mut h, at);
    let hook = cast(&mut h.world, 1, at, Vec3::ZERO, 0, 0, SEED);
    stand(&mut h, at);
    h.tick();
    drain(&mut h);
    reel(&mut h.world, 1);
    let early = drain(&mut h);
    assert!(!early.iter().any(|e| matches!(e, HostEvent::FishingCatch { .. })), "a catch before the bite");
    assert!(early.iter().any(|e| matches!(e, HostEvent::HookRetrieved { owner: 1, damage: 0 })));
    assert!(h.world.get_entity(hook).is_err(), "the hook is taken back");

    let mut h = Herd::with_slab(16, true, false);
    lake(&mut h);
    stand(&mut h, at);
    cast(&mut h.world, 1, at, Vec3::ZERO, 0, 0, SEED);
    until_bite(&mut h, at).expect("a bite within 800 ticks");
    reel(&mut h.world, 1);
    let bitten = drain(&mut h);
    let catches = bitten.iter().filter(|e| matches!(e, HostEvent::FishingCatch { owner: 1, .. })).count();
    assert_eq!(catches, 1, "one catch in the bite");
    assert!(bitten.iter().any(|e| matches!(e, HostEvent::HookRetrieved { owner: 1, damage: 1 })));
}

/// A hook that hits a cow is hooked to it and follows it; the reel pulls the cow towards the owner by 0.1 of the offset from the hook
/// (`pullEntity`), and takes the rod's damage of 5.
#[test]
fn reel_pulls_a_hooked_creature_towards_the_owner() {
    if !have_data() {
        return;
    }
    let owner = Vec3::new(0.5, 1.5, 0.5);
    let mut h = Herd::with_slab(16, true, false);
    let cow = ecs_cow(&mut h, 3.5, 1.0, 0.5, 7);
    stand(&mut h, owner);
    let hook = cast(&mut h.world, 1, owner, Vec3::new(1.0, 0.0, 0.0), 0, 0, SEED);
    let mut hooked = false;
    for _ in 1..=20 {
        stand(&mut h, owner);
        h.tick();
        if h.world.get::<Hook>(hook).map(|x| x.phase) == Some(Phase::Hooked(Target::Entity(cow))) {
            hooked = true;
            break;
        }
    }
    assert!(hooked, "the hook hit the cow within 20 ticks");
    stand(&mut h, owner);
    h.tick();
    let at = h.world.get::<Pos>(hook).unwrap().0;
    drain(&mut h);
    reel(&mut h.world, 1);
    let out = drain(&mut h);
    let k = 0.1;
    let want = [(owner.x - at.x) * k, (owner.y - at.y) * k, (owner.z - at.z) * k];
    assert!(out.iter().any(|e| matches!(e, HostEvent::PullEntity { entity, velocity } if *entity == cow && *velocity == want)), "pull {want:?}");
    assert!(out.iter().any(|e| matches!(e, HostEvent::HookRetrieved { owner: 1, damage: 5 })));
    assert!(h.world.get_entity(hook).is_err(), "the hook is taken back");
}
