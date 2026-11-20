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
    pub fn names(&self, i: usize) -> Vec<&'static str> {
        self.mobs[i].ai.goals.running_names()
    }
    /// A blow of a player standing at `from`: damage and knockback away from it (`Herd::hit` on the ecs side).
    pub fn hit(&mut self, i: usize, amount: f32, from: [f64; 3]) {
        let mut lvl = self.tl.level();
        self.mobs[i].damage(&mut lvl, amount, Damage::player(from));
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
pub fn ecs_cow(h: &mut Herd, x: f64, y: f64, z: f64, seed: u64) -> Entity {
    let e = h.world.spawn(cow_bundle(x, z, seed)).id();
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

pub fn flat_pair() -> (MobRef, Herd) {
    (MobRef::new(20, false), Herd::new(0, 6.0, true, false, 3))
}
pub fn xz(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}
pub fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}
