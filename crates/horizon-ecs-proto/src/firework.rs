//! The firework rocket (`FireworkRocketEntity`), a projectile that is not a `Mob`. Launched, it rises and speeds up each tick (or keeps
//! its motion when shot at an angle), or follows the player who holds it while that player glides. It goes off where it hits a
//! creature or a player, or on a block when it carries explosions, or when its life is over: a wheel deadline, not a tick count.
//! The blast hurts the creatures and players within 5 blocks that it sees. Its own state is `Rocket`, its body and owner the
//! projectile's (`Projectile`); it is not visited by `projectile::fly`.
use crate::cow::*;
use crate::firework_burst::{rocket_end, FireworkOut};
use crate::goals::{Online, Player};
use crate::host_out_firework::FireworkEvent;
use crate::projectile::{entity_hit, Projectile};
use crate::rules::firework::{box_at, detonation_damage, exposure, glide_boost, hit_margin, is_sideways_stop, life_ticks, rise_motion};
use crate::targeting::{player_box, Who};
use bevy_ecs::prelude::*;
use horizon_client_lib::arrows::clip_block_hit;
use horizon_client_render::collisions::SceneCollisions;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};
use std::collections::HashMap;

/// `FireworkRocketEntity.explode` broadcasts this entity status before it is discarded.
pub const EXPLODE_EVENT: u8 = 17;

/// A rocket in the air: its explosions (their count sets the blast), its flight (1 to 3) and its life in ticks (`LifeTime`), the player
/// who holds it (`attachedToEntity`), whether it was shot at an angle, whether its last move was stopped sideways
/// (`horizontalCollision`), and whether it is silent.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Rocket {
    pub explosions: u8,
    pub flight: u8,
    pub lifetime: i32,
    pub carrier: Option<u32>,
    pub angled: bool,
    pub horizontal: bool,
    pub silent: bool,
}

/// The glide of a player who holds a rocket, indexed by the player's id (an assumed player component): the velocity and the view
/// (`getLookAngle`) that the boost reads, the hand's offset (`getHandHoldingItemAngle`), and whether the player is fall-flying.
#[derive(Clone, Copy, Debug)]
pub struct Glider {
    pub velocity: Vec3,
    pub look: Vec3,
    pub hand: Vec3,
    pub fall_flying: bool,
}
/// The gliding players, by id (written by the player's state, read here).
#[derive(Resource, Default)]
pub struct Gliders(pub HashMap<u32, Glider>);

/// What a rocket is launched with: where, who shot it, its flight (1 to 3), its explosions, whether it is silent, the player who
/// holds it (none for a free rocket), and its velocity when it is shot at an angle (none: it rises from its random velocity).
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub at: Vec3,
    pub owner: Option<Who>,
    pub flight: u8,
    pub explosions: u8,
    pub silent: bool,
    pub carrier: Option<u32>,
    pub angled: Option<Vec3>,
}

pub fn insert_resources(w: &mut World) {
    w.insert_resource(Gliders::default());
    w.insert_resource(FireworkOut::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(rockets.after(travel).before(reindex));
}

/// `FireworkRocketEntity`'s constructor, its draws in vanilla's order: the two triangles of the velocity (x, then z), then the two
/// `nextInt`s of the life. A shot at an angle keeps the velocity it is given.
pub fn launch(w: &mut World, s: Shot) -> Entity {
    // The seed mixes the tick and the live entities, so two rockets launched in one tick draw differently.
    let seed = (w.resource::<Clock>().0 as i64).wrapping_mul(0x9E37_79B9) ^ i64::from(w.entities().len());
    let mut random = LegacyRandom::new(seed);
    let drawn_x = random.triangle(0.0, 0.002297);
    let drawn_z = random.triangle(0.0, 0.002297);
    let lifetime = life_ticks(s.flight, random.next_int_bound(6).unwrap_or(0), random.next_int_bound(7).unwrap_or(0));
    let (velocity, angled) = match s.angled {
        Some(v) => (v, true),
        None => (Vec3::new(drawn_x, 0.05, drawn_z), false),
    };
    let rocket = Rocket { explosions: s.explosions, flight: s.flight, lifetime, carrier: s.carrier, angled, horizontal: false, silent: s.silent };
    let e = w.spawn((Pos(s.at), Vel(velocity), Projectile { owner: s.owner, age: 0, left_owner: false, expires: 0 }, rocket, OwnTime))
.id();
    crate::nbt::mark_saved(w, e, crate::species::lists::FIREWORK_ROCKET);
    // `life > lifetime` after the tick's increment: the rocket goes off at the end of its tick `lifetime + 1`, which is the start of
    // the tick `lifetime + 2` for the wheel (the deadline runs before the flight of its tick).
    after(w, e, lifetime + 2, rocket_end);
    e
}

/// The first creature or player the path from `from` to `motion` meets, before the block it stops at (`ProjectileUtil`'s entity
/// hit, the box grown by the rocket's age), and whether a block stops it. The owner is spared until it has left it.
#[allow(clippy::too_many_arguments)]
fn struck(
    level: &Level,
    idx: &Index,
    on: &Online,
    creatures: &Query<&Bounds, With<Health>>,
    from: Vec3,
    motion: Vec3,
    age: u32,
    skip: Option<Who>,
) -> (Option<Who>, bool) {
    let mw = level.mw.0;
    let block = clip_block_hit(mw.world, mw.facts, mw.collision, from, motion);
    let stop = block.map_or(Vec3::new(from.x + motion.x, from.y + motion.y, from.z + motion.z), |b| b.0);
    let margin = hit_margin(age);
    let area = box_at(from).expand_towards(motion).inflate(Vec3::new(1.0, 1.0, 1.0));
    let mut best: Option<(Who, f64)> = None;
    let mut consider = |who: Who, b: Aabb| {
        if Some(who) == skip || !b.intersects(area) {
            return;
        }
        if let Some(at) = entity_hit(level, b, from, stop, margin) {
            let d = from.distance_squared(at);
            if best.is_none_or(|x| d < x.1) {
                best = Some((who, d));
            }
        }
    };
    let reach = motion.length() + 2.0;
    idx.for_radius(from.x, from.z, reach, |c| {
        if let Ok(b) = creatures.get(c) {
            consider(Who::Entity(c), b.0);
        }
    });
    on.for_near(from.x, from.z, reach, |pl| consider(Who::Player(pl.id), player_box(&pl)));
    (best.map(|b| b.0), block.is_some())
}

/// One tick of every rocket: the sound at its first tick, its deadline, its holder's boost, its speed-up and rise, its move, and
/// the impact that makes it go off (an entity, or a block when it carries explosions).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
/// Nothing to run without a rocket in flight: the system (a dozen parameters to fetch) is run only then.
fn rockets(w: &mut World) {
    if crate::goals::sparse_any::<Rocket>(w) {
        let _ = w.run_system_cached(fly_rockets);
    }
}
pub fn fly_rockets(
    level: Res<Level>,
    idx: Res<Index>,
    on: Res<Online>,
    mut out: ResMut<FireworkOut>,
    mut gliders: ResMut<Gliders>,
    creatures: Query<&Bounds, With<Health>>,
    mut q: Query<(Entity, &mut Pos, &mut Vel, &mut Projectile, &mut Rocket)>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    for (e, mut pos, mut vel, mut p, mut r) in &mut q {
        p.age += 1;
        if p.age == 1 && !r.silent {
            out.0.push(FireworkEvent::Launched { at: [pos.0.x, pos.0.y, pos.0.z] });
        }
        // `Projectile.checkLeftOwner`: out of the owner's box once the box, swept by its velocity and grown by 1, no longer touches it.
        if !p.left_owner {
            let owner_box = match p.owner {
                Some(Who::Entity(o)) => creatures.get(o).ok().map(|b| b.0),
                Some(Who::Player(id)) => on.get(id).map(|pl| player_box(&pl)),
                None => None,
            };
            let swept = box_at(pos.0).expand_towards(vel.0).inflate(Vec3::new(1.0, 1.0, 1.0));
            p.left_owner = owner_box.is_none_or(|b| !swept.intersects(b));
        }
        let skip = p.owner.filter(|_| !p.left_owner);
        let holding = r.carrier.and_then(|id| on.get(id).map(|pl| (id, pl))).filter(|(id, _)| gliders.0.contains_key(id));
        let glider = holding.and_then(|(id, pl)| gliders.0.get_mut(&id).map(|g| (g, pl)));
        let (who, block, motion) = if let Some((g, pl)) = glider {
            // `FireworkRocketEntity.tick` attached: the boost while fall-flying (else no hand offset), then the holder's motion.
            let hand = if g.fall_flying {
                g.velocity = glide_boost(g.velocity, g.look);
                g.hand
            } else {
                Vec3::ZERO
            };
            pos.0 = Vec3::new(pl.x + hand.x, pl.y + hand.y, pl.z + hand.z);
            let motion = g.velocity;
            let (who, block) = struck(&level, &idx, &on, &creatures, pos.0, motion, p.age, skip);
            (who, block, motion)
        } else {
            // A rocket shot at an angle keeps its motion; else it speeds up (1.15 sideways, 1.0 after a sideways stop) and rises 0.04.
            let motion = if r.angled {
                vel.0
            } else {
                rise_motion(vel.0, r.horizontal)
            };
            let (who, block) = struck(&level, &idx, &on, &creatures, pos.0, motion, p.age, skip);
            let scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
            let moved = level.occ.resolve(scene, box_at(pos.0), motion, 0.0, false).unwrap_or(Vec3::ZERO);
            r.horizontal = is_sideways_stop(motion, moved);
            pos.0 = Vec3::new(pos.0.x + moved.x, pos.0.y + moved.y, pos.0.z + moved.z);
            (who, block, motion)
        };
        // The velocity after the move is the one before it (`setDeltaMovement(motion)`, whatever the move stopped).
        vel.0 = motion;
        let goes_off = who.is_some() || (block && r.explosions > 0);
        if goes_off {
            commands.queue(move |w: &mut World| crate::firework_burst::go_off(w, e));
        }
    }
}
