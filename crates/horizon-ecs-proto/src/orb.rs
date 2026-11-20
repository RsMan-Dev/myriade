//! The experience orb (`Orb`): a value that a player collects. An entity of its own kind like the item on the ground: a position, a velocity,
//! a box and a body of an object (`Body.orb`: `ExperienceOrb.tick`'s motion), plus `Orb`. It sleeps at rest (no `Moving`: zero cost); it is
//! drawn towards a player within 8 blocks (a system that visits only the orbs a player is near: `PlayerWatch`, `Attracted`); stacks of the
//! same value merge (`scanForMerges`: every 20 ticks from its tick 1, at its own deadline, asked again only while another orb of its
//! value lies within reach); it ends at 6 000 ticks (a deadline of the wheel). A death drops its experience as orbs (`award`).
use crate::cow::*;
use crate::goals::{Events, Online, PlayerWatch};
use bevy_ecs::prelude::*;
use horizon_client_lib::entities::ExperienceOrb;
use horizon_core::geometry::{Aabb, Vec3};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A multiplicative hash for the small integer keys of the orb index (SipHash costs ten times more per lookup).
#[derive(Default)]
pub struct Fx(u64);
impl Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }
    fn write_i32(&mut self, i: i32) {
        self.write_u64(i as u32 as u64);
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}
type Cells = HashMap<[i32; 3], Vec<Entity>, BuildHasherDefault<Fx>>;

/// An orb: its value, how many merged into it, when it was made (own time), and what it follows.
#[derive(Component, Clone, Copy, Debug)]
#[component(on_remove = leave_index)]
pub struct Orb {
    pub value: i32,
    pub count: u32,
    pub born: u64,
    /// `age = now - age_base` (a merge takes the smaller age).
    pub age_base: u64,
    pub id: u32,
    pub cell: [i32; 3],
    /// `ExperienceOrb.followingPlayer`.
    pub following: Option<u32>,
    expire: u32,
    check: u32,
}
/// A player is within 8 blocks of this orb: the attraction system visits it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Attracted;
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = awake_added, on_remove = awake_removed)]
pub struct OrbAwake;
fn awake_added(mut w: bevy_ecs::world::DeferredWorld, _: bevy_ecs::lifecycle::HookContext) {
    w.resource_mut::<OrbIndex>().awake += 1;
}
fn awake_removed(mut w: bevy_ecs::world::DeferredWorld, _: bevy_ecs::lifecycle::HookContext) {
    w.resource_mut::<OrbIndex>().awake -= 1;
}
/// The hook of `Moving`: an orb that starts to move is listed for the orb system.
pub fn moving_added(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    if world.get::<Orb>(ctx.entity).is_some() {
        world.commands().entity(ctx.entity).insert(OrbAwake);
    }
}
#[derive(Resource, Default)]
pub struct OrbIndex {
    cells: Cells,
    next_id: u32,
    token: u32,
    awake: usize,
}
impl OrbIndex {
    fn near(&self, area: Aabb) -> Vec<Entity> {
        let (min, max) = (area.min_position(), area.max_position());
        let (lo, hi) = (cell_of(Vec3::new(min.x - 0.25, min.y, min.z - 0.25)), cell_of(Vec3::new(max.x + 0.25, max.y, max.z + 0.25)));
        let mut out = Vec::new();
        for x in lo[0]..=hi[0] {
            for y in lo[1] - 1..=hi[1] {
                for z in lo[2]..=hi[2] {
                    if let Some(v) = self.cells.get(&[x, y, z]) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
        out
    }
    fn token(&mut self) -> u32 {
        self.token += 1;
        self.token
    }
}
fn cell_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn leave_index(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    if let Some(c) = world.get::<Orb>(ctx.entity).map(|o| o.cell) {
        let mut ix = world.resource_mut::<OrbIndex>();
        if let Some(v) = ix.cells.get_mut(&c) {
            v.retain(|&x| x != ctx.entity);
        }
    }
}
const LIFE: u64 = 6000;
/// The reach of the attraction: `getNearestPlayer(this, 8.0)`.
const REACH: f64 = 8.0;
pub fn insert_resources(w: &mut World) {
    w.insert_resource(OrbIndex::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(orb_attract.after(due).before(travel));
    s.add_systems(orb_moved.after(travel).before(reindex));
}

/// `new ExperienceOrb(level, pos, motion, value)` in the world.
pub fn spawn_orb(world: &mut World, value: i32, count: u32, at: Vec3, velocity: Vec3) -> Entity {
    let id = {
        let mut ix = world.resource_mut::<OrbIndex>();
        ix.next_id += 1;
        ix.next_id
    };
    let cell = cell_of(at);
    let e = world
        .spawn((
            Pos(at),
            Vel(velocity),
            crate::goals::Uid(crate::goals::next_uid()),
            OwnTime,
            PushFree,
            Body { width: 0.5, height: 0.5, step_height: 0.0, gravity: 0.03, speed: 0.0, fall_factor: 1.0, object: true, orb: true, lava_walker: false },
            Bounds(ExperienceOrb::box_at(at)),
            Cell(u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0 },
            FluidParams { eye_height: f64::from(0.425_f32), can_float: false, water_slowdown: 0.8, water_efficiency: 0.0 },
            FluidState::default(),
            Recheck,
            Moving,
        ))
        .id();
    world.entity_mut(e).insert((
        Sizes { adult: (0.5, 0.5), baby: (0.5, 0.5) },
        Health(5.0),
        HurtAt { at: 0, last: 0.0 },
        Frozen(0),
        Air(300),
        Resist(0.0),
        Immune(BREATHES_WATER | NO_HURT_COOLDOWN),
        TypeName("minecraft:experience_orb"),
    ));
    let now = now_of(world, e);
    world.entity_mut(e).insert(OrbAwake);
    world.entity_mut(e).insert(Orb { value, count, born: now, age_base: now, id, cell, following: None, expire: 0, check: 0 });
    world.resource_mut::<OrbIndex>().cells.entry(cell).or_default().push(e);
    // A player already within reach: the attraction starts with its first tick (the watch tells only about what comes later).
    let mut near = false;
    world.resource::<Online>().for_near(at.x, at.z, REACH, |p| near |= (p.x - at.x).powi(2) + (p.y - at.y).powi(2) + (p.z - at.z).powi(2) <= REACH * REACH);
    // The watch tells when a player comes; an orb already drawn does not need it (it looks again at its tick 1 modulo 20).
    if near {
        world.entity_mut(e).insert(Attracted);
    } else {
        world.entity_mut(e).insert(PlayerWatch::new(REACH as f32));
    }
    enter_index(world, e);
    arm_expire(world, e);
    arm_check(world, e);
    e
}
/// `ExperienceOrb.award(level, pos, amount)`: the amount in orbs of the largest sizes, each merged into a stack of the same value that
/// is there (one the draw of the slot picks) or made new, thrown a little (`random` is the level's).
pub fn award(world: &mut World, at: Vec3, mut amount: u32, random: &mut impl horizon_core::random::RandomSource) {
    while amount > 0 {
        let v = ExperienceOrb::split(amount);
        amount -= v;
        let slot = random.next_int_bound(40).unwrap_or(0);
        let area = Aabb::new(Vec3::new(at.x - 0.5, at.y - 0.5, at.z - 0.5), Vec3::new(at.x + 0.5, at.y + 0.5, at.z + 0.5));
        let near = world.resource::<OrbIndex>().near(area);
        let mut found = None;
        for o in near {
            let (Some(orb), Some(p)) = (world.get::<Orb>(o), world.get::<Pos>(o)) else { continue };
            if ExperienceOrb::new(orb.id, p.0, Vec3::ZERO, orb.value).can_merge(slot as u32, v as i32) && ExperienceOrb::box_at(p.0).intersects(area) {
                found = Some(o);
                break;
            }
        }
        match found {
            Some(o) => {
                let now = now_of(world, o);
                let mut orb = world.get_mut::<Orb>(o).unwrap();
                orb.count += 1;
                orb.age_base = now;
                arm_expire(world, o);
            }
            None => {
                let vel = Vec3::new((random.next_double() * 0.2 - 0.1) * 2.0, random.next_double() * 0.2 * 2.0, (random.next_double() * 0.2 - 0.1) * 2.0);
                spawn_orb(world, v as i32, 1, at, vel);
            }
        }
    }
}
/// A player collects one of the orb: its value, or `None` for an orb with no experience (`playerTouch`); the stack is one less (gone at 0).
pub fn collect(world: &mut World, orb: Entity) -> Option<u32> {
    let mut o = world.get_mut::<Orb>(orb)?;
    let value = o.value.max(0) as u32;
    if o.count <= 1 {
        world.entity_mut(orb).despawn();
    } else {
        o.count -= 1;
    }
    Some(value)
}

fn arm_expire(world: &mut World, e: Entity) {
    let now = now_of(world, e);
    let token = world.resource_mut::<OrbIndex>().token();
    let Some(mut o) = world.get_mut::<Orb>(e) else { return };
    o.expire = token;
    let at = o.age_base + LIFE;
    arm_at(world, e, at.min(now + 4000).max(now + 1), Due::OrbExpire(token));
}
pub fn expire_due(world: &mut World, e: Entity, token: u32) {
    let Some(o) = world.get::<Orb>(e) else { return };
    if o.expire != token {
        return;
    }
    if now_of(world, e) < o.age_base + LIFE {
        arm_expire(world, e);
        return;
    }
    world.entity_mut(e).despawn();
}
/// The next tick of the orb that is 1 modulo 20 (`ExperienceOrb.tick`: `tickCount % 20 == 1` scans for merges and looks for a player).
fn arm_check(world: &mut World, e: Entity) {
    let now = now_of(world, e);
    let token = world.resource_mut::<OrbIndex>().token();
    let Some(mut o) = world.get_mut::<Orb>(e) else { return };
    o.check = token;
    let t = now - o.born;
    let to = (21 - t % 20 - 1) % 20 + 1;
    arm_at(world, e, now + to, Due::OrbCheck(token));
}
pub fn check_due(world: &mut World, e: Entity, token: u32) {
    let Some(o) = world.get::<Orb>(e) else { return };
    if o.check != token {
        return;
    }
    world.get_mut::<Orb>(e).unwrap().check = 0;
    // A moving orb scans in `orb_moved`, after its move (`ExperienceOrb.tick` moves, then scans) and with the re-arm there.
    if world.get::<OrbAwake>(e).is_some() {
        return;
    }
    scan_for_merges(world, e);
    if world.get::<Orb>(e).is_some() && !partners(world, e).is_empty() {
        arm_check(world, e);
    }
}
fn partners(world: &World, e: Entity) -> Vec<Entity> {
    let (Some(o), Some(p)) = (world.get::<Orb>(e), world.get::<Pos>(e)) else { return Vec::new() };
    let area = ExperienceOrb::box_at(p.0).inflate(Vec3::new(0.5, 0.5, 0.5));
    world
        .resource::<OrbIndex>()
        .near(area)
        .into_iter()
        .filter(|&x| x != e)
        .filter(|&x| {
            let (Some(q), Some(b)) = (world.get::<Pos>(x), world.get::<Orb>(x)) else { return false };
            ExperienceOrb::box_at(q.0).intersects(area) && b.value == o.value && (b.id as i32).wrapping_sub(o.id as i32) % 40 == 0
        })
        .collect()
}
/// `ExperienceOrb.scanForMerges` and `merge`: every orb of the same value (ids a multiple of 40 apart) in the box grown by 0.5 joins this one.
fn scan_for_merges(world: &mut World, e: Entity) {
    let now = now_of(world, e);
    for o in partners(world, e) {
        let (c, age) = {
            let b = world.get::<Orb>(o).unwrap();
            (b.count, now - b.age_base)
        };
        let me = world.get_mut::<Orb>(e).unwrap().into_inner();
        me.count += c;
        me.age_base = now.saturating_sub((now - me.age_base).min(age));
        world.entity_mut(o).despawn();
    }
    if world.get::<Orb>(e).is_some() {
        arm_expire(world, e);
    }
}

/// The attraction: a player within 8 blocks (the watch says when one comes) draws the orb each tick it follows it. It follows the nearest
/// one found at its tick 1 modulo 20, and lets it go when it is further than 8 blocks.
fn orb_attract(on: Res<Online>, ev: Res<Events>, clock: Res<Clock>, mut q: Query<(Entity, &Pos, &mut Vel, &mut Orb, Has<Moving>), With<Attracted>>, mut commands: Commands, watch: Query<(), With<Orb>>) {
    for (e, _) in ev.player_entered.iter() {
        if watch.get(e).is_ok() {
            // Drawn: the watch is not needed until it lets go (the look of its tick 1 modulo 20 puts it back).
            commands.entity(e).insert((Attracted, Moving)).remove::<PlayerWatch>();
        }
    }
    for (e, pos, mut vel, mut orb, moving) in &mut q {
        let ticks = clock.of(e) - orb.born;
        if let Some(id) = orb.following
            && on.get(id).is_none_or(|p| (p.x - pos.0.x).powi(2) + (p.y - pos.0.y).powi(2) + (p.z - pos.0.z).powi(2) > REACH * REACH)
        {
            orb.following = None;
        }
        if orb.following.is_none() && ticks % 20 == 1 {
            let mut best: Option<(u32, f64)> = None;
            on.for_near(pos.0.x, pos.0.z, REACH, |p| {
                let d = (p.x - pos.0.x).powi(2) + (p.y - pos.0.y).powi(2) + (p.z - pos.0.z).powi(2);
                if d <= REACH * REACH && best.is_none_or(|b| d < b.1) {
                    best = Some((p.id, d));
                }
            });
            orb.following = best.map(|b| b.0);
        }
        let Some(p) = orb.following.and_then(|id| on.get(id)) else {
            // Nobody to follow after the look of its tick 1: the watch puts it back in the list when a player comes.
            if ticks % 20 == 1 {
                commands.entity(e).remove::<Attracted>().insert(PlayerWatch::new(REACH as f32));
            }
            continue;
        };
        let d = Vec3::new((p.x - pos.0.x) / REACH, (p.y + 0.81 - pos.0.y) / REACH, (p.z - pos.0.z) / REACH);
        let length = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
        let power = 1.0 - length;
        if power > 0.0 && length > 1e-6 {
            let power = power * power;
            vel.0 = Vec3::new(vel.0.x + d.x / length * power * 0.1, vel.0.y + d.y / length * power * 0.1, vel.0.z + d.z / length * power * 0.1);
            if !moving {
                commands.entity(e).insert(Moving);
            }
        }
    }
}
/// Each tick, the orbs that move: the cell they are in, and the tick-1-modulo-20 scan for merges.
fn orb_moved(world: &mut World, mut cached: Local<Option<QueryState<(Entity, &Orb, &Pos, Has<Moving>), With<OrbAwake>>>>) {
    if world.resource::<OrbIndex>().awake == 0 {
        return;
    }
    let state = cached.get_or_insert_with(|| world.query_filtered());
    // One pass reads what the loop needs; the order is the orbs' ids.
    let mut list: Vec<(u32, Entity, Vec3, bool, [i32; 3], u64, bool)> =
        state.iter(world).map(|(e, o, p, m)| (o.id, e, p.0, m, o.cell, o.born, o.check != 0)).collect();
    list.sort_unstable_by_key(|l| l.0);
    for (_, e, p, moving, old, born, pending) in list {
        // An orb merged away by an earlier scan of this pass is gone.
        if world.get::<Orb>(e).is_none() {
            continue;
        }
        let now = now_of(world, e);
        let cell = cell_of(p);
        let ticks = now - born;
        if old != cell {
            let mut ix = world.resource_mut::<OrbIndex>();
            if let Some(v) = ix.cells.get_mut(&old) {
                v.retain(|&x| x != e);
            }
            ix.cells.entry(cell).or_default().push(e);
            world.get_mut::<Orb>(e).unwrap().cell = cell;
        }
        if ticks % 20 == 1 {
            scan_for_merges(world, e);
        }
        if !pending && world.get::<Orb>(e).is_some() {
            arm_check(world, e);
        }
        if !moving && world.get::<Orb>(e).is_some() {
            world.entity_mut(e).remove::<OrbAwake>();
        }
    }
}

/// Present in a bench world: births give no experience and deaths drop nothing (`loot::on_death`). A bench player never collects, and an orb that nobody collects orbits its
/// nearest player for its 6 000 ticks (some 40 000 instructions per orb and tick in a dense herd; the `Mob`'s orb costs 14 000 at rest),
/// which a game does not show: the orb is collected within seconds. The benches set it (`NO_XP=1`).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct NoXp;
