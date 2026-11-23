//! More "entity inside a block" effects, driven by movement (`Changed<Bounds>` / `Changed<Pos>`), a resting entity costs nothing:
//! minecarts and boats take the cactus, minecarts take lava and fire (`VehicleEntity.hurtServer`), boats are lit by lava, a projectile
//! crossing lava or fire ignites and one in water or rain goes out, and a honey block slows a creature, TNT, cart or boat that slides on it.
//! Jar facts: `CactusBlock.entityInside` hurts 1 (any entity); `AbstractMinecart.tick` has no `baseTick`: in lava it `lavaIgnite`s and
//! `lavaHurt`s 4 each tick and halves the fall distance, with no fire countdown; a fire source destroys a TNT cart (`MinecartTNT.
//! shouldSourceDestroy`), which primes the short fuse; `BaseFireBlock` hurts `fireDamage` (soul fire 2); `Entity.lavaIgnite` is 15 s,
//! `fireIgnite` 8 s; `EXTINGUISH` (`clearFire`) runs last, `AbstractArrow.tick` clears the fire in water or rain. `HoneyBlock`: only a
//! `LivingEntity`, `AbstractMinecart`, `PrimedTnt` or `AbstractBoat` slides; it slides when off the ground, below the block's top
//! (0.9375), falling faster than 0.08 once the gravity step is undone, and its box reaches the sides (0.4375 + half the width from the
//! cell centre); the fall is then capped at 0.05 (`doSlideMovement`) and the fall distance reset.
use crate::boat_state::{BoatMarker, Hull};
use crate::cart_state::Cart;
use crate::cow::{Bounds, Contact, Health, Level, Pos, Vel};
use crate::explosion::ExplosionRng;
use crate::fishing_hook::Hook;
use crate::inside_effects::Ablaze;
use crate::inside::IgnoresTriggers;
use crate::item::Item;
use crate::minecart::{Fall, Track};
use crate::orb::Orb;
use crate::projectile::{Flight, Flying};
use crate::trident::{Trident, TridentFlying};
use crate::tnt::Primed;
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_items::blocks::Fluid;
use horizon_world::registry::BlockStateId;

const UNKNOWN: u8 = u8::MAX;
const K_FIRE: u8 = 1;
const K_SOUL_FIRE: u8 = 2;
const K_CACTUS: u8 = 3;
const K_HONEY: u8 = 4;
const K_LAVA: u8 = 5;
const K_WATER: u8 = 6;

/// `Entity.checkInsideBlocks`: the deflation of the box.
const DEFLATE: f64 = 1.0e-5_f32 as f64;
const CACTUS_DAMAGE: f32 = 1.0;
const LAVA_HURT: f32 = 4.0;
const FIRE_DAMAGE: f32 = 1.0;
const SOUL_FIRE_DAMAGE: f32 = 2.0;
/// `Entity.lavaIgnite` (15 s) and `BaseFireBlock.fireIgnite` (8 s), in ticks.
pub const LAVA_IGNITE: u64 = 300;
pub const FIRE_IGNITE: u64 = 160;
/// `MinecartTNT.destroy`'s short fuse: two draws of `nextInt(20)`.
const FUSE_DRAW: i32 = 20;
/// `Entity.baseTick`: lava halves the fall distance.
const LAVA_FALL: f64 = 0.5;
/// `HoneyBlock`: the slide's thresholds and speeds.
const SLIDE_TOP: f64 = 0.9375;
const SLIDE_SIDE: f64 = 0.4375;
const MIN_SLIDE_SPEED: f64 = 0.08;
const SLIDE_AT: f64 = 0.13;
const SLIDE_CAP: f64 = 0.05;
const GRAVITY_DRAG: f64 = 0.98;
/// How often a stuck burning projectile re-reads its cell.
const STUCK_RECHECK: i32 = 20;

fn classify(mw: &MobWorld<'static>, id: BlockStateId) -> u8 {
    let f = mw.facts.get(id);
    match &*f.name {
        "minecraft:fire" => K_FIRE,
        "minecraft:soul_fire" => K_SOUL_FIRE,
        "minecraft:cactus" => K_CACTUS,
        "minecraft:honey_block" => K_HONEY,
        _ => match f.fluid {
            Fluid::Lava => K_LAVA,
            Fluid::Water => K_WATER,
            _ => 0,
        },
    }
}

/// The reaction kind of each block state, learnt on first sight (the name is read once per state).
#[derive(Resource, Default)]
pub struct MoreKinds(Vec<u8>);

impl MoreKinds {
    fn of(&mut self, mw: &MobWorld<'static>, id: BlockStateId) -> u8 {
        let i = id.0 as usize;
        if i >= self.0.len() {
            self.0.resize(i + 1, UNKNOWN);
        }
        if self.0[i] == UNKNOWN {
            self.0[i] = classify(mw, id);
        }
        self.0[i]
    }
}

pub fn insert_resources(w: &mut World) {
    w.insert_resource(MoreKinds::default());
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((vehicles_moved, creatures_on_honey, shot_flame).after(crate::cow::reindex));
}

/// The reacting cells a box reaches into.
#[derive(Default)]
struct Seen {
    fire: Vec<f32>,
    cactus: u32,
    lava: bool,
    water: bool,
    honey: Vec<[i32; 3]>,
}

fn scan(mw: &MobWorld<'static>, kinds: &mut MoreKinds, b: &Aabb) -> Seen {
    let (lo, hi) = (b.min_position(), b.max_position());
    let mut s = Seen::default();
    for x in (lo.x + DEFLATE).floor() as i32..=(hi.x - DEFLATE).floor() as i32 {
        for y in (lo.y + DEFLATE).floor() as i32..=(hi.y - DEFLATE).floor() as i32 {
            for z in (lo.z + DEFLATE).floor() as i32..=(hi.z - DEFLATE).floor() as i32 {
                match kinds.of(mw, mw.world.block(x, y, z)) {
                    0 => {}
                    K_FIRE => s.fire.push(FIRE_DAMAGE),
                    K_SOUL_FIRE => s.fire.push(SOUL_FIRE_DAMAGE),
                    K_CACTUS => s.cactus += 1,
                    K_HONEY => s.honey.push([x, y, z]),
                    K_LAVA => s.lava = true,
                    _ => s.water = true,
                }
            }
        }
    }
    s
}

/// What a projectile at `pos` in `b` does to its fire: `Some(0)` puts it out (water in its box, or rain at its block or at the block of
/// its top: `Entity.isInRain`, read by `AbstractArrow.tick`), `Some(n)` sets at least `n` ticks (lava, fire), `None` leaves it. Water wins
/// over the flames, as `EXTINGUISH` is the last effect.
pub fn flame(mw: &MobWorld<'static>, kinds: &mut MoreKinds, b: &Aabb, pos: Vec3, burning: bool) -> Option<u64> {
    let s = scan(mw, kinds, b);
    let (x, y, z) = (pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32);
    let rain = burning && (mw.raining_at(x, y, z) || mw.raining_at(x, b.max_position().y.floor() as i32, z));
    if s.water || rain {
        return burning.then_some(0);
    }
    if s.lava {
        Some(LAVA_IGNITE)
    } else if !s.fire.is_empty() {
        Some(FIRE_IGNITE)
    } else {
        None
    }
}

/// Sets the fire a `flame` result asks for, never shortening a longer one.
fn light(w: &mut World, e: Entity, ticks: u64) {
    if ticks == 0 || crate::inside_effects::fire_left(w, e) < ticks {
        crate::inside_effects::ignite_projectile(w, e, ticks);
    }
}

/// Projectiles in flight and the ones that just stopped: their fire is lit or put out. Not gated by the `Sensitive` table (the host hands
/// it over at the first block tick, possibly after the first shot).
#[allow(clippy::type_complexity)]
fn shot_flame(
    level: Res<Level>,
    mut kinds: ResMut<MoreKinds>,
    mut commands: Commands,
    flying: Query<(Entity, &Pos, &Flight, Has<Ablaze>), (With<Flying>, Without<Hook>)>,
    planes: Query<(Entity, &Pos, Has<Ablaze>), (With<Trident>, With<TridentFlying>, Without<Flight>)>,
    mut landed: (RemovedComponents<Flying>, RemovedComponents<TridentFlying>),
) {
    let mw = level.mw.0;
    let mut check = |e: Entity, p: Vec3, size: f64, burning: bool| {
        if let Some(t) = flame(mw, &mut kinds, &crate::arrow_more::arrow_box(p, size), p, burning) {
            commands.queue(move |w: &mut World| light(w, e, t));
        }
    };
    for (e, p, f, burning) in &flying {
        check(e, p.0, f.size, burning);
    }
    for (e, p, burning) in &planes {
        check(e, p.0, Flight::TRIDENT.size, burning);
    }
    for e in landed.0.read().chain(landed.1.read()) {
        commands.queue(move |w: &mut World| stuck_flame(w, e));
    }
}

/// A stuck projectile keeps checking while it burns or sits in fire (`AbstractArrow.tick` runs in the ground too): a deadline every
/// `STUCK_RECHECK` ticks, none for one that is neither.
fn stuck_flame(w: &mut World, e: Entity) {
    let (Some(p), Some(mw)) = (w.get::<Pos>(e).map(|p| p.0), w.get_resource::<Level>().map(|l| l.mw.0)) else { return };
    if w.get::<Flying>(e).is_some() || w.get::<TridentFlying>(e).is_some() {
        return;
    }
    let size = w.get::<Flight>(e).map_or(Flight::TRIDENT.size, |f| f.size);
    let burning = crate::inside_effects::fire_left(w, e) > 0;
    let found = w.resource_scope(|_, mut k: Mut<MoreKinds>| flame(mw, &mut k, &crate::arrow_more::arrow_box(p, size), p, burning));
    if let Some(t) = found {
        light(w, e, t);
    }
    if found.is_some_and(|t| t > 0) || (burning && found.is_none()) {
        crate::cow::after(w, e, STUCK_RECHECK, stuck_flame);
    }
}

/// `HoneyBlock.isSlidingDown` for a body of `width` at `p` moving by `v` over the honey cell `cell`.
pub fn sliding_down(cell: [i32; 3], p: Vec3, v: Vec3, width: f32, on_ground: bool) -> bool {
    if on_ground || p.y > f64::from(cell[1]) + SLIDE_TOP - 1.0e-7 || v.y / GRAVITY_DRAG + MIN_SLIDE_SPEED >= -MIN_SLIDE_SPEED {
        return false;
    }
    let reach = SLIDE_SIDE + f64::from(width) / 2.0;
    let (dx, dz) = ((f64::from(cell[0]) + 0.5 - p.x).abs(), (f64::from(cell[2]) + 0.5 - p.z).abs());
    dx + 1.0e-7 > reach || dz + 1.0e-7 > reach
}

/// `HoneyBlock.doSlideMovement`: the velocity after the slide.
pub fn slide(v: Vec3) -> Vec3 {
    let old = v.y / GRAVITY_DRAG + MIN_SLIDE_SPEED;
    let (k, y) = if old < -SLIDE_AT { (-SLIDE_CAP / old, -SLIDE_CAP) } else { (1.0, -SLIDE_CAP) };
    Vec3::new(v.x * k, (y - MIN_SLIDE_SPEED) * GRAVITY_DRAG, v.z * k)
}

fn slide_all(cells: &[[i32; 3]], p: Vec3, v: &mut Vec3, width: f32, on_ground: bool) -> bool {
    let mut slid = false;
    for &c in cells {
        if sliding_down(c, p, *v, width, on_ground) {
            *v = slide(*v);
            slid = true;
        }
    }
    slid
}

/// Creatures and primed TNT that moved: the honey slide (`LivingEntity`, `PrimedTnt`; items, orbs and projectiles never slide).
#[allow(clippy::type_complexity)]
fn creatures_on_honey(
    level: Res<Level>,
    mut kinds: ResMut<MoreKinds>,
    mut q: Query<(&Pos, &Bounds, &mut Vel, &mut Contact), (Changed<Bounds>, Or<(With<Health>, With<Primed>)>, Without<Item>, Without<Orb>, Without<IgnoresTriggers>)>,
) {
    let mw = level.mw.0;
    for (p, b, mut v, mut c) in &mut q {
        let s = scan(mw, &mut kinds, &b.0);
        if s.honey.is_empty() {
            continue;
        }
        let mut nv = v.0;
        if slide_all(&s.honey, p.0, &mut nv, (b.0.max_position().x - b.0.min_position().x) as f32, c.on_ground) {
            v.0 = nv;
            c.fall_distance = 0.0;
        }
    }
}

/// Carts and boats that moved: scanned with world access, as their hits and fires are world operations.
#[allow(clippy::type_complexity)]
fn vehicles_moved(mut commands: Commands, q: Query<Entity, (Changed<Pos>, Or<(With<Cart>, With<BoatMarker>)>, Without<IgnoresTriggers>)>) {
    for e in &q {
        commands.queue(move |w: &mut World| vehicle_scan(w, e));
    }
}

fn short_fuse(w: &mut World) -> i32 {
    let mut r = w.resource_mut::<ExplosionRng>();
    let a = r.0.next_int(FUSE_DRAW);
    a + r.0.next_int(FUSE_DRAW)
}

/// The effects of the cells of cart or boat `e`. A cart in lava is scanned again next tick: it breaks within two (`lavaHurt` each tick).
pub fn vehicle_scan(w: &mut World, e: Entity) {
    let (Some(p), Some(mw)) = (w.get::<Pos>(e).map(|p| p.0), w.get_resource::<Level>().map(|l| l.mw.0)) else { return };
    let cart = w.get::<Cart>(e).is_some();
    let b = if cart { crate::rules::minecart::bounds_at(p) } else { crate::boat::bounds_at(p) };
    let s = w.resource_scope(|_, mut k: Mut<MoreKinds>| scan(mw, &mut k, &b));
    for _ in 0..s.cactus {
        if cart {
            crate::cart_variants::hit_cart(w, e, CACTUS_DAMAGE, false, None);
        } else {
            let flags = crate::boat_hurt::HurtFlags { invulnerable: false, entity_drops: true };
            crate::boat_hurt::hit(w, e, CACTUS_DAMAGE, None, false, flags);
        }
    }
    if cart {
        for d in s.fire {
            crate::cart_variants::hit_cart_fire(w, e, d, short_fuse);
        }
        if s.lava {
            crate::cart_variants::hit_cart_fire(w, e, LAVA_HURT, short_fuse);
            if let Some(mut f) = w.get_mut::<Fall>(e) {
                f.0 *= LAVA_FALL;
            }
            crate::cow::after(w, e, 1, vehicle_scan);
        }
    } else if s.lava {
        let now = w.get::<Hull>(e).map_or(0, |h| h.fire);
        crate::boat_effects::set_fire(w, e, now.max(LAVA_IGNITE as i32));
    }
    if s.honey.is_empty() || w.get_entity(e).is_err() {
        return;
    }
    let on_ground = w.get::<Track>(e).map(|t| t.on_ground).or_else(|| w.get::<Hull>(e).map(|h| h.on_ground)).unwrap_or(false);
    let Some(mut v) = w.get::<Vel>(e).map(|v| v.0) else { return };
    if slide_all(&s.honey, p, &mut v, (b.max_position().x - b.min_position().x) as f32, on_ground) {
        w.entity_mut(e).insert(Vel(v));
        if cart {
            w.entity_mut(e).remove::<Fall>();
        } else if let Some(mut h) = w.get_mut::<Hull>(e) {
            h.fall = 0.0;
        }
    }
}
