//! The "entity inside a block" effects the host's `Mob` used to apply and that live in P now, driven by the mover (`Changed<Bounds>`, as
//! `inside.rs`): cobweb, sweet berry bush and powder snow slow the next move (`Entity.makeStuckInBlock`, kept as a kind in `Contact.stuck`
//! and consumed by `do_move`), the bush pricks a moving living entity, a lit campfire burns and a wither rose withers what stands in it
//! (those two are held by a sparse `Soaking` while the entity stays), a lava cauldron ignites, a magma block hurts the feet
//! (`stepOn`), and a stalagmite tip multiplies a landing (`fallOn`). A resting entity costs nothing.
//! Jar facts: `WebBlock` stuck (0.25, 0.05f, 0.25); `SweetBerryBushBlock` skips non-living, fox and bee, stuck (0.8f, 0.75, 0.8f), hurts 1
//! (`sweet_berry_bush`) when `age != 0` and the horizontal move since last tick has |dx| or |dz| >= 0.003; `PowderSnowBlock` stuck (0.9f, 1.5,
//! 0.9f) for a non-living entity or a living one whose own block is powder snow; `WitherRoseBlock` (server, not peaceful, living) adds
//! WITHER 40 ticks; `CampfireBlock` (lit, living) hurts `fireDamage` (1, soul 2); `LavaCauldronBlock` ignites 15 s (`lavaIgnite`) then
//! `lavaHurt` 4; `MagmaBlock.stepOn` (living, not stepping carefully) hurts 1 `hot_floor`; `PointedDripstoneBlock.fallOn` on an up-pointing
//! tip: distance + 2.5, modifier 2.0 (`calculateFallDamage` floors). `Entity.makeStuckInBlock` resets the fall distance; `Entity.move` scales the move (not a piston's)
//! then zeroes the velocity.
use crate::effects::{Effects, Kind};
use crate::cow::{Bounds, Cause, Clock, Contact, DamageQueue, Health, Level, set_on_fire};
use crate::inside::IgnoresTriggers;
use crate::item::Item;
use crate::orb::Orb;
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_world::registry::BlockStateId;

/// The kinds of `Contact.stuck` (1..=3) and of the other reacting blocks.
pub const K_WEB: u8 = 1;
pub const K_BERRY: u8 = 2;
pub const K_POWDER: u8 = 3;
const K_ROSE: u8 = 4;
const K_CAMPFIRE: u8 = 5;
const K_SOUL_CAMPFIRE: u8 = 6;
const K_LAVA_CAULDRON: u8 = 7;
const K_MAGMA: u8 = 8;
/// A cobweb a living entity with `Weaving` stands in (`WebBlock.entityInside`: 0.5, 0.25, 0.5).
pub const K_WEB_WEAVING: u8 = 9;
const K_CACTUS: u8 = 10;

/// `Entity.checkInsideBlocks`: the deflation of the box.
const DEFLATE: f64 = 1.0e-5_f32 as f64;
/// `SweetBerryBushBlock.entityInside`: the horizontal move that pricks.
const BERRY_MOVE: f64 = 0.003_f32 as f64;
/// How often a creature that stays in a campfire or a rose is visited again: the 10 ticks of the hurt window (a repeat inside it is absorbed).
const SOAK_PERIOD: i32 = 10;
/// `CampfireBlock.fireDamage` of the two campfires.
const CAMPFIRE_DAMAGE: f32 = 1.0;
const SOUL_CAMPFIRE_DAMAGE: f32 = 2.0;
/// `WitherRoseBlock`: the wither effect it gives (ticks) and how often a creature that stays in it gets it again (it never lapses).
const ROSE_TICKS: i32 = 40;
const ROSE_REFRESH: u64 = 10;
/// `Entity.lavaIgnite` (15 s), `lavaHurt` (4), `MagmaBlock.stepOn` (1), the stalagmite's fall distance factor and damage multiplier.
const LAVA_IGNITE_TICKS: u64 = 300;
const LAVA_HURT: f32 = 4.0;
const HOT_FLOOR: f32 = 1.0;
const STALAGMITE_DISTANCE: f64 = 2.5;
const STALAGMITE_MULTIPLIER: f64 = 2.0;
const SAFE_FALL: f64 = 3.0;

/// `Entity.stuckSpeedMultiplier` of a kind.
pub fn stuck_factor(kind: u8) -> Vec3 {
    match kind {
        K_WEB => Vec3::new(0.25, f64::from(0.05_f32), 0.25),
        K_WEB_WEAVING => Vec3::new(0.5, 0.25, 0.5),
        K_BERRY => Vec3::new(f64::from(0.8_f32), 0.75, f64::from(0.8_f32)),
        K_POWDER => Vec3::new(f64::from(0.9_f32), 1.5, f64::from(0.9_f32)),
        _ => Vec3::new(1.0, 1.0, 1.0),
    }
}

fn classify(name: &str) -> u8 {
    match name {
        "minecraft:cobweb" => K_WEB,
        "minecraft:sweet_berry_bush" => K_BERRY,
        "minecraft:powder_snow" => K_POWDER,
        "minecraft:wither_rose" => K_ROSE,
        "minecraft:campfire" => K_CAMPFIRE,
        "minecraft:soul_campfire" => K_SOUL_CAMPFIRE,
        "minecraft:lava_cauldron" => K_LAVA_CAULDRON,
        "minecraft:magma_block" => K_MAGMA,
        "minecraft:cactus" => K_CACTUS,
        _ => 0,
    }
}

/// The reaction class of every block state: one byte per state, built once on first use from the registry's names (the systems read
/// only the class, never a name). The `Level` is re-inserted by the bridge, so the table cannot be built with the resources.
#[derive(Resource, Default)]
pub struct InsideKinds(Vec<u8>);

impl InsideKinds {
    fn of(&mut self, mw: &MobWorld<'static>, id: BlockStateId) -> u8 {
        if self.0.is_empty() {
            self.0 = mw.facts.states.iter().map(|s| classify(&s.name)).collect();
        }
        self.0.get(id.0 as usize).copied().unwrap_or(0)
    }
}

/// The fox and the bee: `SweetBerryBushBlock.entityInside` skips them (a capability, put at creation).
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct IgnoresBerryBush;

/// The stuck kinds its class does not apply (`WitherBoss`: all; `Spider`: the web kinds), one bit per kind (`1 << kind`).
/// Its fall is not reset by those blocks either. Put at creation, from the species' capabilities.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NeverStuck(pub u16);
impl NeverStuck {
    pub const ALL: u16 = u16::MAX;
    pub const WEBS: u16 = 1 << K_WEB | 1 << K_WEB_WEAVING;
}

/// `Entity.wasInPowderSnow`: the creature was in powder snow at its last move, so the sun does not light it one tick more. Only while it
/// is in powder snow or has just left it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct WasInPowder;

/// `Entity.isInPowderSnow`: the creature's last move found powder snow in its box. Put by that move, taken away by the next move out of it
/// (absence = outside); the conversion tracker of a freezing skeleton reads it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct InPowderSnow;

/// Where a creature in a bush last was (centre x, z): the move since then decides the prick. Only while it is in a bush.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct BerryTrail(pub f64, pub f64);

/// In a campfire or a wither rose and staying: the next time the rose is given again. Only while it stands in one.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Soaking(pub u64);

pub fn insert_resources(w: &mut World) {
    w.insert_resource(InsideKinds::default());
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((moved_effects.after(crate::cow::reindex), living_lava));
}

/// `LivingEntity.baseTick`: a creature standing in lava (not fire immune) is hit for 4 and set on fire for 300 ticks, each tick; the
/// invulnerable time of the hit keeps the next one off until it runs down (`DamageQueue`). Reads the fluid state of the tick before.
fn living_lava(q: Query<(Entity, &crate::cow::FluidState), With<Health>>, mut commands: Commands) {
    for (e, f) in &q {
        if f.in_lava {
            commands.queue(move |w: &mut World| {
                if w.get::<crate::cow::Immune>(e).is_some_and(|i| i.has(crate::effects::IMMUNE_FIRE)) {
                    return;
                }
                set_on_fire(w, e, LAVA_IGNITE_TICKS);
                w.resource_mut::<DamageQueue>().add(e, LAVA_HURT, Cause::Lava);
            });
        }
    }
}

/// What the cells a box reaches into hold.
#[derive(Default)]
struct Found {
    stuck: u8,
    berry: bool,
    pricks: bool,
    rose: bool,
    campfire: f32,
    cauldron: bool,
    cactus: u32,
    /// The powder snow cell a living creature stands in: `PowderSnowBlock.entityInside` breaks it when it burns.
    powder: Option<[i32; 3]>,
    /// Any powder snow cell the box reaches.
    in_powder: bool,
}

fn scan(level: &Level, kinds: &mut InsideKinds, b: &Aabb, living: bool, weaving: bool) -> Found {
    let (lo, hi) = (b.min_position(), b.max_position());
    let mw = level.mw.0;
    let own = [((lo.x + hi.x) / 2.0).floor() as i32, lo.y.floor() as i32, ((lo.z + hi.z) / 2.0).floor() as i32];
    let mut f = Found::default();
    for x in (lo.x + DEFLATE).floor() as i32..=(hi.x - DEFLATE).floor() as i32 {
        for y in (lo.y + DEFLATE).floor() as i32..=(hi.y - DEFLATE).floor() as i32 {
            for z in (lo.z + DEFLATE).floor() as i32..=(hi.z - DEFLATE).floor() as i32 {
                match kinds.of(mw, mw.world.block(x, y, z)) {
                    0 => {}
                    K_WEB => f.stuck = if weaving { K_WEB_WEAVING } else { K_WEB },
                    K_BERRY if living => {
                        f.stuck = K_BERRY;
                        f.berry = true;
                        f.pricks |= mw.prop(x, y, z, "age") != Some("0");
                    }
                    K_POWDER => {
                        f.in_powder = true;
                        if !living || [x, y, z] == own {
                            f.stuck = K_POWDER;
                            if living {
                                f.powder = Some([x, y, z]);
                            }
                        }
                    }
                    K_ROSE => f.rose = true,
                    k @ (K_CAMPFIRE | K_SOUL_CAMPFIRE) if mw.prop(x, y, z, "lit") == Some("true") => {
                        f.campfire += if k == K_CAMPFIRE { CAMPFIRE_DAMAGE } else { SOUL_CAMPFIRE_DAMAGE };
                    }
                    K_LAVA_CAULDRON => f.cauldron = true,
                    K_CACTUS => f.cactus += 1,
                    _ => {}
                }
            }
        }
    }
    f
}

/// The hits and effects of the cells found, for a living entity (`living`) or any (`f.cauldron`).
fn apply(f: &Found, e: Entity, living: bool, rose_due: bool, dmg: &mut DamageQueue, commands: &mut Commands) {
    if living {
        let mut left = f.campfire;
        while left > 0.0 {
            let one = if left >= SOUL_CAMPFIRE_DAMAGE && f.campfire >= SOUL_CAMPFIRE_DAMAGE { SOUL_CAMPFIRE_DAMAGE } else { CAMPFIRE_DAMAGE };
            dmg.add(e, one, Cause::Campfire);
            left -= one;
        }
        if f.rose && rose_due {
            commands.queue(move |w: &mut World| {
                if crate::effects::difficulty(w) != 0 {
                    crate::effects::add(w, e, crate::effects::Kind::Wither, 0, ROSE_TICKS);
                }
            });
        }
    }
    if let Some(p) = f.powder {
        // The EXTINGUISH effect: a burning creature breaks the powder snow (no drop) when mobs may grief, and is put out.
        commands.queue(move |w: &mut World| {
            let burning = w.get::<crate::cow::Burning>(e).is_some_and(|b| b.until > crate::cow::now_of(w, e) as i64);
            if burning && crate::goals::mob_griefing(w) {
                crate::level::break_block(w, p, false, None, Some(e));
                w.entity_mut(e).remove::<crate::cow::Burning>();
            }
        });
    }
    if f.cauldron {
        commands.queue(move |w: &mut World| set_on_fire(w, e, LAVA_IGNITE_TICKS));
        dmg.add(e, LAVA_HURT, Cause::Lava);
    }
}

#[allow(clippy::type_complexity)]
fn moved_effects(
    level: Res<Level>,
    clock: Res<Clock>,
    mut kinds: ResMut<InsideKinds>,
    mut dmg: ResMut<DamageQueue>,
    mut commands: Commands,
    mut q: Query<
        (Entity, Ref<Bounds>, Option<&mut Contact>, Has<Health>, Has<Item>, Has<Orb>, Has<IgnoresBerryBush>, Option<&mut BerryTrail>, Has<Soaking>, Option<&Effects>, Option<&NeverStuck>),
        (Changed<Bounds>, Without<IgnoresTriggers>),
    >,
    was: Query<(), With<WasInPowder>>,
    inside: Query<(), With<InPowderSnow>>,
) {
    for (e, b, contact, health, item, orb, no_berry, trail, soaking, effects, never) in &mut q {
        let living = health && !item && !orb;
        let f = scan(&level, &mut kinds, &b.0, living, effects.is_some_and(|f| f.list.iter().any(|x| x.kind == Kind::Weaving)));
        if f.in_powder {
            commands.entity(e).insert((WasInPowder, InPowderSnow));
        } else if inside.contains(e) {
            commands.entity(e).remove::<InPowderSnow>();
        }
        if !f.in_powder && was.contains(e) {
            commands.queue(move |w: &mut World| crate::cow::after(w, e, 1, forget_powder));
        }
        let stuck = if never.is_some_and(|n| (n.0 >> f.stuck) & 1 != 0) || (f.stuck == K_BERRY && no_berry) { 0 } else { f.stuck };
        if let Some(mut c) = contact {
            if c.stuck != stuck {
                c.stuck = stuck;
            }
            if stuck != 0 && c.fall_distance != 0.0 {
                c.fall_distance = 0.0;
            }
            if living && c.on_ground {
                let (lo, hi) = (b.0.min_position(), b.0.max_position());
                let mw = level.mw.0;
                let below = mw.world.block(((lo.x + hi.x) / 2.0).floor() as i32, (lo.y - 1.0e-5).floor() as i32, ((lo.z + hi.z) / 2.0).floor() as i32);
                if kinds.of(mw, below) == K_MAGMA {
                    dmg.add(e, HOT_FLOOR, Cause::HotFloor);
                }
            }
        }
        let (lo, hi) = (b.0.min_position(), b.0.max_position());
        let (cx, cz) = ((lo.x + hi.x) / 2.0, (lo.z + hi.z) / 2.0);
        if f.berry && !no_berry {
            let moved = match trail {
                Some(mut t) => {
                    let m = (cx - t.0).abs() >= BERRY_MOVE || (cz - t.1).abs() >= BERRY_MOVE;
                    t.0 = cx;
                    t.1 = cz;
                    m
                }
                None => {
                    commands.entity(e).insert(BerryTrail(cx, cz));
                    // `SweetBerryBushBlock.entityInside` compares `oldPosition` with `position`: a body just put there (summon, load) has not moved.
                    !b.is_added()
                }
            };
            if moved && f.pricks {
                dmg.add(e, 1.0, Cause::SweetBerry);
            }
        } else if trail.is_some() {
            commands.entity(e).remove::<BerryTrail>();
        }
        apply(&f, e, living, true, &mut dmg, &mut commands);
        // `CactusBlock.entityInside` hurts an item a point per cactus cell its box reaches (living ones take theirs elsewhere).
        if item {
            for _ in 0..f.cactus {
                dmg.add(e, 1.0, Cause::Cactus);
            }
        }
        let holds = living && (f.rose || f.campfire > 0.0 || f.cauldron);
        if holds && !soaking {
            let due = clock.of(e) + ROSE_REFRESH;
            commands.queue(move |w: &mut World| {
                w.entity_mut(e).insert(Soaking(due));
                crate::cow::after(w, e, SOAK_PERIOD, soak_due);
            });
        }
    }
}

/// The deadline of a creature that stays in a campfire or a wither rose: the fire hurts it, the rose is given again before it lapses, and
/// the next deadline is armed while it is still in one. One that no longer is lets go of `Soaking`, so nothing is armed: no cost at rest.
fn soak_due(w: &mut World, e: Entity) {
    let (Some(b), Some(&Soaking(rose_at))) = (w.get::<Bounds>(e).map(|b| b.0), w.get::<Soaking>(e)) else { return };
    let now = crate::cow::now_of(w, e);
    let f = w.resource_scope(|w, mut kinds: Mut<InsideKinds>| scan(w.resource::<Level>(), &mut kinds, &b, true, false));
    if !f.rose && f.campfire <= 0.0 && !f.cauldron {
        w.entity_mut(e).remove::<Soaking>();
        return;
    }
    let due = now >= rose_at;
    if due {
        w.entity_mut(e).insert(Soaking(now + ROSE_REFRESH));
    }
    let mut queue = bevy_ecs::world::CommandQueue::default();
    w.resource_scope(|w, mut dmg: Mut<DamageQueue>| apply(&Found { stuck: 0, berry: false, pricks: false, powder: None, ..f }, e, true, due, &mut dmg, &mut Commands::new(&mut queue, w)));
    queue.apply(w);
    crate::cow::after(w, e, SOAK_PERIOD, soak_due);
}

/// The tick after a creature left powder snow: the memory goes, unless it is back in one by then.
fn forget_powder(w: &mut World, e: Entity) {
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return };
    let f = w.resource_scope(|w, mut kinds: Mut<InsideKinds>| scan(w.resource::<Level>(), &mut kinds, &b, true, false));
    if !f.in_powder {
        w.entity_mut(e).remove::<WasInPowder>();
    }
}

/// `PointedDripstoneBlock.fallOn`: a landing on an up-pointing tip counts the distance plus 2.5 and doubles the damage (modifier 2.0);
/// `Some(damage)` replaces the ordinary one. Read at a landing only.
pub fn fall_on(level: &Level, feet: Vec3, fall: f64) -> Option<f32> {
    let mw = level.mw.0;
    let (x, y, z) = (feet.x.floor() as i32, (feet.y - 1.0e-5).floor() as i32, feet.z.floor() as i32);
    if &*mw.facts.get(mw.world.block(x, y, z)).name != "minecraft:pointed_dripstone" || mw.prop(x, y, z, "tip_direction") != Some("up") || mw.prop(x, y, z, "thickness") != Some("tip") {
        return None;
    }
    Some(fall_damage(fall + STALAGMITE_DISTANCE, STALAGMITE_MULTIPLIER))
}

/// `LivingEntity.calculateFallDamage`: floor of (distance + 1e-6 - safe distance 3) x modifier, never negative.
pub fn fall_damage(distance: f64, modifier: f64) -> f32 {
    fall_damage_safe(distance, SAFE_FALL, modifier)
}

/// `calculateFallDamage` with the entity's own safe fall distance (`Attributes.SAFE_FALL_DISTANCE`).
pub fn fall_damage_safe(distance: f64, safe: f64, modifier: f64) -> f32 {
    ((distance + 1.0e-6 - safe) * modifier).floor().max(0.0) as f32
}

/// `Entity.remainingFireTicks` of a projectile, as the own tick it ends at (sparse; only a burning projectile has it). `AbstractArrow.onHitEntity`
/// ignites its victim for 5 s while it is on fire, before the damage.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Ablaze(pub u64);

/// The ticks of fire the projectile `e` has left (0: not burning).
pub fn fire_left(w: &World, e: Entity) -> u64 {
    w.get::<Ablaze>(e).map_or(0, |a| a.0.saturating_sub(crate::cow::now_of(w, e)))
}

/// Sets the fire of a projectile to `ticks` from now; 0 puts it out.
pub fn ignite_projectile(w: &mut World, e: Entity, ticks: u64) {
    if ticks == 0 {
        w.entity_mut(e).remove::<Ablaze>();
    } else {
        let until = crate::cow::now_of(w, e) + ticks;
        w.entity_mut(e).insert(Ablaze(until));
    }
}

/// `igniteForSeconds(5.0F)`: 100 ticks.
pub const ARROW_IGNITES: u64 = 100;
