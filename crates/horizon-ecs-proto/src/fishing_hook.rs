//! The fishing hook (`FishingHook`, `game/items/fishing.rs`): a bobber a player casts. It flies until it lands in water or meets a
//! creature, a player or a dropped item; it floats in the water and waits for a fish: the lure, the fish swimming up, the bite and
//! its window. The waits are counted per tick as vanilla counts them (rain and sky change the speed of the fish), by this system,
//! which runs only while a hook exists (`sparse_any`). The catch, the pull, the sounds, the particles and the rod's damage leave
//! through `HostEvent`; the host rolls the loot and gives the xp.
use crate::cow::*;
use crate::goals::{sparse_any, Events, Online};
use crate::host_out_hook::{HookEvent, HookOut};
use crate::targeting::Who;
use crate::hostile::PlayerState;
use crate::item::ItemIndex;
use crate::projectile::entity_hit;
use crate::hook_wait::{Approach, Wait};
use crate::rules::fishing_hook::{bite_impulse, buoyancy_force, path_margin, pull_toward};
use crate::rules::hook_wait::{approach_distance, lured_after};
use crate::targeting::player_box;
use crate::type_tags::TagId;
use bevy_ecs::prelude::*;
use myriade_engine_lib::arrows::clip_block_hit;
use myriade_engine_lib::mobs::spawn_rules::SpawnLevel;
use myriade_engine_lib::blocks::Fluid;
use horizon_core::geometry::{trig, Aabb, Vec3};
use horizon_core::random::{LegacyRandom, RandomSource};

/// The constants of `FishingHook`: one static for every hook, never copied per entity.
pub struct Params {
    /// Gravity of the air (0.03, as a float) and the drag of a tick (0.92).
    pub gravity: f64,
    pub drag: f64,
    /// The velocity kept on entering water: x 0.3, y 0.2, z 0.3.
    pub water_drag: [f64; 3],
    /// The hook sits at this fraction of a hooked target's height (0.8).
    pub hooked_height: f64,
    /// The share of the owner's offset a pull adds (0.1), and the squared reach of the owner (32 blocks).
    pub pull: f64,
    pub reach2: f64,
    /// The life of a hook lying on a block (1 200 ticks).
    pub life: u16,
}
pub static PARAMS: Params = Params { gravity: 0.03_f32 as f64, drag: 0.92, water_drag: [0.3, 0.2, 0.3], hooked_height: 0.8, pull: 0.1, reach2: 1024.0, life: 1200 };
/// The side of the hook's box (`FishingHook` SIZE 0.25), also the width of a dropped item.
pub(crate) const SIZE: f64 = 0.25;

/// `FishingHook.state`: flying, floating, or hooked to a target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Flying,
    Bobbing,
    Hooked(Target),
}
/// What a hook holds: any entity with a box (creature, dropped item, tnt, cart, stand), or a player by id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Entity(Entity),
    Player(u32),
}

/// A hook out. Its position and velocity are the `Pos` and `Vel` components.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Hook {
    /// The constants of the fishing: one reference shared by all hooks (8 bytes).
    pub params: &'static Params,
    /// The player who cast it: the catch, the pull and the retrieve go to them (4 bytes).
    pub owner: u32,
    /// Flying, floating, or hooked (16 bytes).
    pub phase: Phase,
    /// The draws of the hook (`FishingHook.random`), seeded at the cast (8 bytes).
    pub rng: LegacyRandom,
    /// The rod's Lure, taken off the lure wait (4 bytes).
    pub lure: i32,
    /// The rod's Luck of the Sea (1 byte).
    pub luck: u8,
    /// Ticks since the cast, saturating: the target margin grows over the first 8 (1 byte).
    pub tick_count: u8,
    /// Ticks lying on a block, the hook ends at 1 200 (2 bytes).
    pub life: u16,
    /// Whether the hook lies on a block (1 byte).
    pub on_ground: bool,
    /// Ticks out of water, 0 to 10, the open-water test of the catch (1 byte).
    pub out_of_water: u8,
    /// Whether the catch is in open water (`openWater`, the treasure table) (1 byte).
    pub open_water: bool,
    /// The bite window's ticks left (4 bytes).
    pub nibble: i32,
    /// The lure's ticks left (4 bytes).
    pub time_until_lured: i32,
    /// The fish's ticks left (4 bytes).
    pub time_until_hooked: i32,
    /// The angle of the fish's path, in degrees (4 bytes).
    pub fish_angle: f32,
    /// The entity type tag that turns the hook back (`deflects_projectiles`), resolved at the cast, None if the data has none (4 bytes).
    pub deflect: Option<TagId>,
    /// The entity that last turned the hook back, to play its sound once (`lastDeflectedBy`, 8 bytes).
    pub last_deflected: Option<u64>,
}

/// `FishingHook(player, level, luck, lureSpeed)` thrown from `at` with `velocity`: the hook of `owner`, and the cast sound at its place.
/// `seed` is the hook's own draws (the caller's level random, drawn in vanilla's order).
pub fn cast(w: &mut World, owner: u32, at: Vec3, velocity: Vec3, lure: i32, luck: u8, seed: i64) -> Entity {
    let hook = Hook {
        params: &PARAMS,
        owner,
        phase: Phase::Flying,
        rng: LegacyRandom::new(seed),
        lure: lure * 100,
        luck,
        tick_count: 0,
        life: 0,
        on_ground: false,
        out_of_water: 0,
        open_water: false,
        nibble: 0,
        time_until_lured: 0,
        time_until_hooked: 0,
        fish_angle: 0.0,
        deflect: crate::type_tags::id("minecraft:deflects_projectiles"),
        last_deflected: None,
    };
    let e = w.spawn((Pos(at), Vel(velocity), hook, OwnTime)).id();
    crate::hook_wait::own(w, owner, e);
    let pitch = with_level_random(w, |r| 0.4 / (r.next_float() * 0.4 + 0.8));
    sound(w, "minecraft:entity.fishing_bobber.throw", at, 0.5, pitch);
    e
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(fly);
}

/// `hit_entity(deflects)` of hook `h` hitting the deflecting entity `c` at velocity `vel`: `Projectile.deflect` turns it back at half
/// (`ProjectileDeflection.REVERSE`) and sets `needsSync`. Returns the new velocity and whether the sound plays, once per deflecting entity.
pub fn deflect_hit(h: &mut Hook, c: Entity, vel: Vec3) -> (Vec3, bool) {
    let play = h.last_deflected != Some(c.to_bits());
    h.last_deflected = Some(c.to_bits());
    (vel.scale(-0.5), play)
}

fn sound(w: &mut World, name: &'static str, at: Vec3, volume: f32, pitch: f32) {
    w.resource_mut::<Events>().host.sound(name, [at.x, at.y, at.z], volume, pitch);
}
fn particles(w: &mut World, kind: &'static str, at: [f64; 3], spread: [f64; 3], speed: f64, count: u32) {
    w.resource_mut::<Events>().host.particles(kind, at, spread, speed, count);
}
fn take(w: &mut World, e: Entity) -> Option<Hook> {
    w.get_entity_mut(e).ok()?.take::<Hook>()
}
fn put(w: &mut World, e: Entity, h: Hook) {
    w.entity_mut(e).insert(h);
}
pub(crate) fn cell(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
/// The random of a synchronized draw (`FishingHook.syncronizedRandom`): the hook and the game time, not the level's draws.
fn sync_random(e: Entity, time: u64) -> LegacyRandom {
    LegacyRandom::new(e.to_bits() as i64 ^ time as i64)
}
/// The surface of a block cell: the fluid's height (1 over water, the level over 9 otherwise), or none when it holds no water.
fn water_height(w: &World, b: [i32; 3]) -> Option<f32> {
    let mw = w.resource::<Level>().mw.0;
    let f = mw.facts.get(mw.world.block(b[0], b[1], b[2]));
    if f.fluid != Fluid::Water {
        return None;
    }
    let above = mw.facts.get(mw.world.block(b[0], b[1] + 1, b[2]));
    Some(if above.fluid == Fluid::Water { 1.0 } else { f32::from(f.fluid_level) / 9.0 })
}
/// `FishingHook.waterBlock`: the block under a fish is water (by name, as vanilla).
fn water_block(w: &World, fish: [f64; 3]) -> bool {
    let mw = w.resource::<Level>().mw.0;
    let f = mw.facts.get(mw.world.block(fish[0].floor() as i32, (fish[1] - 1.0).floor() as i32, fish[2].floor() as i32));
    &*f.name == "minecraft:water"
}
/// `FishingHook.openWaterType`: 0 for air or a lily pad, 1 for a still source of water, 2 for anything else.
fn open_water_type(w: &World, x: i32, y: i32, z: i32) -> u8 {
    let mw = w.resource::<Level>().mw.0;
    let state = mw.world.block(x, y, z);
    let f = mw.facts.get(state);
    if mw.facts.is_air(state) || &*f.name == "minecraft:lily_pad" {
        return 0;
    }
    if f.fluid == Fluid::Water && f.fluid_level == 8 && !f.fluid_falling && !f.full_collision && !f.legacy_solid {
        1
    } else {
        2
    }
}
/// `FishingHook.calculateOpenWater`: the 5 by 5 layers from one below to two above hold no land and no air in mixed layers.
fn calculate_open_water(w: &World, b: [i32; 3]) -> bool {
    let mut previous = 2u8;
    for dy in -1..=2 {
        let mut layer = None;
        for dx in -2..=2 {
            for dz in -2..=2 {
                let t = open_water_type(w, b[0] + dx, b[1] + dy, b[2] + dz);
                layer = Some(match layer {
                    None => t,
                    Some(l) if l == t => l,
                    Some(_) => 2,
                });
            }
        }
        let layer = layer.unwrap_or(2);
        match layer {
            0 if previous == 2 => return false,
            1 if previous == 0 => return false,
            2 => return false,
            _ => {}
        }
        previous = layer;
    }
    true
}

/// The bite: the window of 20 to 40 ticks, a deadline on the wheel (`end_bite`), and the plunge of the synchronized draw.
fn trigger_bite(w: &mut World, e: Entity, h: &mut Hook, vel: &mut Vec3, pitch: f32) {
    h.nibble = 20 + h.rng.next_int_bound(21).unwrap_or(0);
    after(w, e, h.nibble, end_bite);
    push_hook(w, HookEvent::Bite { hook: e, pitch });
    let time = w.resource::<Clock>().0;
    vel.y = bite_impulse(&mut sync_random(e, time));
}

/// The bite window's deadline: the window closes and the lure starts again (`FishingHook.catchingFish` with the nibble at 0).
fn end_bite(w: &mut World, e: Entity) {
    let Some(mut h) = take(w, e) else { return };
    h.nibble = 0;
    h.time_until_lured = 0;
    h.time_until_hooked = 0;
    put(w, e, h);
}

/// The hook holds `t`: the host is told which target (`Attached`).
fn attached(w: &mut World, e: Entity, t: Target, owner: u32) {
    push_hook(w, HookEvent::Attached { hook: e, target: t, by: Who::Player(owner) });
}

fn push_hook(w: &mut World, ev: HookEvent) {
    w.init_resource::<HookOut>();
    w.resource_mut::<HookOut>().0.push(ev);
}

/// `FishingHook.catchingFish` on a hook in the water that has no counter yet: the fish's speed is drawn and the lure's wait starts.
/// A hook already waiting is visited here only in its approach, for the trail.
fn catching_fish(w: &mut World, e: Entity, h: &mut Hook, pos: Vec3, b: [i32; 3]) {
    if h.nibble > 0 {
        return;
    }
    if w.get::<Wait>(e).is_none() {
        let speed = fish_speed(w, h, b);
        let left = lured_after(&mut h.rng, h.lure);
        crate::hook_wait::arm_lure(w, e, left, speed);
    } else {
        approach_trail(w, e, h, pos);
    }
}

/// The fish's speed (`catchingFish`'s rain and sky draws): 1, one more under the rain, one less out of the sky.
fn fish_speed(w: &World, h: &mut Hook, b: [i32; 3]) -> i32 {
    let mut speed = 1;
    let above = [b[0], b[1] + 1, b[2]];
    let (raining, sky) = {
        let mw = w.resource::<Level>().mw.0;
        let light = mw.sky_light(above[0], above[1], above[2]) >= 15;
        (mw.raining && light, light)
    };
    if h.rng.next_float() < 0.25 && raining {
        speed += 1;
    }
    if h.rng.next_float() < 0.5 && !sky {
        speed -= 1;
    }
    speed
}

/// The approach's trail (`catchingFish` with the fish on its way): the path turns, the fish's place is `approach_distance` away.
fn approach_trail(w: &mut World, e: Entity, h: &mut Hook, pos: Vec3) {
    let left = crate::hook_wait::left_now(w, e);
    h.fish_angle += h.rng.triangle(0.0, 9.188) as f32;
    let angle = h.fish_angle * (std::f64::consts::PI / 180.0) as f32;
    let (sin, cos) = (trig::sin(f64::from(angle)), trig::cos(f64::from(angle)));
    let distance = approach_distance(left);
    let fish = [pos.x + f64::from(sin * distance), f64::from(pos.y.floor() as f32 + 1.0), pos.z + f64::from(cos * distance)];
    if water_block(w, fish) {
        if h.rng.next_float() < 0.15 {
            let spread = [f64::from(sin), 0.1, f64::from(cos)];
            particles(w, "bubble", [fish[0], fish[1] - f64::from(0.1_f32), fish[2]], spread, 0.0, 1);
        }
        let (x, z) = (sin * 0.04, cos * 0.04);
        for side in [1.0_f32, -1.0] {
            particles(w, "fishing", fish, [f64::from(z * side), 0.01, f64::from(-x * side)], 1.0, 0);
        }
    }
}

/// A splash of the lure (`catchingFish` with the tease drawn): a place 25 to 60 blocks out, a splash there if it is water.
pub(crate) fn lure_splash(w: &mut World, e: Entity) {
    let Some(mut h) = take(w, e) else { return };
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let angle = (h.rng.next_float() * 360.0) * (std::f64::consts::PI / 180.0) as f32;
    let distance = h.rng.next_float() * 35.0 + 25.0;
    let fish = [pos.x + f64::from(trig::sin(f64::from(angle)) * distance) * 0.1, f64::from(pos.y.floor() as f32 + 1.0), pos.z + f64::from(trig::cos(f64::from(angle)) * distance) * 0.1];
    if water_block(w, fish) {
        let count = 2 + h.rng.next_int_bound(2).unwrap_or(0) as u32;
        particles(w, "splash", fish, [f64::from(0.1_f32), 0.0, f64::from(0.1_f32)], 0.0, count);
    }
    put(w, e, h);
}

/// The fish's speed read again for a hook in the water: an event (rain or sky changed), never a tick.
fn refresh_speed(w: &mut World, e: Entity) {
    let Some(mut h) = take(w, e) else { return };
    let b = w.get::<Pos>(e).map_or([0; 3], |p| cell(p.0));
    let speed = fish_speed(w, &mut h, b);
    put(w, e, h);
    crate::hook_wait::speed_changed(w, e, speed);
}
/// The weather changed (rain or none): the hooks waiting in the water read their fish's speed again.
pub fn weather_changed(w: &mut World) {
    let hooks: Vec<Entity> = w.query_filtered::<Entity, With<Wait>>().iter(w).collect();
    for e in hooks {
        refresh_speed(w, e);
    }
}
/// A block changed at `c`: the hooks whose sky is that block read their fish's speed again.
pub fn block_changed(w: &mut World, c: [i32; 3]) {
    for e in crate::hook_wait::waiting_at(w, [c[0], c[1] - 1, c[2]]) {
        crate::hook_wait::leave_water(w, e);
    }
}

/// The approach's end (`FishingHook.catchingFish` with the fish at its place): the splash, the bite window and the plunge.
pub(crate) fn bite_due(w: &mut World, e: Entity) {
    let Some(mut h) = take(w, e) else { return };
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let mut vel = w.get::<Vel>(e).map_or(Vec3::ZERO, |v| v.0);
    let pitch = 1.0 + (h.rng.next_float() - h.rng.next_float()) * 0.4;
    sound(w, "minecraft:entity.fishing_bobber.splash", pos, 0.25, pitch);
    let at = [pos.x, pos.y + 0.5, pos.z];
    let count = (1.0 + SIZE as f32 * 20.0) as u32;
    let spread = [SIZE, 0.0, SIZE];
    particles(w, "bubble", at, spread, f64::from(0.2_f32), count);
    particles(w, "fishing", at, spread, f64::from(0.2_f32), count);
    trigger_bite(w, e, &mut h, &mut vel, pitch);
    w.entity_mut(e).insert(Vel(vel));
    put(w, e, h);
}

/// The creature, player or dropped item whose box the path `from`..`to` enters, nearest first; the owner and the spectators are skipped.
fn first_hit(w: &World, owner: u32, from: Vec3, to: Vec3, margin: f64) -> Option<(Vec3, Target)> {
    let level = *w.resource::<Level>();
    let (mx, mz) = ((from.x + to.x) / 2.0, (from.z + to.z) / 2.0);
    let r = from.distance(to) / 2.0 + 1.5;
    let mut cands = Vec::new();
    w.resource::<Index>().for_radius(mx, mz, r, |c| cands.push(c));
    let mut best: Option<(f64, Vec3, Target)> = None;
    let mut keep = |d: f64, at: Vec3, t: Target| {
        if best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, at, t));
        }
    };
    for c in cands {
        let Some(b) = w.get::<Bounds>(c) else { continue };
        if w.get::<Health>(c).is_some_and(|hp| hp.0 <= 0.0) {
            continue;
        }
        if let Some(at) = entity_hit(&level, b.0, from, to, margin) {
            keep(from.distance(at), at, Target::Entity(c));
        }
    }
    let area = Aabb::new(Vec3::new(from.x.min(to.x) - 1.0, from.y.min(to.y) - 1.0, from.z.min(to.z) - 1.0), Vec3::new(from.x.max(to.x) + 1.0, from.y.max(to.y) + 1.0, from.z.max(to.z) + 1.0));
    for c in w.resource::<ItemIndex>().near(area) {
        let Some(b) = w.get::<Bounds>(c) else { continue };
        if let Some(at) = entity_hit(&level, b.0, from, to, margin) {
            keep(from.distance(at), at, Target::Entity(c));
        }
    }
    let ps = w.resource::<PlayerState>();
    let mut near = Vec::new();
    w.resource::<Online>().for_near(mx, mz, r, |p| near.push(p));
    for p in near {
        if p.id == owner || ps.spectator(p.id) {
            continue;
        }
        if let Some(at) = entity_hit(&level, player_box(&p), from, to, margin) {
            keep(from.distance(at), at, Target::Player(p.id));
        }
    }
    best.map(|(_, at, t)| (at, t))
}

/// The feet and the height of a hooked target (`FishingHook.tick` on a hooked state), or none when it is gone.
fn hooked_at(w: &World, t: Target) -> Option<(Vec3, f64)> {
    match t {
        Target::Entity(c) => {
            if w.get::<Health>(c).is_some_and(|hp| hp.0 <= 0.0) {
                return None;
            }
            let b = w.get::<Bounds>(c)?.0;
            Some((w.get::<Pos>(c)?.0, b.max_position().y - b.min_position().y))
        }
        Target::Player(id) => {
            let p = w.resource::<Online>().get(id)?;
            let b = player_box(&p);
            let (lo, hi) = (b.min_position(), b.max_position());
            Some((Vec3::new((lo.x + hi.x) / 2.0, lo.y, (lo.z + hi.z) / 2.0), hi.y - lo.y))
        }
    }
}

/// One tick of every hook out (`FishingHook.tick`): the range and the life, then the phase. A hooked one follows its target and
/// stops; a floating one bobs and waits; a flying one enters the water, meets a target, or moves.
fn fly(world: &mut World) {
    if !sparse_any::<Hook>(world) {
        return;
    }
    let hooks: Vec<Entity> = world.query_filtered::<Entity, (With<Hook>, Or<(Without<Wait>, With<Approach>)>)>().iter(world).collect();
    for e in hooks {
        step(world, e);
    }
}
fn step(w: &mut World, e: Entity) {
    let Some(mut h) = take(w, e) else { return };
    let params = h.params;
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let mut vel = w.get::<Vel>(e).map_or(Vec3::ZERO, |v| v.0);
    let Some(owner) = w.resource::<Online>().get(h.owner) else {
        w.despawn(e);
        return;
    };
    if pos.distance_squared(Vec3::new(owner.x, owner.y, owner.z)) > params.reach2 {
        w.despawn(e);
        return;
    }
    h.tick_count = h.tick_count.saturating_add(1);
    if h.on_ground {
        h.life += 1;
        if h.life >= params.life {
            w.despawn(e);
            return;
        }
    } else {
        h.life = 0;
    }
    let b = cell(pos);
    let liquid = water_height(w, b).unwrap_or(0.0);
    let in_water = liquid > 0.0;
    match h.phase {
        Phase::Hooked(t) => {
            match hooked_at(w, t) {
                Some((at, height)) => {
                    w.entity_mut(e).insert(Pos(Vec3::new(at.x, at.y + height * params.hooked_height, at.z)));
                }
                None => h.phase = Phase::Flying,
            }
            return put(w, e, h);
        }
        Phase::Flying if in_water => {
            let d = params.water_drag;
            vel = Vec3::new(vel.x * d[0], vel.y * d[1], vel.z * d[2]);
            h.phase = Phase::Bobbing;
            w.entity_mut(e).insert(Vel(vel));
            return put(w, e, h);
        }
        _ => {}
    }
    let level = *w.resource::<Level>();
    let mw = level.mw.0;
    if h.phase == Phase::Flying {
        let block = clip_block_hit(mw.world, mw.facts, mw.collision, pos, vel);
        let end = block.map_or(pos.add_vector(vel), |hit| hit.0);
        match first_hit(w, h.owner, pos, end, path_margin(h.tick_count)) {
            Some((_, Target::Entity(c))) if h.deflect.is_some_and(|tag| crate::type_tags::has(w, tag, c)) => {
                let (turned, play) = deflect_hit(&mut h, c, vel);
                vel = turned;
                if play {
                    let at = w.get::<Pos>(c).map_or(Vec3::ZERO, |p| p.0);
                    sound(w, "minecraft:entity.breeze.deflect", at, 1.0, 1.0);
                }
            }
            Some((_, t)) => {
                h.phase = Phase::Hooked(t);
                vel = Vec3::ZERO;
                attached(w, e, t, h.owner);
            }
            None => {}
        }
    }
    let mut armed = false;
    if h.phase == Phase::Bobbing {
        // The floating force: the hook is pushed up to the surface (`surface`), the water fights its fall.
        let force = buoyancy_force(pos.y, vel.y, b[1], liquid);
        let r = f64::from(h.rng.next_float());
        vel = Vec3::new(vel.x * 0.9, vel.y - force * r * 0.2, vel.z * 0.9);
        if h.nibble <= 0 && !crate::hook_wait::approaching(w, e) {
            h.open_water = true;
        } else {
            h.open_water = h.open_water && h.out_of_water < 10 && calculate_open_water(w, b);
        }
        if in_water {
            h.out_of_water = h.out_of_water.saturating_sub(1);
            if h.nibble > 0 {
                let time = w.resource::<Clock>().0;
                let mut s = sync_random(e, time);
                vel.y += -0.1 * f64::from(s.next_float()) * f64::from(s.next_float());
            }
            let waiting = w.get::<Wait>(e).is_some();
            catching_fish(w, e, &mut h, pos, b);
            armed = !waiting && w.get::<Wait>(e).is_some();
        } else {
            crate::hook_wait::leave_water(w, e);
            h.out_of_water = (h.out_of_water + 1).min(10);
        }
    }
    if !in_water && !h.on_ground {
        vel.y -= params.gravity;
    }
    let block = clip_block_hit(mw.world, mw.facts, mw.collision, pos, vel);
    let end = block.map_or(pos.add_vector(vel), |hit| hit.0);
    if block.is_some() {
        if h.phase == Phase::Flying {
            vel = Vec3::ZERO;
        }
        h.on_ground = vel.y < 0.0;
    }
    let k = params.drag;
    w.entity_mut(e).insert((Pos(end), Vel(Vec3::new(vel.x * k, vel.y * k, vel.z * k))));
    if armed {
        crate::hook_wait::settle(w, e, b[1], liquid);
    }
    put(w, e, h);
}

/// The pull of a hooked target towards its owner: the offset from the hook to the owner times 0.1 (`pullEntity`).
pub fn pull_offset(w: &World, owner: u32, at: Vec3) -> [f64; 3] {
    let Some(p) = w.resource::<Online>().get(owner) else { return [0.0; 3] };
    pull_toward([p.x, p.y, p.z], at, PARAMS.pull)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new();
        w.init_resource::<Events>();
        w.init_resource::<Clock>();
        w.init_resource::<Wheel>();
        w
    }
    fn test_hook(owner: u32) -> Hook {
        Hook {
            params: &PARAMS,
            owner,
            phase: Phase::Bobbing,
            rng: LegacyRandom::new(1),
            lure: 0,
            luck: 0,
            tick_count: 0,
            life: 0,
            on_ground: false,
            out_of_water: 0,
            open_water: false,
            nibble: 0,
            time_until_lured: 0,
            time_until_hooked: 0,
            fish_angle: 0.0,
            deflect: None,
            last_deflected: None,
        }
    }

    #[test]
    fn bite_window_closes_at_its_deadline_on_the_wheel() {
        let mut w = world();
        let e = w.spawn_empty().id();
        let mut h = test_hook(7);
        let mut vel = Vec3::ZERO;
        trigger_bite(&mut w, e, &mut h, &mut vel, 1.0);
        let window = h.nibble;
        assert!((20..=40).contains(&window));
        assert_eq!(w.resource::<Wheel>().due_len(window as u64), 1);
        put(&mut w, e, h);
        end_bite(&mut w, e);
        let h = w.get::<Hook>(e).unwrap();
        assert_eq!((h.nibble, h.time_until_lured, h.time_until_hooked), (0, 0, 0));
    }

    #[test]
    fn bite_and_hold_are_pushed_to_the_host() {
        let mut w = world();
        let e = w.spawn_empty().id();
        let mut h = test_hook(7);
        let mut vel = Vec3::ZERO;
        trigger_bite(&mut w, e, &mut h, &mut vel, 1.0);
        attached(&mut w, e, Target::Player(8), 7);
        let out = std::mem::take(&mut w.resource_mut::<HookOut>().0);
        assert!(matches!(out[0], HookEvent::Bite { hook, .. } if hook == e));
        assert_eq!(out[1], HookEvent::Attached { hook: e, target: Target::Player(8), by: Who::Player(7) });
    }
}
