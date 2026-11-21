//! The firework rocket (`FireworkRocketEntity`), a projectile that is not a `Mob`. Launched, it rises and speeds up each tick (or keeps
//! its motion when shot at an angle), or follows the player who holds it while that player glides. It goes off where it hits a
//! creature or a player, or on a block when it carries explosions, or when its life is over: a wheel deadline, not a tick count.
//! The blast hurts the creatures and players within 5 blocks that it sees. Its own state is `Rocket`, its body and owner the
//! projectile's (`Projectile`); it is not visited by `projectile::fly`.
use crate::cow::*;
use crate::goals::{Events, Online, Player};
use crate::hostile::PlayerState;
use crate::projectile::{entity_hit, Projectile};
use crate::targeting::{player_box, Who};
use bevy_ecs::prelude::*;
use horizon_client_lib::arrows::clip_block_hit;
use horizon_client_render::collisions::SceneCollisions;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};
use std::collections::HashMap;

/// The box of a rocket: 0.25 wide and high.
const SIZE: f64 = 0.25;
/// `FireworkRocketEntity.explode` broadcasts this entity status before it is discarded.
const EXPLODE_EVENT: u8 = 17;

/// A rocket in the air: its explosions (their count sets the blast), the player who holds it (`attachedToEntity`), whether it was
/// shot at an angle, whether its last move was stopped sideways (`horizontalCollision`), whether it is silent, and whether its life
/// is over (set by its deadline; it goes off where it is).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Rocket {
    pub explosions: u8,
    pub carrier: Option<u32>,
    pub angled: bool,
    pub horizontal: bool,
    pub silent: bool,
    pub due: bool,
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
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(rockets.after(travel).before(reindex));
}

/// `FireworkRocketEntity`'s constructor, its draws in vanilla's order: the two triangles of the velocity (x, then z), then the two
/// `nextInt`s of the life. A shot at an angle keeps the velocity it is given.
pub fn launch(w: &mut World, s: Shot, random: &mut LegacyRandom) -> Entity {
    let drawn_x = random.triangle(0.0, 0.002297);
    let drawn_z = random.triangle(0.0, 0.002297);
    let lifetime = 10 * (1 + i32::from(s.flight)) + random.next_int_bound(6).unwrap_or(0) + random.next_int_bound(7).unwrap_or(0);
    let (velocity, angled) = match s.angled {
        Some(v) => (v, true),
        None => (Vec3::new(drawn_x, 0.05, drawn_z), false),
    };
    let rocket = Rocket { explosions: s.explosions, carrier: s.carrier, angled, horizontal: false, silent: s.silent, due: false };
    let e = w.spawn((Pos(s.at), Vel(velocity), Projectile { owner: s.owner, age: 0, left_owner: false, expires: 0 }, rocket, OwnTime))
.id();
    crate::nbt::mark_saved(w, e, crate::species::lists::FIREWORK_ROCKET);
    // `life > lifetime` after the tick's increment: the rocket goes off at the end of its tick `lifetime + 1`, which is the start of
    // the tick `lifetime + 2` for the wheel (the deadline runs before the flight of its tick).
    after(w, e, lifetime + 2, lifetime_over);
    e
}
/// The deadline of a rocket's life: it goes off where it is, at the start of its tick (it does not move that tick).
pub fn lifetime_over(w: &mut World, e: Entity) {
    if let Some(mut r) = w.get_mut::<Rocket>(e) {
        r.due = true;
    }
}

/// `Entity.move` box of a rocket at `at`.
fn box_at(at: Vec3) -> Aabb {
    let w = SIZE / 2.0;
    Aabb::new(Vec3::new(at.x - w, at.y, at.z - w), Vec3::new(at.x + w, at.y + SIZE, at.z + w))
}
/// `ServerPlayer.travel`'s boost of a glider: the holder's velocity, plus its look (0.1), plus the look at 1.5 minus the velocity,
/// half (`FireworkRocketEntity.tick`, attached to a gliding player).
fn glide_boost(v: Vec3, look: Vec3) -> Vec3 {
    let axis = |m: f64, l: f64| m + l * 0.1 + (l * 1.5 - m) * 0.5;
    Vec3::new(axis(v.x, look.x), axis(v.y, look.y), axis(v.z, look.z))
}
/// `dealExplosionDamage`'s amount for a target at `to`: `damage * sqrt((5 - distance) / 5)`, the distance in floats.
fn blast_damage(damage: f32, from: Vec3, to: Vec3) -> f32 {
    let (x, y, z) = ((from.x - to.x) as f32, (from.y - to.y) as f32, (from.z - to.z) as f32);
    let distance = f64::from(x * x + y * y + z * z).sqrt() as f32;
    damage * ((5.0 - f64::from(distance)) / 5.0).sqrt() as f32
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
    let margin = f64::from(((age as i32 - 2) as f32 / 20.0).clamp(0.0, 0.3));
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

/// `FireworkRocketEntity.dealExplosionDamage`: `5 + 2 * explosions` to the holder, then to each other creature and player within 5
/// blocks of the blast that sees its feet or its middle; each takes `blast_damage`.
#[allow(clippy::too_many_arguments)]
fn detonate(
    level: &Level,
    idx: &Index,
    on: &Online,
    creatures: &Query<&Bounds, With<Health>>,
    ps: &mut PlayerState,
    dq: &mut DamageQueue,
    now: u64,
    at: Vec3,
    owner: Option<Who>,
    holder: Option<u32>,
    explosions: u8,
) {
    let damage = 5.0 + f32::from(explosions) * 2.0;
    let from = (at.x, at.z);
    if let Some(id) = holder {
        ps.hurt(now, id, damage, from, owner);
    }
    let mut near = Vec::new();
    idx.for_radius(at.x, at.z, 6.0, |c| near.push(c));
    for c in near {
        let Ok(b) = creatures.get(c) else { continue };
        let feet = b.0.center();
        let feet = Vec3::new(feet.x, b.0.min_position().y, feet.z);
        let height = b.0.max_position().y - b.0.min_position().y;
        if let Some(amount) = exposure(level, at, feet, height, damage) {
            match owner {
                Some(o) => dq.add_via(c, amount, from, o),
                None => dq.add_from(c, amount, Cause::Entity, from),
            }
        }
    }
    let mut players: Vec<Player> = Vec::new();
    on.for_near(at.x, at.z, 6.0, |pl| players.push(pl));
    for pl in players {
        if Some(pl.id) == holder {
            continue;
        }
        let feet = Vec3::new(pl.x, pl.y, pl.z);
        let body = player_box(&pl);
        let height = body.max_position().y - body.min_position().y;
        if let Some(amount) = exposure(level, at, feet, height, damage) {
            ps.hurt(now, pl.id, amount, from, owner);
        }
    }
}
/// The blast's reach to a target with its feet at `feet`: within 5 blocks, and seen from the blast at its feet or its middle
/// (`ClipContext` collider, no fluids).
fn exposure(level: &Level, from: Vec3, feet: Vec3, height: f64, damage: f32) -> Option<f32> {
    let d2 = (feet.x - from.x).powi(2) + (feet.y - from.y).powi(2) + (feet.z - from.z).powi(2);
    if d2 > 25.0 {
        return None;
    }
    let mw = level.mw.0;
    let sees = (0..2).any(|step| {
        let to = Vec3::new(feet.x, feet.y + height * 0.5 * f64::from(step), feet.z);
        clip_block_hit(mw.world, mw.facts, mw.collision, from, Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z)).is_none()
    });
    sees.then(|| blast_damage(damage, from, feet))
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
    clock: Res<Clock>,
    level: Res<Level>,
    idx: Res<Index>,
    on: Res<Online>,
    mut ps: ResMut<PlayerState>,
    mut dq: ResMut<DamageQueue>,
    mut events: ResMut<Events>,
    mut gliders: ResMut<Gliders>,
    creatures: Query<&Bounds, With<Health>>,
    mut q: Query<(Entity, &mut Pos, &mut Vel, &mut Projectile, &mut Rocket)>,
    mut commands: Commands,
) {
    let mw = level.mw.0;
    let now = clock.0;
    for (e, mut pos, mut vel, mut p, mut r) in &mut q {
        p.age += 1;
        if p.age == 1 && !r.silent {
            events.host.sound("entity.firework_rocket.launch", [pos.0.x, pos.0.y, pos.0.z], 3.0, 1.0);
        }
        let holder = r.carrier.filter(|id| gliders.0.contains_key(id));
        if r.due {
            events.host.entity_event(e, EXPLODE_EVENT);
            detonate(&level, &idx, &on, &creatures, &mut ps, &mut dq, now, pos.0, p.owner, holder, r.explosions);
            commands.entity(e).despawn();
            continue;
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
                let f = if r.horizontal { 1.0 } else { 1.15 };
                Vec3::new(vel.0.x * f, vel.0.y + 0.04, vel.0.z * f)
            };
            let (who, block) = struck(&level, &idx, &on, &creatures, pos.0, motion, p.age, skip);
            let scene = SceneCollisions { world: mw.world, binding: mw.collision, subject: None };
            let moved = level.occ.resolve(scene, box_at(pos.0), motion, 0.0, false).unwrap_or(Vec3::ZERO);
            let differs = |a: f64, b: f64| (b - a).abs() >= f64::from(1.0e-5_f32);
            r.horizontal = differs(motion.x, moved.x) || differs(motion.z, moved.z);
            pos.0 = Vec3::new(pos.0.x + moved.x, pos.0.y + moved.y, pos.0.z + moved.z);
            (who, block, motion)
        };
        // The velocity after the move is the one before it (`setDeltaMovement(motion)`, whatever the move stopped).
        vel.0 = motion;
        let goes_off = who.is_some() || (block && r.explosions > 0);
        if goes_off {
            events.host.entity_event(e, EXPLODE_EVENT);
            detonate(&level, &idx, &on, &creatures, &mut ps, &mut dq, now, pos.0, p.owner, holder, r.explosions);
            commands.entity(e).despawn();
        }
    }
}
