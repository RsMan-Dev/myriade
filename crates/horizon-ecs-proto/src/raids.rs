//! The raids socle: `Raids` (the `Mob` side's own pure state machine, `horizon_client_lib::raid`: waves, the countdown, the bar, the leader per wave,
//! heroes) driven over the ECS world. A raid is a deadline of the wheel (`Due::Raids`, one entry per tick while some raid runs, none when
//! none does): `raid_due` ticks the raids with this world as their `RaidLevel` (the registry of the points of interest says what is a village,
//! the players are `Online`) and does what the raids ask: a wave spawns (`Action::Spawn`: each raider of the species table at the spot, with the
//! `Raider` component: raid, wave, leader), a raider leaves the raid. The goals read `Raider` and the raid's centre (`PathfindToRaid`). Species that
//! the table lacks (evoker, ravager) are not spawned. Not done: the omen's trigger, heroes' reward, the bar for players, the horn, `ticks_outside`,
//! the raiders' buffs (enchantments, banners' head item), the ravager rider, recruitment of the raiders around a raider, saving.
use crate::cow::*;
use crate::goals::{Entered, Online, Req, Requests};
use crate::poi::PoiRegistry;
use bevy_ecs::prelude::*;
use horizon_client_lib::raid::{self, Action, RaidLevel, RaidPlayer, RaiderInfo, Raids};
use std::collections::HashMap;

/// A raider: which raid it is in, the wave it came with, whether it leads the wave.
#[derive(Component, Clone, Copy, Debug)]
pub struct Raider {
    pub raid: Option<i32>,
    pub wave: i32,
    pub leader: bool,
}
#[derive(Resource)]
pub struct RaidState {
    pub raids: Raids,
    pub difficulty: i32,
    host: Entity,
    /// The ids the raids know the raiders by (the entity's index) and the entities.
    by_id: HashMap<u32, Entity>,
    armed: bool,
}
/// Created at the first raid (a resource is made at its first use, never at the world's construction).
fn ensure(w: &mut World) {
    if !w.contains_resource::<RaidState>() {
        let host = w.spawn_empty().id();
        w.insert_resource(RaidState { raids: Raids::default(), difficulty: 2, host, by_id: HashMap::new(), armed: false });
    }
}
fn id_of(e: Entity) -> u32 {
    e.index().index()
}

/// The world as a raid sees it.
struct Level<'a> {
    w: &'a World,
    difficulty: i32,
}
impl RaidLevel for Level<'_> {
    fn difficulty(&self) -> i32 {
        self.difficulty
    }
    fn has_chunk_at(&self, pos: [i32; 3]) -> bool {
        self.w.resource::<crate::cow::Level>().mw.0.world.loaded(pos[0], pos[2])
    }
    fn has_chunks_at(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        let mw = self.w.resource::<crate::cow::Level>().mw.0;
        mw.world.loaded(x0, z0) && mw.world.loaded(x1, z1)
    }
    fn sections_to_village(&self, pos: [i32; 3]) -> i32 {
        self.w.resource::<PoiRegistry>().sections_to_village([pos[0] >> 4, pos[1] >> 4, pos[2] >> 4])
    }
    fn surface_height(&self, x: i32, z: i32) -> i32 {
        // `Heightmap.Types.WORLD_SURFACE`: the first block from the top that is not air, plus one.
        let mw = self.w.resource::<crate::cow::Level>().mw.0;
        let mut y = mw.world.top_y() - 1;
        while y > mw.world.bottom_y() && mw.facts.is_air(mw.world.block(x, y, z)) {
            y -= 1;
        }
        y + 1
    }
    fn is_position_entity_ticking(&self, _: [i32; 3]) -> bool {
        true
    }
    fn ravager_can_spawn(&self, _: [i32; 3]) -> bool {
        true
    }
    fn players(&self) -> Vec<RaidPlayer> {
        let on = self.w.resource::<Online>();
        let mut v = Vec::new();
        on.for_near(0.0, 0.0, 1.0e9, |p| v.push(RaidPlayer { uuid: u128::from(p.id), position: [p.x, p.y, p.z], block: [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32], alive: true }));
        v
    }
}

/// A raid at `center` (`Raids.createOrExtendRaid`), and its first tick is armed.
pub fn start(w: &mut World, center: [i32; 3], seed: i64) -> i32 {
    ensure(w);
    let mut st = w.resource_mut::<RaidState>();
    let d = st.difficulty;
    let (id, _) = st.raids.create_or_extend(center, d, None, seed);
    arm(w);
    id
}
fn arm(w: &mut World) {
    let mut st = w.resource_mut::<RaidState>();
    if st.armed {
        return;
    }
    st.armed = true;
    let host = st.host;
    let now = w.resource::<Clock>().0;
    w.resource_mut::<Wheel>().push(now + 1, host, Due::Raids);
}
/// The raiders of a raid, as the raid reads them.
fn members(w: &mut World, id: i32) -> Vec<RaiderInfo> {
    let mut q = w.query::<(Entity, &Raider, &Health, &Pos)>();
    q.iter(w)
        .filter(|(_, r, ..)| r.raid == Some(id))
        .map(|(e, r, h, p)| RaiderInfo { id: id_of(e), wave: r.wave, health: h.0, position: [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32], in_level: true, ..Default::default() })
        .collect()
}
/// The deadline: the raids tick, what they ask is done, and the next tick is armed while some raid runs.
pub fn raid_due(w: &mut World) {
    let now = w.resource::<Clock>().0;
    let _ = now;
    w.resource_mut::<RaidState>().armed = false;
    let ids: Vec<i32> = w.resource::<RaidState>().raids.ids();
    let before: Vec<(i32, (bool, bool, bool))> = ids.iter().filter_map(|&i| Some((i, phase(w, i)?))).collect();
    let mut all: HashMap<i32, Vec<RaiderInfo>> = HashMap::new();
    for id in ids {
        all.insert(id, members(w, id));
    }
    let mut actions: Vec<(i32, Action)> = Vec::new();
    w.resource_scope(|w: &mut World, mut st: Mut<RaidState>| {
        let d = st.difficulty;
        let level = Level { w, difficulty: d };
        let st = &mut *st;
        actions = st.raids.tick(&level, true, &|id| all.get(&id).cloned().unwrap_or_default());
    });
    for (id, a) in actions {
        match a {
            Action::Spawn { pos, wave, spawns } => spawn_wave(w, id, pos, wave, &spawns),
            Action::Remove { id: mob } => {
                if let Some(e) = w.resource::<RaidState>().by_id.get(&mob).copied()
                    && let Some(mut r) = w.get_mut::<Raider>(e)
                {
                    r.raid = None;
                }
            }
            _ => {}
        }
    }
    // The goals that read the raid's phase are asked once when it changes (a raid starts, is won or lost), never every tick.
    for (id, was) in before {
        if phase(w, id) != Some(was) {
            wake_raiders(w, id);
        }
    }
    if !w.resource::<RaidState>().raids.map.is_empty() {
        arm(w);
    }
}
/// What the goals read of a raid: active, over, lost.
fn phase(w: &World, id: i32) -> Option<(bool, bool, bool)> {
    let r = w.resource::<RaidState>().raids.get(id)?;
    Some((r.active, r.is_over(), r.is_loss()))
}
/// Every raider of the raid asks its raid goals (`Req::Start`: the goal asks its own `can_use`).
pub fn wake_raiders(w: &mut World, id: i32) {
    use crate::behaviours as b;
    let mut q = w.query::<(Entity, &Raider)>();
    let ids: Vec<Entity> = q.iter(w).filter(|(_, r)| r.raid == Some(id)).map(|(e, _)| e).collect();
    for e in ids {
        for m in [&b::pathfind_to_raid::MODULE, &b::celebration::MODULE, &b::obtain_banner::MODULE, &b::raid_move_through_village::MODULE, &b::raider_open_door::MODULE, &b::break_door::MODULE] {
            if let Some(slot) = crate::goals::slot_of(w, e, m) {
                w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
            }
        }
    }
}
/// The raid of the raider, when it has one the game still knows.
pub fn raid_of(w: &World, e: Entity) -> Option<&raid::Raid> {
    let id = w.get::<Raider>(e)?.raid?;
    w.get_resource::<RaidState>()?.raids.get(id)
}
/// `Raider.hasActiveRaid`: the raider's raid is active and not over.
pub fn has_active_raid(w: &World, e: Entity) -> bool {
    raid_of(w, e).is_some_and(|r| r.active && !r.is_over())
}
/// The level's difficulty (0 peaceful .. 3 hard): the raids' own, 2 when no raid ever ran.
pub fn difficulty(w: &World) -> i32 {
    w.get_resource::<RaidState>().map_or(2, |s| s.difficulty)
}
/// `PatrollingMonster`: the patrol of a raider that is not in a raid (a leader, where it walks to, until when a failed hop keeps it still).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Patrol {
    pub leader: bool,
    pub patrolling: bool,
    pub target: Option<[i32; 3]>,
    pub cooldown_until: u64,
}
/// A raider shouted at by its patrol, or one that attacks for good (`Raider.hasAggressiveTarget`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Aggressive;
/// A raider that celebrates (`Raider.setCelebrating`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Celebrating;
fn spawn_wave(w: &mut World, raid_id: i32, pos: [i32; 3], wave: i32, spawns: &[raid::Spawn]) {
    let table = w.resource::<SpeciesTable>().0.clone();
    for sp in spawns {
        let Some(species) = table.iter().find(|s| s.name == sp.kind.entity()).copied() else { continue };
        let at = (f64::from(pos[0]) + 0.5, f64::from(pos[2]) + 0.5);
        let seed = u64::from(pos[0] as u32) << 32 ^ u64::from(pos[2] as u32) ^ (wave as u64) << 8 ^ 0x9E37_79B9_7F4A_7C15;
        let e = spawn_species(w, &species, at.0, at.1, seed);
        teleport(w, e, horizon_core::geometry::Vec3::new(at.0, f64::from(pos[1]) + 1.0, at.1));
        w.entity_mut(e).insert(Raider { raid: Some(raid_id), wave, leader: sp.leader });
        crate::behaviours::pathfind_to_raid::wake(w, e);
        crate::goals::arm_module(w, e, &crate::behaviours::obtain_banner::MODULE, 40);
        w.resource_mut::<Entered>().0.push(e);
        let health = w.get::<Health>(e).map_or(24.0, |h| h.0);
        let living: f32 = members(w, raid_id).iter().map(|m| m.health).sum();
        let mut st = w.resource_mut::<RaidState>();
        st.by_id.insert(id_of(e), e);
        if let Some(r) = st.raids.get_mut(raid_id) {
            if sp.leader {
                r.set_leader(wave, id_of(e));
            }
            r.add_wave_mob(health, true, living);
        }
    }
}
/// `Raider.getCurrentRaid`'s centre, when the raid is active and not over: where `PathfindToRaid` walks to.
pub fn raid_centre(w: &World, e: Entity) -> Option<[i32; 3]> {
    let id = w.get::<Raider>(e)?.raid?;
    let r = w.get_resource::<RaidState>()?.raids.get(id)?;
    (r.active && !r.is_over()).then_some(r.center)
}
