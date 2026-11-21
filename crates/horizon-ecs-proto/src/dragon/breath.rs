//! The dragon's breath (Horizon `game/mobs/dragon.rs`: `Breath`, `breath_victims`, `breath_expired`, `harm_amount`, `cloud_hits`,
//! `breath_puffs`, and the flame's origin of `Phase::Flaming`, D:2563-2592). A breath is an area cloud: `Cloud` names no creature.
//! It is inert for `WAIT` ticks, then grows by `radius_per_tick`, harms what lies in it every 20 ticks (checked every 5) and goes
//! at its duration. The puffs are client particles: they come back as data, not emitted (see the notes).
use crate::area;
use crate::cow::Pos;
use crate::goals::Events;
use crate::rules::breath_cloud as cloud_rule;
use crate::targeting::{self, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

pub use crate::rules::dragon_breath::{due, expired, flame_origin, flame_puffs, harm, hits, landing_puffs, Puff, WAIT};

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
    let e = crate::breath_cloud::spawn_led(w, FLAME, at, 1);
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
        let area = cloud_rule::scan_box(c.at, cloud_rule::cloud_reach(c.radius, c.radius_per_tick));
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
