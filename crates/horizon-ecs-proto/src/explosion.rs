//! The explosion capability (`ServerExplosion`): `explode(w, blast)` at a centre with a radius: the creatures within twice the radius take
//! `(pow^2 + pow) / 2 * 7 * diameter + 1` with `pow = (1 - distance / diameter) * exposure` (the share of the rays from a grid over their box
//! that reach the centre unblocked) and are pushed away by `pow`; the blocks on 1 352 rays from the centre are broken while the ray's power
//! (`radius * (0.7 + 0.6 * random)`, less `(resistance + 0.3) * 0.3` and 0.225 per 0.3-block step) lasts; with fire, a third of them that
//! have air and a floor take fire. The blocks are asked as `BlockChanges` (applied at the next step). Boats and carts are hurt and pushed, the
//! other entities without health are pushed (`blast_others`). Not done: the items' and orbs' damage, the redirected projectiles' owner, the players' push, enchantment protections, the sounds and particles, the other blocks' own `onExplosionHit`.
//! A creeper's fuse (`Fuse`) is a deadline of the wheel (`Due::Fuse`), armed when its direction turns to +1: no tick per creeper.
use crate::cow::*;
use crate::goals::mob_griefing;
use crate::projectile::Blast;
use crate::targeting::{player_box, Who};
use bevy_ecs::prelude::*;
use crate::rules::explosion::{charged_radius, distance_share, fuse_arm_tick, fuse_value, push_power, seen_share};
use horizon_core::geometry::{Aabb, Vec3};
pub use crate::rules::explosion::blast_damage;

#[derive(Resource)]
pub struct ExplosionRng(pub Rng);
pub fn insert_resources(w: &mut World) {
    w.insert_resource(ExplosionRng(Rng(0x2545_F491_4F6C_DD1D)));
}

/// `ServerExplosion.getSeenPercent` for a box: the share of the grid's rays to the centre that no block collider stops.
pub fn seen_percent(level: &Level, center: Vec3, b: Aabb) -> f32 {
    seen_share(level.mw.0, center, b)
}
/// `ServerExplosion.calculateExplodedPositions`.
fn exploded_positions(w: &mut World, c: Vec3, radius: f32, skull: bool) -> Vec<[i32; 3]> {
    let level = *w.resource::<Level>();
    let mw = level.mw.0;
    let (min_y, max_y) = (mw.world.bottom_y(), mw.world.top_y());
    let mut rng = w.resource::<ExplosionRng>().0;
    let mut set: Vec<[i32; 3]> = Vec::new();
    // The tag answer of each block state met (the same few states along the 1352 rays).
    let mut immune: Vec<(u32, bool)> = Vec::new();
    let mut seen: [Option<([i32; 3], horizon_world::registry::BlockStateId)>; 64] = [None; 64];
    for xx in 0..16 {
        for yy in 0..16 {
            for zz in 0..16 {
                if !(xx == 0 || xx == 15 || yy == 0 || yy == 15 || zz == 0 || zz == 15) {
                    continue;
                }
                let (mut xd, mut yd, mut zd) = (f64::from(xx as f32 / 15.0 * 2.0 - 1.0), f64::from(yy as f32 / 15.0 * 2.0 - 1.0), f64::from(zz as f32 / 15.0 * 2.0 - 1.0));
                let d = (xd * xd + yd * yd + zd * zd).sqrt();
                (xd, yd, zd) = (xd / d, yd / d, zd / d);
                let mut power = radius * (0.7 + rng.next_float() * 0.6);
                let (mut xp, mut yp, mut zp) = (c.x, c.y, c.z);
                while power > 0.0 {
                    let pos = [xp.floor() as i32, yp.floor() as i32, zp.floor() as i32];
                    if pos[1] < min_y || pos[1] >= max_y {
                        break;
                    }
                    // The rays cross the same few blocks: a direct-mapped table of the states read in this blast.
                    let slot = &mut seen[((pos[0] as u32).wrapping_mul(73) ^ (pos[1] as u32).wrapping_mul(179) ^ (pos[2] as u32).wrapping_mul(283)) as usize & 63];
                    let state = match *slot {
                        Some((p, s)) if p == pos => s,
                        _ => {
                            let s = mw.world.block(pos[0], pos[1], pos[2]);
                            *slot = Some((pos, s));
                            s
                        }
                    };
                    let f = mw.facts.get(state);
                    let wet = f.fluid != myriade_engine_lib::blocks::Fluid::None;
                    let fluid = if wet { 100.0_f32 } else { 0.0 };
                    let air = mw.facts.is_air(state);
                    if !air || wet {
                        let mut res = f.explosion_resistance.max(fluid);
                        // `WitherSkull.getBlockExplosionResistance`: a blue skull's blast gives the wither-destroyable blocks at most 0.8.
                        if skull && !air && !(match immune.iter().find(|t| t.0 == state.0) {
                            Some(t) => t.1,
                            None => {
                                let v = crate::level::has_block_tag(w, "minecraft:wither_immune", pos);
                                immune.push((state.0, v));
                                v
                            }
                        }) {
                            res = res.min(0.8);
                        }
                        power -= (res + 0.3) * 0.3;
                    }
                    if power > 0.0 && (!air || wet) && !set.contains(&pos) {
                        set.push(pos);
                    }
                    xp += xd * f64::from(0.3_f32);
                    yp += yd * f64::from(0.3_f32);
                    zp += zd * f64::from(0.3_f32);
                    power -= 0.225_000_01;
                }
            }
        }
    }
    w.resource_mut::<ExplosionRng>().0 = rng;
    set
}
/// The explosion: the creatures, then the blocks (and their fire).
/// A wind charge's burst (`ServerExplosion` with `TRIGGER_BLOCK`): no damage, no block; every creature within the diameter is pushed
/// away from the centre by `(1 - distance / diameter) * exposure`, its distance from its feet and its direction from its eyes, as
/// `Entity.pushFromExplosion`. Players are the host's (their push is not in the state of the frame); blocks that react (doors, levers) are not ported.
pub fn wind_burst(w: &mut World, at: Vec3, radius: f32, owner: Option<crate::targeting::Who>) {
    let level = *w.resource::<Level>();
    let diameter = f64::from(radius * 2.0);
    let mut near: Vec<Entity> = Vec::new();
    w.resource::<Index>().for_radius(at.x, at.z, diameter, |c| near.push(c));
    for c in near {
        // A breeze's charge does not push its own owner's kind: the owner is spared (`Breeze.isInvulnerableTo` covers the damage).
        if owner == Some(crate::targeting::Who::Entity(c)) {
            continue;
        }
        let (Some(feet), Some(bounds)) = (w.get::<Pos>(c).map(|p| p.0), w.get::<Bounds>(c).map(|x| x.0)) else { continue };
        let dist = distance_share(feet, at, diameter);
        // `ItemEntity.ignoreExplosion`: a wind burst does not affect the blocklike entities, an item is one.
        let vehicle = is_vehicle(w, c);
        if dist > 1.0 || w.get::<crate::item::Item>(c).is_some() || (w.get::<Vel>(c).is_none() && !vehicle) {
            continue;
        }
        let exposure = seen_percent(&level, at, bounds);
        let eye = feet.y + eye_offset(w, c, bounds);
        let (dx, dy, dz) = (feet.x - at.x, eye - at.y, feet.z - at.z);
        let l = (dx * dx + dy * dy + dz * dz).sqrt();
        let dir = if l < 1.0e-5 { Vec3::ZERO } else { Vec3::new(dx / l, dy / l, dz / l) };
        let pow = push_power(dist, exposure);
        if vehicle {
            push_vehicle(w, c, dir, pow);
            continue;
        }
        if let Some(mut v) = w.get_mut::<Vel>(c) {
            v.0 = Vec3::new(v.0.x + dir.x * pow, v.0.y + dir.y * pow, v.0.z + dir.z * pow);
        }
        if let Ok(mut m) = w.get_entity_mut(c) {
            m.insert(Moving);
        }
        crate::stand::unsettle(w, c);
    }
}
fn is_vehicle(w: &World, c: Entity) -> bool {
    w.get::<crate::boat_state::BoatMarker>(c).is_some() || w.get::<crate::cart_state::Cart>(c).is_some()
}
/// Where an entity's eyes are above its feet: its `Eye`, else `EntityDimensions`' 0.85 of the box height (jar: no boat or cart overrides it).
fn eye_offset(w: &World, c: Entity, bounds: Aabb) -> f64 {
    w.get::<crate::targeting::Eye>(c).map_or((bounds.max_position().y - bounds.min_position().y) * EYE_SHARE, |e| e.0)
}
/// `Entity.push` on a vehicle: a cart at rest has no velocity yet, the push gives it one.
fn push_vehicle(w: &mut World, c: Entity, dir: Vec3, pow: f64) {
    if w.get_entity(c).is_ok() && w.get::<Vel>(c).is_none() {
        w.entity_mut(c).insert(Vel(Vec3::ZERO));
    }
    if w.get_entity(c).is_ok() {
        crate::vehicle_push::push(w, c, [dir.x * pow, dir.y * pow, dir.z * pow]);
    }
}
/// The item entities an explosion's damage spares though it pushes them: the host lists those whose stack `canBeHurtBy` refuses the
/// explosion (its data is the host's); `explode_full` takes the list away.
#[derive(Resource, Default)]
pub struct ExplosionSpared(pub Vec<Entity>);
/// The projectiles an explosion redirects (`REDIRECTABLE_PROJECTILE`): their owner becomes the explosion's causing entity.
const REDIRECTABLE: [&str; 3] = ["minecraft:fireball", crate::species::lists::WIND_CHARGE, "minecraft:breeze_wind_charge"];
/// The share of its box height where an entity's eyes are, as `EntityDimensions.scalable` sets them (jar: 0.85).
const EYE_SHARE: f64 = 0.85;
/// `ServerExplosion.hurtEntities` for what has no health: every entity of the box is pushed by `pushFromExplosion`; a boat or a cart is
/// hurt first (`VehicleEntity.hurtServer`, a TNT cart primes: `MinecartTNT.destroy`) unless `VehicleEntity.ignoreExplosion` spares it
/// (the indirect source is a mob and mob griefing is off). `others` is the snapshot of the entities the index listed: one a hit removed
/// is skipped, the rest are all visited. Not done: the items' and orbs' damage, the redirected projectiles' owner.
fn blast_others(w: &mut World, b: &Blast, others: &[Entity], diameter: f64) {
    let level = *w.resource::<Level>();
    let indirect = b.owner.or(b.source.map(Who::Entity));
    let mob_source = matches!(indirect, Some(Who::Entity(m)) if w.get::<Health>(m).is_some());
    let spare_vehicles = mob_source && !mob_griefing(w);
    let drops = w.get_resource::<crate::boat_hurt::EntityDrops>().is_none_or(|d| d.0);
    for &c in others {
        let boat = w.get::<crate::boat_state::BoatMarker>(c).is_some();
        let vehicle = boat || w.get::<crate::cart_state::Cart>(c).is_some();
        if vehicle && spare_vehicles {
            continue;
        }
        let (Some(feet), Some(bounds)) = (w.get::<Pos>(c).map(|p| p.0), w.get::<Bounds>(c).map(|x| x.0)) else { continue };
        if !vehicle && w.get::<Vel>(c).is_none() {
            continue;
        }
        let dist = distance_share(feet, b.at, diameter);
        if dist > 1.0 {
            continue;
        }
        let exposure = seen_percent(&level, b.at, bounds);
        // A primed TNT is aimed at from its position, the rest from their eyes.
        let up = if w.get::<crate::tnt::Primed>(c).is_some() { 0.0 } else { eye_offset(w, c, bounds) };
        let (dx, dy, dz) = (feet.x - b.at.x, feet.y + up - b.at.y, feet.z - b.at.z);
        let l = (dx * dx + dy * dy + dz * dz).sqrt();
        let dir = if l < 1.0e-4 { Vec3::ZERO } else { Vec3::new(dx / l, dy / l, dz / l) };
        let pow = push_power(dist, exposure);
        // `Projectile.setOwner(damageSource.getEntity())` for a redirectable projectile.
        if w.get::<crate::projectile::Flight>(c).is_some_and(|f| REDIRECTABLE.contains(&f.id)) {
            if let Some(mut p) = w.get_mut::<crate::projectile::Projectile>(c) {
                p.owner = indirect;
            }
        }
        if vehicle {
            let amount = blast_damage(dist, exposure, diameter);
            if boat {
                // A host that runs the mods' `vehicle_damage` hook takes the blow and plays it through its ordinary hit (`BoatBlows`).
                let routed = w.get_resource::<crate::boat_hurt::BoatBlows>().is_some_and(|q| q.host);
                if routed {
                    w.resource_mut::<crate::boat_hurt::BoatBlows>().queue.push((c, amount, indirect));
                } else {
                    let flags = crate::boat_hurt::HurtFlags { invulnerable: false, entity_drops: drops };
                    crate::boat_hurt::hit(w, c, amount, indirect, false, flags);
                }
            } else {
                crate::cart_variants::hit_cart_blast(w, c, amount, indirect, |w| {
                    let mut rng = w.resource::<ExplosionRng>().0;
                    let fuse = rng.next_int(TNT_CART_FUSE_DRAW) + rng.next_int(TNT_CART_FUSE_DRAW);
                    w.resource_mut::<ExplosionRng>().0 = rng;
                    fuse
                });
            }
            push_vehicle(w, c, dir, pow);
            continue;
        }
        if w.get_entity(c).is_ok() {
            crate::vehicle_push::push(w, c, [dir.x * pow, dir.y * pow, dir.z * pow]);
        }
    }
}
/// `MinecartTNT.destroy`'s short fuse: two draws of `nextInt(20)` added (jar: bound 20).
const TNT_CART_FUSE_DRAW: i32 = 20;

pub fn explode(w: &mut World, b: Blast) {
    let griefs = mob_griefing(w);
    explode_with(w, b, griefs);
}
/// A primed TNT's blast (`ExplosionInteraction.TNT`): its blocks break whatever the mob griefing rule says.
pub fn explode_tnt(w: &mut World, b: Blast) {
    explode_with(w, b, true);
}
/// `explode` with `breaks_blocks` false for `ExplosionInteraction.NONE` / `BlockInteraction.KEEP`: the creatures are hurt, the blocks stay.
pub fn explode_with(w: &mut World, b: Blast, breaks_blocks: bool) {
    explode_full(w, b, breaks_blocks, false);
}
/// A wither skull's blast (`level.explode` with power 1, no fire); a blue one (`dangerous`) caps the blocks' resistance at 0.8.
pub fn explode_wither(w: &mut World, b: Blast, dangerous: bool) {
    let griefs = mob_griefing(w);
    explode_full(w, b, griefs, dangerous);
}
fn explode_full(w: &mut World, b: Blast, breaks_blocks: bool, skull: bool) {
    if b.radius < 1.0e-5 {
        return;
    }
    crate::hanging::blast(w, b.at, b.radius, b.source.map(Who::Entity), breaks_blocks);
    let level = *w.resource::<Level>();
    let diameter = f64::from(b.radius * 2.0);
    // The creatures around.
    let mut near: Vec<Entity> = Vec::new();
    let mut others: Vec<Entity> = Vec::new();
    let indirect = b.owner.or(b.source.map(Who::Entity));
    let mob_source = matches!(indirect, Some(Who::Entity(m)) if w.get::<Health>(m).is_some());
    let griefs = mob_griefing(w);
    w.resource::<Index>().for_radius(b.at.x, b.at.z, diameter, |c| near.push(c));
    for c in near {
        // `ArmorStand.ignoreExplosion`: an invisible stand is no target.
        let spared = w.get::<crate::stand::Stand>(c).is_some_and(|s| crate::stand::ignores_explosion(*s));
        // `Warden.ignoreExplosion`: `isDiggingOrEmerging`.
        let buried = w.get::<crate::behaviours::act_phase::ActPhase>(c).is_some_and(|a| matches!(a.phase, crate::behaviours::act_phase::Phase::Burrowing | crate::behaviours::act_phase::Phase::Emerging));
        if Some(c) == b.source || spared || buried {
            continue;
        }
        if w.get::<Health>(c).is_none() {
            others.push(c);
            continue;
        }
        let (Some(feet), Some(bounds)) = (w.get::<Pos>(c).map(|p| p.0), w.get::<Bounds>(c).map(|x| x.0)) else { continue };
        let dist = distance_share(feet, b.at, diameter);
        if dist > 1.0 {
            continue;
        }
        let exposure = seen_percent(&level, b.at, bounds);
        let eye = feet.y + w.get::<crate::targeting::Eye>(c).map_or(0.0, |e| e.0);
        let (dx, dy, dz) = (feet.x - b.at.x, eye - b.at.y, feet.z - b.at.z);
        let l = (dx * dx + dy * dy + dz * dz).sqrt();
        let dir = if l < 1.0e-4 { Vec3::ZERO } else { Vec3::new(dx / l, dy / l, dz / l) };
        // `ServerExplosion.hurtEntities`: the push is scaled by 1 minus the entity's explosion knockback resistance (0 for a non-living).
        let pow = push_power(dist, exposure) * (1.0 - crate::attr_mod::read(w, c, "explosion_knockback_resistance", 0.0).clamp(0.0, 1.0));
        // `ItemEntity.hurtServer`: no damage when a mob blasts with mob griefing off, nor to a stack that resists the blast; still pushed.
        let item = w.get::<crate::item::Item>(c).is_some();
        let spared_item = item && ((mob_source && !griefs) || w.get::<crate::item::BlastResist>(c).is_some() || w.get_resource::<ExplosionSpared>().is_some_and(|s| s.0.contains(&c)));
        if !spared_item {
            w.resource_mut::<DamageQueue>().add_blast(c, blast_damage(dist, exposure, diameter), (b.at.x, b.at.z), b.source.is_some());
        }
        if let Some(mut v) = w.get_mut::<Vel>(c) {
            v.0 = Vec3::new(v.0.x + dir.x * pow, v.0.y + dir.y * pow, v.0.z + dir.z * pow);
        }
        if let Ok(mut m) = w.get_entity_mut(c) {
            m.insert(Moving);
        }
        crate::stand::unsettle(w, c);
    }
    blast_others(w, &b, &others, diameter);
    w.remove_resource::<ExplosionSpared>();
    // The players around.
    let tick = w.resource::<Clock>().0;
    let players: Vec<(u32, Aabb)> = {
        let on = w.resource::<crate::goals::Online>();
        let mut v = Vec::new();
        on.for_near(b.at.x, b.at.z, diameter + 2.0, |p| v.push((p.id, player_box(&p))));
        v
    };
    for (id, bx) in players {
        // `Entity.position()`: the middle of the box's bottom, not its corner.
        let feet = Vec3::new((bx.min_position().x + bx.max_position().x) / 2.0, bx.min_position().y, (bx.min_position().z + bx.max_position().z) / 2.0);
        let dist = distance_share(feet, b.at, diameter);
        if dist > 1.0 {
            continue;
        }
        let exposure = seen_percent(&level, b.at, bx);
        let amount = blast_damage(dist, exposure, diameter);
        let by = b.source.map(Who::Entity);
        w.resource_scope(|_, mut ps: Mut<crate::hostile::PlayerState>| ps.hurt(tick, id, amount, (b.at.x, b.at.z), by));
    }
    // The blocks.
    if breaks_blocks {
        let blocks = exploded_positions(w, b.at, b.radius, skull);
        let mw = level.mw.0;
        let tnt = crate::blocks::resolve(mw, crate::species::tnt::BLOCK, &[]);
        let mut rng = w.resource::<ExplosionRng>().0;
        for p in &blocks {
            // `TntBlock.wasExploded`: a TNT block the blast destroys is primed in its place.
            if Some(mw.world.block(p[0], p[1], p[2])) == tnt {
                crate::tnt::spawn_from_blast_by(w, *p, &mut rng, b.owner);
            }
            w.resource_mut::<BlockChanges>().0.push((p[0], p[1], p[2], "minecraft:air"));
        }
        if b.fire {
            for p in &blocks {
                if rng.next_int(3) == 0 && mw.facts.is_air(mw.world.block(p[0], p[1], p[2])) && mw.facts.get(mw.world.block(p[0], p[1] - 1, p[2])).solid_render {
                    w.resource_mut::<BlockChanges>().0.push((p[0], p[1], p[2], "minecraft:fire"));
                }
            }
        }
        w.resource_mut::<ExplosionRng>().0 = rng;
    }
}

// ---- the fuse of a creature that swells -----------------------------------------------------------------------------
/// A creeper's fuse: the swell grows by one a tick while the direction is +1 and falls by one while it is -1, from `value` at `at`; at
/// `max` it explodes. The wheel holds the tick it is due at (`armed`), none while it falls.
#[derive(Component, Clone, Copy, Debug)]
#[component(on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Fuse {
    pub value: i32,
    pub at: u64,
    pub dir: i8,
    pub max: i32,
    pub radius: f32,
    pub armed: u64,
    /// Lit by an outside hand (flint and steel): the direction is +1 whatever the target does.
    pub ignited: bool,
}
/// A creature struck by lightning: its blast is twice the radius.
#[derive(Component, Clone, Copy, Debug)]
#[component(on_insert = crate::bridge::view::touched, on_remove = crate::bridge::view::touched)]
pub struct Charged;
impl Fuse {
    pub fn value_at(&self, now: u64) -> i32 {
        fuse_value(self.value, self.dir, self.at, now, self.max)
    }
}
/// The swell direction changed (`Swell.tick` sets +1 or -1): the fuse is brought up to now and the deadline is armed or dropped.
pub fn set_dir(w: &mut World, e: Entity, dir: i8) {
    let now = now_of(w, e);
    let Some(mut f) = w.get::<Fuse>(e).copied() else { return };
    let eff = if f.ignited { 1 } else { dir };
    if eff == f.dir && (eff <= 0 || f.armed != 0) {
        return;
    }
    f.value = f.value_at(now);
    f.at = now;
    f.dir = eff;
    // The creeper's own tick comes before its goals' (`pre_tick`): the direction written now counts from the next tick.
    f.armed = 0;
    if f.dir > 0 {
        let due = fuse_arm_tick(now, f.value, f.max);
        f.armed = due;
        let at = w.resource::<Clock>().at(e, due);
        w.resource_mut::<Wheel>().push(at, e, Due::Fuse);
    }
    *w.get_mut::<Fuse>(e).unwrap() = f;
    crate::bridge::view::mark(w, e);
}
/// The deadline: if the fuse is at its end the creature explodes and is gone.
pub fn fuse_due(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(f) = w.get::<Fuse>(e).copied() else { return };
    if f.armed != now || f.dir <= 0 || f.value_at(now) < f.max {
        return;
    }
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let radius = charged_radius(f.radius, w.get::<Charged>(e).is_some());
    let blast = Blast { at, radius, fire: false, owner: None, source: Some(e) };
    w.resource_mut::<crate::goals::Events>().exploded.push(blast);
    explode(w, blast);
    if let Some(cell) = w.get::<Cell>(e).map(|c| c.0)
        && cell != u32::MAX
    {
        w.resource_mut::<Index>().remove(e, cell);
    }
    w.entity_mut(e).despawn();
}
