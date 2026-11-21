//! Shared by the tests that compare the ecs with a real `Mob` in the same scenario (`HORIZON_TEST_RESOURCES` set).
#![allow(dead_code)]
use bevy_ecs::prelude::*;
use horizon_client_lib::{
    health::TestLevel,
    mobs::{self, Damage, DamageKind, Mob, MobEvent, MobWorld, PendingPushes, PlayerView, Seen},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    cow::*,
    goals::*,
};
use std::sync::Arc;

pub struct MobRef {
    pub w: &'static MobWorld<'static>,
    pub mobs: Vec<Mob>,
    pub tl: TestLevel,
    pub pushes: PendingPushes,
    pub t: i64,
    pub kind: Arc<mobs::MobType>,
    pub events: Vec<MobEvent>,
    pub level: Level,
    applied: usize,
    /// The path searches of each mob (by the counter around its own tick).
    pub searches: Vec<u64>,
}
impl MobRef {
    pub fn new(half: i32, patches: bool) -> Self {
        let (level, _s, kind) = build_level(half, patches);
        Self { w: level.mw.0, mobs: Vec::new(), tl: TestLevel::new(7), pushes: PendingPushes::default(), t: 0, kind, events: Vec::new(), level, applied: 0, searches: Vec::new() }
    }
    pub fn add(&mut self, at: Vec3) -> usize {
        let mut m = Mob::new(self.mobs.len() as u32 + 1, self.kind.clone(), at, 0.0);
        m.persistent = true;
        m.ai.no_action_time = 100;
        self.mobs.push(m);
        self.searches.push(0);
        self.mobs.len() - 1
    }
    /// A mob with the id given: the brain's gates shuffle with a random seeded by the id, and the first draws of Java's generator
    /// are correlated for neighbouring seeds (sequential ids make the golems choose alike); vanilla seeds from the level random.
    pub fn add_id(&mut self, at: Vec3, id: u32) -> usize {
        let mut m = Mob::new(id, self.kind.clone(), at, 0.0);
        m.persistent = true;
        m.ai.no_action_time = 100;
        self.mobs.push(m);
        self.searches.push(0);
        self.mobs.len() - 1
    }
    /// A mob of another type (the process's species is `self.kind`).
    pub fn add_kind(&mut self, at: Vec3, kind: Arc<mobs::MobType>) -> usize {
        let mut m = Mob::new(self.mobs.len() as u32 + 1, kind, at, 0.0);
        m.persistent = true;
        m.ai.no_action_time = 100;
        scope(&mut m);
        self.mobs.push(m);
        self.searches.push(0);
        self.mobs.len() - 1
    }
    /// `add_kind` with the id given (the brain's random is seeded by it): the same scene under another draw.
    pub fn add_kind_id(&mut self, at: Vec3, kind: Arc<mobs::MobType>, id: u32) -> usize {
        let mut m = Mob::new(id, kind, at, 0.0);
        m.persistent = true;
        m.ai.no_action_time = 100;
        scope(&mut m);
        self.mobs.push(m);
        self.searches.push(0);
        self.mobs.len() - 1
    }
    pub fn names(&self, i: usize) -> Vec<&'static str> {
        self.mobs[i].ai.goals.running_names()
    }
    /// A blow of a player standing at `from`: damage and knockback away from it (`Herd::hit` on the ecs side).
    pub fn hit(&mut self, i: usize, amount: f32, from: [f64; 3]) {
        let mut lvl = self.tl.level();
        self.mobs[i].damage(&mut lvl, amount, Damage::player(from));
    }
    /// A blow of the mob `by` (an index in `mobs`) on `i`.
    pub fn hit_by_mob(&mut self, i: usize, amount: f32, by: usize) {
        let (id, from) = (self.mobs[by].id, [self.mobs[by].position.x, self.mobs[by].position.y, self.mobs[by].position.z]);
        let mut lvl = self.tl.level();
        self.mobs[i].damage(&mut lvl, amount, Damage::mob(id, from));
    }
    pub fn step(&mut self, player: Option<&PlayerView>) {
        self.step_where(player, |_| true);
    }
    /// A tick where only the mobs `ticks` accepts are ticked (the others stay in the snapshot, as in the game's pass over a
    /// chunk that does not tick).
    pub fn step_where(&mut self, player: Option<&PlayerView>, ticks: impl Fn(&Mob) -> bool) {
        self.t += 1;
        let items = self.w.items.expect("items");
        let mut snapshot: Vec<Seen> = self.mobs.iter().map(|m| m.seen(None, items)).collect();
        let mut grid = mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
        let mut lvl = self.tl.level();
        for slot in 0..self.mobs.len() {
            if !ticks(&self.mobs[slot]) {
                continue;
            }
            mobs::apply_mob_pushes(&mut self.mobs[slot], &mut self.pushes);
            let from = self.events.len();
            let view = MobWorld {
                others: &snapshot,
                others_grid: Some(&grid),
                others_solid: Some(&[]),
                nearest_player: player.map(|p| p.position),
                player: player.cloned(),
                game_time: self.t,
                ..self.w.clone()
            };
            let before = horizon_client_lib::mobs::nav::FINDER_CALLS.load(std::sync::atomic::Ordering::Relaxed);
            let kept = mobs::ai::tick(&mut self.mobs[slot], &view, &mut lvl, &mut self.events);
            self.searches[slot] += horizon_client_lib::mobs::nav::FINDER_CALLS.load(std::sync::atomic::Ordering::Relaxed) - before;
            mobs::take_mob_pushes(&mut self.events, from, &mut self.pushes);
            let mut s = self.mobs[slot].seen(None, items);
            s.alive &= kept;
            s.removed |= !kept;
            grid.update(slot, s.bounds);
            snapshot[slot] = s;
        }
        self.apply_block_events();
    }
}
/// The goals of a skeleton the ECS has (the bow, stroll, the two looks; hurt-by and the nearest player), with the bow in
/// hand: `RestrictSun` (its paths avoid the sun), `FleeSun` and `AvoidEntity` are not ported, and a path that avoids the
/// sun is not the path of the ECS (the cause of the stroll control's gap for the skeleton).
pub fn scope(m: &mut Mob) {
    use horizon_client_lib::mobs::goals::*;
    if m.species() != horizon_client_lib::mobs::Species::Skeleton {
        return;
    }
    m.extra.set_main_hand(Some("minecraft:bow".into()));
    m.ai.goals.clear();
    m.ai.targets.clear();
    m.ai.goals.add(4, horizon_client_lib::mobs::undead::SkeletonWeapon::new(20, 40));
    m.ai.goals.add(5, Stroll::avoiding_water(1.0));
    m.ai.goals.add(6, LookAtPlayer::new(8.0));
    m.ai.goals.add(6, RandomLookAround::default());
    m.ai.targets.add(1, HurtByTarget::new());
    m.ai.targets.add(2, NearestTarget::player(true));
}
pub fn player_view(at: Vec3, item: Option<&str>) -> PlayerView {
    PlayerView {
        position: at,
        bounds: Aabb::new(Vec3::new(at.x - 0.3, at.y, at.z - 0.3), Vec3::new(at.x + 0.3, at.y + 1.8, at.z + 0.3)),
        eye: [at.x, at.y + 1.62, at.z],
        alive: true,
        invulnerable: false,
        creative: false,
        sneaking: false,
        swimming: false,
        look: [0.0; 3],
        main_hand: item.map(Into::into),
        off_hand: None,
        head: None,
        chest: None,
        legs: None,
        feet: None,
        armor_cover: 0.0,
        invisible: false,
        spectator: false,
        gold_armor: false,
        hurt_by: None,
        last_hurt: None,
        velocity: Vec3::ZERO,
        yaw: 0.0,
        sprinting: false,
        riding: false,
        hero: false,
        effects: Vec::new(),
        uuid: [0; 4],
        allies: Default::default(),
        remote: false,
        sleeping: false,
        sleep_timer: 0,
        passenger: false,
        riding_vehicle: None,
        vehicle: None,
    }
}
/// The item that tempts the species of the run (`HORIZON_SPECIES`): its id and its name.
pub fn tempt_item() -> (u32, &'static str) {
    match species().name {
        "minecraft:pig" => (carrot(), "minecraft:carrot"),
        "minecraft:chicken" => (wheat_seeds(), "minecraft:wheat_seeds"),
        _ => (wheat(), "minecraft:wheat"),
    }
}
pub fn have_data() -> bool {
    // The tests inject their own configuration of the modules: every module is inscribed (the frame inscribes carried ones only).
    horizon_ecs_proto::species::inscribe_every_module();
    // A test without its data must fail, never pass empty.
    assert!(std::env::var_os("HORIZON_TEST_RESOURCES").is_some(), "données de test absentes : exporte HORIZON_TEST_RESOURCES");
    true
}
/// Whether the run's species (`HORIZON_SPECIES`, required) is none of `names`: the test is then skipped, and the skip is noted (printed and logged), never silent.
pub fn wrong_species(names: &[&str]) -> bool {
    let sp = horizon_ecs_proto::cow::species_var().expect("HORIZON_SPECIES absent : une espèce par passe, ex. HORIZON_SPECIES=minecraft:cow");
    let wrong = !names.contains(&sp.as_str());
    if wrong {
        let t = std::thread::current();
        horizon_ecs_proto::cow::skip_note(t.name().unwrap_or("?"), &format!("written for {names:?}, run is {sp}"));
    }
    wrong
}
/// Whether the run's species is one of `names`, whose own rule changes what the test asserts (the reason is noted with the skip, never silent).
pub fn unfit_species(names: &[&str], why: &str) -> bool {
    let sp = horizon_ecs_proto::cow::species_var().expect("HORIZON_SPECIES absent");
    let unfit = names.contains(&sp.as_str());
    if unfit {
        let t = std::thread::current();
        horizon_ecs_proto::cow::skip_note(t.name().unwrap_or("?"), &format!("{sp}: {why}"));
    }
    unfit
}
pub fn ecs_cow(h: &mut Herd, x: f64, y: f64, z: f64, seed: u64) -> Entity {
    let e = h.world.spawn(cow_bundle(x, z, seed)).id();
    horizon_ecs_proto::type_tags::mark(&mut h.world, e, species().id);
    horizon_ecs_proto::rules::equip_all(&mut h.world, e, species().rules);
    h.world.get_mut::<Pos>(e).unwrap().0.y = y;
    h.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(x, y, z), species().width, species().height);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
    // A cow at rest has the resting vertical velocity (a fresh one is in the air for its first tick).
    if y <= 1.0 {
        h.world.get_mut::<Vel>(e).unwrap().0.y = -0.08 * f64::from(0.98_f32);
    }
    e
}
/// A creature of the water lives in a pool (out of it, it flops in its own tick): a block of water around the cell where it stands.
pub fn pool_if_aquatic(h: &mut Herd, x: i32, z: i32) {
    if horizon_ecs_proto::type_tags::lists("can_breathe_under_water", species().id) {
        for dx in -2..=2 {
            for dz in -3..=3 {
                for y in 1..=2 {
                    h.set_block(x + dx, y, z + dz, "minecraft:water");
                }
            }
        }
    }
}
/// Where a creature stands at rest: put back after each tick, so one that flops, hops or hovers (fish out of water, slimes, flyers) is where
/// the test aimed, as the cow is.
pub struct Pin(Vec3, Aabb, Vec3);
pub fn pin(h: &Herd, e: Entity) -> Pin {
    Pin(h.world.get::<Pos>(e).unwrap().0, h.world.get::<Bounds>(e).unwrap().0, h.world.get::<Vel>(e).unwrap().0)
}
pub fn tick_pinned(h: &mut Herd, e: Entity, at: &Pin) {
    h.tick();
    if h.world.get_entity(e).is_ok() {
        h.world.get_mut::<Pos>(e).unwrap().0 = at.0;
        h.world.get_mut::<Bounds>(e).unwrap().0 = at.1;
        h.world.get_mut::<Vel>(e).unwrap().0 = at.2;
    }
}
pub fn health(h: &Herd, e: Entity) -> f32 {
    h.world.get::<Health>(e).map_or(0.0, |x| x.0)
}
pub fn mob_health(m: &Mob) -> f32 {
    if m.health <= 0.0 || m.removed { 0.0 } else { m.health }
}

/// A cow put in a cactus, in fire, in lava: its health at the ticks 40, 140 and 340, with the cow at rest (the
/// resting vertical velocity) and fresh from its spawn (velocity zero: it is in the air for its first tick).

impl MobRef {
    /// Sets a block of the `Mob` world (the leaked world of the level: nothing else reads it meanwhile).
    pub fn set_block(&self, x: i32, y: i32, z: i32, name: &str) {
        let Some(state) = self.w.registry.default_state(name) else { return };
        // SAFETY: the test thread owns the world while it steps; no reference to its blocks is alive.
        unsafe {
            let world = &mut *self.level.blocks.0;
            let _ = world.set(x, y, z, state);
            world.path_type_block_updated(horizon_core::position::BlockPos::new(x, y, z));
        }
    }
    /// A block with properties over its default state (a bed's head, an open door).
    pub fn set_block_props(&self, x: i32, y: i32, z: i32, name: &str, props: &[(&str, &str)]) {
        let Some(state) = horizon_ecs_proto::blocks::resolve(self.w, name, props) else { return };
        // SAFETY: as in `set_block`.
        unsafe {
            let world = &mut *self.level.blocks.0;
            let _ = world.set(x, y, z, state);
            world.path_type_block_updated(horizon_core::position::BlockPos::new(x, y, z));
        }
    }
    /// A property of the `Mob` world's block at (x, y, z) (`open` of a door, `type` of a chest), as the golems read it.
    pub fn prop(&self, x: i32, y: i32, z: i32, key: &str) -> Option<String> {
        horizon_ecs_proto::blocks::property(self.w, [x, y, z], key)
    }
    /// Applies to the `Mob` world the block changes the mobs asked for since the last call.
    pub fn apply_block_events(&mut self) {
        let fresh: Vec<(i32, i32, i32, &'static str)> = self.events[self.applied..]
            .iter()
            .filter_map(|e| match e {
                MobEvent::SetBlockName { pos, name, .. } => Some((pos[0], pos[1], pos[2], *name)),
                MobEvent::BreakBlock { pos, .. } => Some((pos[0], pos[1], pos[2], "minecraft:air")),
                _ => None,
            })
            .collect();
        // The doors the mobs opened or closed (`MobEvent::SetDoorOpen`, which the game's world applies): both halves take the `open` property.
        let doors: Vec<([i32; 3], bool)> = self.events[self.applied..]
            .iter()
            .filter_map(|e| match e {
                MobEvent::SetDoorOpen { pos, open } => Some((*pos, *open)),
                _ => None,
            })
            .collect();
        self.applied = self.events.len();
        for (x, y, z, n) in fresh {
            self.set_block(x, y, z, n);
        }
        for (p, open) in doors {
            for y in [p[1] - 1, p[1], p[1] + 1] {
                let name = self.w.block_name(p[0], y, p[2]).to_string();
                if !name.ends_with("_door") {
                    continue;
                }
                let Some(state) = self.w.registry.state(self.w.world.block(p[0], y, p[2])) else { continue };
                let mut props: Vec<(String, String)> = state.properties.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                for kv in props.iter_mut().filter(|kv| kv.0 == "open") {
                    kv.1 = open.to_string();
                }
                let refs: Vec<(&str, &str)> = props.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                self.set_block_props(p[0], y, p[2], &name, &refs);
            }
        }
    }
}

/// A chest in the block entities of a world (`MobRef::level`, or `*h.world.resource::<Level>()`) with these slots: the block entity the
/// golems read. The block itself is placed by `set_block`.
pub fn set_chest(level: Level, x: i32, y: i32, z: i32, slots: Vec<Option<horizon_client_lib::items::ItemStack>>) {
    // SAFETY: the test thread owns the world between two steps; no reference to its block entities is alive.
    unsafe {
        let world = &mut *level.blocks.0;
        world.block_entities.insert((x, y, z), horizon_client_render::chunk::block_entity::BlockEntity::Chest(slots));
    }
}
pub fn flat_pair() -> (MobRef, Herd) {
    (MobRef::new(20, false), Herd::new_awake(0, 6.0, true, false, 3))
}
pub fn xz(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}
pub fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

impl MobRef {
    /// Grows the crops a `Mob` villager worked on since `seen` (`MobEvent::Bonemeal`: the game's `BoneMealItem.growCrop`, 3 ages
    /// here, capped at 7), tells the villager (`bonemeal_grown`), and returns the ticks of the growths.
    pub fn bonemeal_growths(&mut self, seen: &mut usize) -> Vec<i64> {
        let fresh: Vec<(u32, [i32; 3])> = self.events[*seen..]
            .iter()
            .filter_map(|e| match e {
                MobEvent::Bonemeal { mob, pos } => Some((*mob, *pos)),
                _ => None,
            })
            .collect();
        *seen = self.events.len();
        let mut ticks = Vec::new();
        for (id, p) in fresh {
            let name = self.w.block_name(p[0], p[1], p[2]).to_string();
            let age = self.prop(p[0], p[1], p[2], "age").and_then(|a| a.parse::<i32>().ok()).unwrap_or(0);
            let next = (age + 3).min(7).to_string();
            self.set_block_props(p[0], p[1], p[2], &name, &[("age", next.as_str())]);
            if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                horizon_client_lib::mobs::villager_brain::bonemeal_grown(m, self.t);
            }
            ticks.push(self.t);
        }
        ticks
    }
}
/// Whether the warden's brain still holds an attack target (`Warden::target`).
pub fn warden_has_target(m: &Mob) -> bool {
    m.extra.warden.as_deref().is_some_and(|w| w.target().is_some())
}
/// The yaw (degrees, vanilla `lookAt`) a head at `from` takes to look at `to`, in the x/z plane.
pub fn look_yaw(from: Vec3, to: Vec3) -> f32 {
    ((to.z - from.z).atan2(to.x - from.x).to_degrees() - 90.0) as f32
}

/// The species under test walks on the ground with its own locomotion (a creature standing in water, on honey or in a bubble column):
/// objects, projectiles and flyers do not take these scenes (they have no ground mode, or no gravity to bring them to rest).
pub fn ground_walker() -> bool {
    let s = species();
    s.gravity > 0.0 && s.modes.iter().any(|(m, _)| *m == horizon_ecs_proto::locomotion::Medium::Ground)
}
