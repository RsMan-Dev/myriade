//! The dragon's breath (Horizon `game/mobs/dragon.rs`: `Breath`, `breath_victims`, `breath_expired`, `harm_amount`, `cloud_hits`,
//! `breath_puffs`, and the flame's origin of `Phase::Flaming`, D:2563-2592). A breath is an area cloud: `Cloud` names no creature.
//! It is inert for `WAIT` ticks, then grows by `radius_per_tick`, harms what lies in it every 20 ticks (checked every 5) and goes
//! at its duration. The puffs are client particles: they come back as data, not emitted (see the notes).
use crate::area;
use crate::cow::Pos;
use crate::goals::Events;
use crate::targeting::{self, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};

/// `AreaEffectCloud.WAIT_TIME` (D:69): inert for its first 20 ticks.
pub const WAIT: i32 = 20;
/// The victims are checked every 5 ticks and spared for 20 (`reapplicationDelay`), D:77-79.
const CHECK_EVERY: i32 = 5;
const REAPPLY: i32 = 20;

/// The shape of a cloud: its radius, the radius it reaches at its end (`setRadiusPerTick`), its duration, the amplifier of its
/// instant damage. The flame keeps its radius; a fireball's cloud grows (`fireball::CLOUD`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudSpec {
    pub radius: f32,
    pub grown: f32,
    pub duration: i32,
    pub amplifier: u32,
}

/// The flame of `Phase::Flaming` (D:2579-2591): radius 5, duration 200, instant damage of amplifier 0.
pub const FLAME: CloudSpec = CloudSpec { radius: 5.0, grown: 5.0, duration: 200, amplifier: 0 };

impl CloudSpec {
    pub fn cloud(self, at: [f64; 3]) -> Cloud {
        Cloud {
            at,
            radius: self.radius,
            radius_per_tick: (self.grown - self.radius) / self.duration as f32,
            age: 0,
            duration: self.duration,
            amplifier: self.amplifier,
            victims: Vec::new(),
        }
    }
}

/// An area cloud (`AreaEffectCloud`): its floor, its radius, its age and life, its instant damage, and who it hit until when.
#[derive(Component, Clone, Debug)]
pub struct Cloud {
    pub at: [f64; 3],
    pub radius: f32,
    /// Added each tick once the wait is over (0 for the flame).
    pub radius_per_tick: f32,
    pub age: i32,
    pub duration: i32,
    pub amplifier: u32,
    /// `AreaEffectCloud.victims`: who, and the age until which it is spared.
    pub victims: Vec<(Who, i32)>,
}

/// The dragon's flame cloud: it goes when the flame phase is left (`end_flames`).
#[derive(Component)]
pub struct Flame;

/// What a tick of a cloud gives: gone, or the players it harms now.
pub enum Step {
    Gone,
    Live(Vec<Who>),
}

/// `AreaEffectCloud.serverTick`: discarded once `age - WAIT >= duration`, so the wait does not count against the duration.
pub fn expired(age: i32, duration: i32) -> bool {
    age - WAIT >= duration
}

/// The instant damage of `amplifier` at the cloud's 0.5 scale: `(int) (0.5 * (6 << amplifier) + 0.5)` (D:50-52).
pub fn harm(amplifier: u32) -> f32 {
    (0.5 * f64::from(6u32 << amplifier) + 0.5) as i32 as f32
}

/// Whether a cloud at `at` (its floor) reaches a creature: its box is `radius * 2` wide and 0.5 tall, and the creature's
/// position lies within the radius horizontally (D:57-65).
pub fn hits(at: [f64; 3], radius: f32, bounds: Aabb, pos: Vec3) -> bool {
    let r = f64::from(radius);
    let area = Aabb::new(Vec3::new(at[0] - r, at[1], at[2] - r), Vec3::new(at[0] + r, at[1] + 0.5, at[2] + r));
    let (dx, dz) = (pos.x - at[0], pos.z - at[2]);
    bounds.intersects(area) && dx * dx + dz * dz <= f64::from(radius * radius)
}

/// `AreaEffectCloud.serverTick` victims (D:80-97): every 5 ticks after the wait, drop the entries whose delay has passed, then
/// return the candidates not in the list, recording them until `age + 20`.
pub fn due(age: i32, victims: &mut Vec<(Who, i32)>, candidates: impl IntoIterator<Item = Who>) -> Vec<Who> {
    if age < WAIT || age % CHECK_EVERY != 0 {
        return Vec::new();
    }
    victims.retain(|&(_, until)| age < until);
    let mut hit = Vec::new();
    for who in candidates {
        if !victims.iter().any(|&(v, _)| v == who) {
            victims.push((who, age + REAPPLY));
            hit.push(who);
        }
    }
    hit
}

/// One tick of a cloud (D:3196-3249): ages it, grows it after its wait, and returns whom it harms now among `candidates`
/// (who, box, position). A cloud that has expired gives `Gone` and harms nothing on that tick.
pub fn step(c: &mut Cloud, candidates: &[(Who, Aabb, Vec3)]) -> Step {
    c.age += 1;
    if expired(c.age, c.duration) {
        return Step::Gone;
    }
    if c.age >= WAIT {
        c.radius += c.radius_per_tick;
    }
    let (at, r) = (c.at, c.radius);
    let inside = candidates.iter().filter(|(_, b, p)| hits(at, r, *b, *p)).map(|&(w, _, _)| w);
    Step::Live(due(c.age, &mut c.victims, inside))
}

/// Where a flame starts (D:2563-2592): 2.5 blocks ahead of the head along its horizontal look (`look`, a unit (x, z)), then down
/// to the first block that is not air; the cloud floor is the block above it.
pub fn flame_origin(head: [f64; 3], look: [f64; 2], is_air: impl Fn(i32, i32, i32) -> bool) -> [f64; 3] {
    let (x, z) = (head[0] + look[0] * 2.5, head[2] + look[1] * 2.5);
    let mut y = head[1] + 0.5;
    let (xi, zi) = (x.floor() as i32, z.floor() as i32);
    while y > 0.0 && is_air(xi, y.floor() as i32, zi) {
        y -= 1.0;
    }
    [x, y.floor() + 1.0, z]
}

/// A puff: where it starts, and its velocity.
pub type Puff = ([f64; 3], [f64; 3]);

/// The head's centre, jittered by `nextGaussian() / 2` on each axis, in x, y, z order (`breath_puffs`, D:3439-3446).
fn jitter(head: [f64; 3], r: &mut LegacyRandom) -> [f64; 3] {
    [head[0] + r.next_gaussian() / 2.0, head[1] + r.next_gaussian() / 2.0, head[2] + r.next_gaussian() / 2.0]
}

/// The landing dragon's puffs of one tick (D:3450-3461): eight, along `-look * 0.08` (0.3 vertically) plus the dragon's motion.
pub fn landing_puffs(look: [f64; 3], head: [f64; 3], motion: [f64; 3], r: &mut LegacyRandom) -> Vec<Puff> {
    (0..8)
        .map(|_| {
            let v = [
                -look[0] * f64::from(0.08_f32) + motion[0],
                -look[1] * f64::from(0.3_f32) + motion[1],
                -look[2] * f64::from(0.08_f32) + motion[2],
            ];
            (jitter(head, r), v)
        })
        .collect()
}

/// The flaming dragon's puffs of one tick (D:3462-3481): on the even ticks before tick 10, eight starts each breathe a fan of
/// six puffs down the look line. `ticks` is the phase's counter, already advanced for the tick.
pub fn flame_puffs(ticks: u32, look: [f64; 3], head: [f64; 3], r: &mut LegacyRandom) -> Vec<Puff> {
    if ticks % 2 != 0 || ticks >= 10 {
        return Vec::new();
    }
    let mut puffs = Vec::with_capacity(48);
    for _ in 0..8 {
        let at = jitter(head, r);
        for j in 0..6 {
            let j = f64::from(j);
            puffs.push((
                at,
                [-look[0] * f64::from(0.08_f32) * j, -look[1] * f64::from(0.6_f32), -look[2] * f64::from(0.08_f32) * j],
            ));
        }
    }
    puffs
}

/// Spawns a cloud at its floor, and writes its `Cloud` host output.
pub fn spawn(w: &mut World, c: Cloud) -> Entity {
    let at = c.at;
    let (radius, duration) = (c.radius, c.duration);
    let e = w.spawn((Pos(Vec3::new(at[0], at[1], at[2])), c)).id();
    w.resource_mut::<Events>().host.cloud(at, radius, duration);
    e
}

/// Spawns the dragon's flame at `at`.
pub fn spawn_flame(w: &mut World, at: [f64; 3]) -> Entity {
    let e = spawn(w, FLAME.cloud(at));
    w.entity_mut(e).insert(Flame);
    e
}

/// Ends the flames (`DragonSittingFlamingPhase.end`).
pub fn end_flames(w: &mut World) {
    let ids: Vec<Entity> = w.query_filtered::<Entity, With<Flame>>().iter(w).collect();
    for e in ids {
        w.despawn(e);
    }
}

/// The cloud system: each cloud steps once per tick, against the players and creatures its box meets. Players it harms take the
/// instant damage. Not done here: the creatures' effect (`breath_affect_mob`) and the damage source `indirect_magic`.
pub fn tick_clouds(w: &mut World) {
    let ids: Vec<Entity> = w.query_filtered::<Entity, With<Cloud>>().iter(w).collect();
    for e in ids {
        let Some(mut c) = w.get::<Cloud>(e).cloned() else { continue };
        let reach = c.radius.max(c.radius + c.radius_per_tick);
        let (x, y, z) = (c.at[0], c.at[1], c.at[2]);
        let area = Aabb::new(Vec3::new(x - f64::from(reach), y, z - f64::from(reach)), Vec3::new(x + f64::from(reach), y + 0.5, z + f64::from(reach)));
        let mut cands = Vec::new();
        for id in area::players_in(w, area) {
            let who = Who::Player(id);
            if let Some(i) = targeting::info(w, who) {
                cands.push((who, i.bounds, i.pos));
            }
        }
        for m in area::creatures_in(w, area, e) {
            let who = Who::Entity(m);
            if let Some(i) = targeting::info(w, who) {
                cands.push((who, i.bounds, i.pos));
            }
        }
        let amount = harm(c.amplifier);
        match step(&mut c, &cands) {
            Step::Gone => {
                w.despawn(e);
            }
            Step::Live(hit) => {
                *w.get_mut::<Cloud>(e).expect("cloud") = c;
                for who in hit {
                    if let Who::Player(id) = who {
                        area::hurt_player(w, id, amount, e);
                    }
                }
            }
        }
    }
}
