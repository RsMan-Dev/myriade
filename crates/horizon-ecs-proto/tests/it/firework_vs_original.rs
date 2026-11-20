//! The firework rocket of `firework.rs` against an independent model of `FireworkRocketEntity` in open air: the same position on
//! every tick, the same lifetime draw, the blast where the jar's rocket goes off, and its damage to a creature two and a half
//! blocks away. The model draws its own numbers in vanilla's order, from the same seed.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_ecs_proto::{cow::*, firework::*};

const SEED: i64 = 7;
const FLIGHT: u8 = 1;
const EXPLOSIONS: u8 = 2;

/// The jar's rocket: its velocity drawn at the constructor, its lifetime, and its motion per tick.
struct Model {
    pos: Vec3,
    vel: Vec3,
    life: i32,
}
impl Model {
    fn new(at: Vec3, flight: i32, rng: &mut LegacyRandom) -> Self {
        let x = rng.triangle(0.0, 0.002297);
        let z = rng.triangle(0.0, 0.002297);
        let life = 10 * (1 + flight) + rng.next_int_bound(6).unwrap() + rng.next_int_bound(7).unwrap();
        Self { pos: at, vel: Vec3::new(x, 0.05, z), life }
    }
    /// One tick of motion: sideways speed 1.15, rise 0.04, then the velocity is the motion.
    fn step(&mut self) {
        let m = Vec3::new(self.vel.x * 1.15, self.vel.y + 0.04, self.vel.z * 1.15);
        self.pos = Vec3::new(self.pos.x + m.x, self.pos.y + m.y, self.pos.z + m.z);
        self.vel = m;
    }
}

fn shot(at: Vec3) -> Shot {
    Shot { at, owner: None, flight: FLIGHT, explosions: EXPLOSIONS, silent: true, carrier: None, angled: None }
}
fn close(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.z - b.z).abs() < 1e-9
}

/// A creature standing with its feet at `feet`, in the spatial index, with its health.
fn creature_at(h: &mut Herd, feet: Vec3) -> Entity {
    let (w, hgt) = (0.25_f32, 0.25_f32);
    let e = h
        .world
        .spawn((
            Pos(feet),
            Vel(Vec3::ZERO),
            Body { width: w, height: hgt, step_height: 0.0, gravity: 0.04, speed: 0.0, fall_factor: 1.0, object: false, orb: false, tnt: false, lava_walker: false },
            Bounds(Aabb::new(
                Vec3::new(feet.x - 0.125, feet.y, feet.z - 0.125),
                Vec3::new(feet.x + 0.125, feet.y + 0.25, feet.z + 0.125),
            )),
            Cell(u32::MAX, [i32::MIN; 8], u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0 },
            FluidState::default(),
            Health(6.0),
        ))
        .id();
    enter_index(&mut h.world, e);
    e
}

/// A free rocket over open ground: at every tick it is where the jar's rocket is, and it goes off at the tick the jar's does
/// (after its `life` ticks of flight, the lifetime drawn at launch).
#[test]
fn a_free_rocket_flies_and_goes_off_as_the_jar_s_does() {
    if !have_data() {
        return;
    }
    let (_r, mut h) = flat_pair();
    let at = Vec3::new(0.5, 10.0, 0.5);
    let e = launch(&mut h.world, shot(at), &mut LegacyRandom::new(SEED));
    let mut model = Model::new(at, i32::from(FLIGHT), &mut LegacyRandom::new(SEED));
    let life = model.life;
    for t in 1..=life + 2 {
        h.tick();
        if t <= life + 1 {
            model.step();
            assert!(h.world.get_entity(e).is_ok(), "gone too early at tick {t}");
            let p = h.world.get::<Pos>(e).unwrap().0;
            assert!(close(p, model.pos), "t={t}: rocket {p:?} / jar {:?}", model.pos);
        } else {
            assert!(h.world.get_entity(e).is_err(), "still there at tick {t}, life {life}");
        }
    }
}

/// The blast of the same rocket: a creature 2.5 blocks from where it goes off takes `(5 + 2 * explosions) * sqrt((5 - d) / 5)`.
#[test]
fn the_blast_hurts_a_creature_as_the_jar_s_does() {
    if !have_data() {
        return;
    }
    let at = Vec3::new(0.5, 10.0, 0.5);
    let mut probe = Model::new(at, i32::from(FLIGHT), &mut LegacyRandom::new(SEED));
    let life = probe.life;
    for _ in 0..=life {
        probe.step();
    }
    let boom = probe.pos;
    let (_r, mut h) = flat_pair();
    launch(&mut h.world, shot(at), &mut LegacyRandom::new(SEED));
    for _ in 0..life {
        h.tick();
    }
    // The creature is set at the last moment (one that waits for the flight falls out of reach), 2.5 blocks from the blast; the rocket
    // goes off in the tick `life + 1` and the queued blow is applied by the next.
    let c = creature_at(&mut h, Vec3::new(boom.x + 2.5, boom.y, boom.z));
    let mut got = None;
    for _ in 0..3 {
        h.tick();
        got = got.or(h.world.resource::<DamageQueue>().0.iter().find(|x| x.target == c).map(|x| x.amount));
    }
    let want = (5.0 + 2.0 * f32::from(EXPLOSIONS)) * ((5.0 - 2.5_f32) / 5.0).sqrt();
    assert!(got.is_some_and(|a| (a - want).abs() < 1e-3), "creature hit {got:?}, jar {want}");
}
