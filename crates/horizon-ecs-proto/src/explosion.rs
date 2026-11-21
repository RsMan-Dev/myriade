//! The explosion capability (`ServerExplosion`): `explode(w, blast)` at a centre with a radius: the creatures within twice the radius take
//! `(pow^2 + pow) / 2 * 7 * diameter + 1` with `pow = (1 - distance / diameter) * exposure` (the share of the rays from a grid over their box
//! that reach the centre unblocked) and are pushed away by `pow`; the blocks on 1 352 rays from the centre are broken while the ray's power
//! (`radius * (0.7 + 0.6 * random)`, less `(resistance + 0.3) * 0.3` and 0.225 per 0.3-block step) lasts; with fire, a third of them that
//! have air and a floor take fire. The blocks are asked as `BlockChanges` (applied at the next step). Not done: items and projectiles
//! in the blast, the players' push, enchantment protections, the sounds and particles, the other blocks' own `onExplosionHit`.
//! A creeper's fuse (`Fuse`) is a deadline of the wheel (`Due::Fuse`), armed when its direction turns to +1: no tick per creeper.
use crate::cow::*;
use crate::goals::mob_griefing;
use crate::projectile::Blast;
use crate::targeting::{player_box, Who};
use bevy_ecs::prelude::*;
use horizon_client_lib::arrows::clip_block_hit;
use horizon_core::geometry::{Aabb, Vec3};

#[derive(Resource)]
pub struct ExplosionRng(pub Rng);
pub fn insert_resources(w: &mut World) {
    w.insert_resource(ExplosionRng(Rng(0x2545_F491_4F6C_DD1D)));
}

/// `ServerExplosion.getSeenPercent` for a box: the share of the grid's rays to the centre that no block collider stops.
pub fn seen_percent(level: &Level, center: Vec3, b: Aabb) -> f32 {
    let (lo, hi) = (b.min_position(), b.max_position());
    let (xs, ys, zs) = (1.0 / ((hi.x - lo.x) * 2.0 + 1.0), 1.0 / ((hi.y - lo.y) * 2.0 + 1.0), 1.0 / ((hi.z - lo.z) * 2.0 + 1.0));
    let (xo, zo) = ((1.0 - (1.0 / xs).floor() * xs) / 2.0, (1.0 - (1.0 / zs).floor() * zs) / 2.0);
    let mw = level.mw.0;
    let (mut hits, mut count) = (0, 0);
    let mut xx = 0.0;
    while xx <= 1.0 {
        let mut yy = 0.0;
        while yy <= 1.0 {
            let mut zz = 0.0;
            while zz <= 1.0 {
                let from = Vec3::new(lo.x + (hi.x - lo.x) * xx + xo, lo.y + (hi.y - lo.y) * yy, lo.z + (hi.z - lo.z) * zz + zo);
                let to = Vec3::new(center.x - from.x, center.y - from.y, center.z - from.z);
                if clip_block_hit(mw.world, mw.facts, mw.collision, from, to).is_none() {
                    hits += 1;
                }
                count += 1;
                zz += zs;
            }
            yy += ys;
        }
        xx += xs;
    }
    hits as f32 / count as f32
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
                    let wet = f.fluid != horizon_client_lib::blocks::Fluid::None;
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
/// The damage a creature takes at `dist` (a share of the diameter) with `exposure`.
pub fn blast_damage(dist: f64, exposure: f32, diameter: f64) -> f32 {
    let pow = (1.0 - dist) * f64::from(exposure);
    ((pow * pow + pow) / 2.0 * 7.0 * diameter + 1.0) as f32
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
        let dist = ((feet.x - at.x).powi(2) + (feet.y - at.y).powi(2) + (feet.z - at.z).powi(2)).sqrt() / diameter;
        if dist > 1.0 || w.get::<Vel>(c).is_none() {
            continue;
        }
        let exposure = seen_percent(&level, at, bounds);
        let eye = feet.y + w.get::<crate::targeting::Eye>(c).map_or(0.0, |e| e.0);
        let (dx, dy, dz) = (feet.x - at.x, eye - at.y, feet.z - at.z);
        let l = (dx * dx + dy * dy + dz * dz).sqrt();
        let dir = if l < 1.0e-5 { Vec3::ZERO } else { Vec3::new(dx / l, dy / l, dz / l) };
        let pow = (1.0 - dist) * f64::from(exposure);
        if let Some(mut v) = w.get_mut::<Vel>(c) {
            v.0 = Vec3::new(v.0.x + dir.x * pow, v.0.y + dir.y * pow, v.0.z + dir.z * pow);
        }
        if let Ok(mut m) = w.get_entity_mut(c) {
            m.insert(Moving);
        }
        crate::stand::unsettle(w, c);
    }
}
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
    let level = *w.resource::<Level>();
    let diameter = f64::from(b.radius * 2.0);
    // The creatures around.
    let mut near: Vec<Entity> = Vec::new();
    w.resource::<Index>().for_radius(b.at.x, b.at.z, diameter, |c| near.push(c));
    for c in near {
        // `ArmorStand.ignoreExplosion`: an invisible stand is no target.
        let spared = w.get::<crate::stand::Stand>(c).is_some_and(|s| crate::stand::ignores_explosion(*s));
        if Some(c) == b.source || w.get::<Health>(c).is_none() || spared {
            continue;
        }
        let (Some(feet), Some(bounds)) = (w.get::<Pos>(c).map(|p| p.0), w.get::<Bounds>(c).map(|x| x.0)) else { continue };
        let dist = ((feet.x - b.at.x).powi(2) + (feet.y - b.at.y).powi(2) + (feet.z - b.at.z).powi(2)).sqrt() / diameter;
        if dist > 1.0 {
            continue;
        }
        let exposure = seen_percent(&level, b.at, bounds);
        let eye = feet.y + w.get::<crate::targeting::Eye>(c).map_or(0.0, |e| e.0);
        let (dx, dy, dz) = (feet.x - b.at.x, eye - b.at.y, feet.z - b.at.z);
        let l = (dx * dx + dy * dy + dz * dz).sqrt();
        let dir = if l < 1.0e-4 { Vec3::ZERO } else { Vec3::new(dx / l, dy / l, dz / l) };
        let pow = (1.0 - dist) * f64::from(exposure);
        w.resource_mut::<DamageQueue>().add_from(c, blast_damage(dist, exposure, diameter), Cause::Blast, (b.at.x, b.at.z));
        if let Some(mut v) = w.get_mut::<Vel>(c) {
            v.0 = Vec3::new(v.0.x + dir.x * pow, v.0.y + dir.y * pow, v.0.z + dir.z * pow);
        }
        if let Ok(mut m) = w.get_entity_mut(c) {
            m.insert(Moving);
        }
        crate::stand::unsettle(w, c);
    }
    // The players around.
    let tick = w.resource::<Clock>().0;
    let players: Vec<(u32, Aabb)> = {
        let on = w.resource::<crate::goals::Online>();
        let mut v = Vec::new();
        on.for_near(b.at.x, b.at.z, diameter + 2.0, |p| v.push((p.id, player_box(&p))));
        v
    };
    for (id, bx) in players {
        let feet = bx.min_position();
        let dist = ((feet.x - b.at.x).powi(2) + (feet.y - b.at.y).powi(2) + (feet.z - b.at.z).powi(2)).sqrt() / diameter;
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
                crate::tnt::spawn_from_blast(w, *p, &mut rng);
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
pub struct Charged;
impl Fuse {
    pub fn value_at(&self, now: u64) -> i32 {
        (self.value + i32::from(self.dir) * (now - self.at) as i32).clamp(0, self.max)
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
        let due = now + (f.max - f.value).max(1) as u64;
        f.armed = due;
        let at = w.resource::<Clock>().at(e, due);
        w.resource_mut::<Wheel>().push(at, e, Due::Fuse);
    }
    *w.get_mut::<Fuse>(e).unwrap() = f;
}
/// The deadline: if the fuse is at its end the creature explodes and is gone.
pub fn fuse_due(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(f) = w.get::<Fuse>(e).copied() else { return };
    if f.armed != now || f.dir <= 0 || f.value_at(now) < f.max {
        return;
    }
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let radius = f.radius * if w.get::<Charged>(e).is_some() { 2.0 } else { 1.0 };
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
