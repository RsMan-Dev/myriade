//! The creaking heart's side of the bridge: a creaking linked to its heart (`HeartLink`, the jar's `home_pos`), spawned, found by its uuid,
//! told to tear down or twitch to death, and asked whether a player is stuck in it. The heart block itself lives in the host; the
//! creaking is an ordinary entity of the species `minecraft:creaking`.
use crate::bridge::{host::EcsWorld, uuids};
use crate::damage_link::DamageLink;
use crate::tear_down::Crumbling;
use crate::brain::{self, MemValue, NEAREST_PLAYERS};
use crate::cow::{Bounds, Health, Pos};
use crate::despawn::Persistent;
use crate::goals::Online;
use crate::hostile::PlayerState;
use crate::targeting::{self, FollowRange};
use bevy_ecs::{entity::Entity, prelude::*};
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};

/// `Creaking.homePos`: the heart this creaking protects, and the state its heart reads from it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet", on_insert = linked, on_discard = unlinked)]
pub struct HeartLink {
    pub pos: [i32; 3],
    /// `playerStuckCounter`.
    pub stuck: i32,
    /// `Creaking.setTearingDown`: the heart was broken by someone.
    pub tearing_down: bool,
}

/// The creakings linked to each heart position: a block change there asks them again (`ask_heart`). Kept by the hooks of `HeartLink`.
#[derive(Resource, Default)]
pub struct HeartWatch(pub std::collections::HashMap<[i32; 3], Vec<Entity>>);

fn linked(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(pos) = world.get::<HeartLink>(ctx.entity).map(|l| l.pos) else { return };
    let e = ctx.entity;
    world.commands().queue(move |w: &mut World| w.get_resource_or_insert_with(HeartWatch::default).0.entry(pos).or_default().push(e));
}

fn unlinked(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(pos) = world.get::<HeartLink>(ctx.entity).map(|l| l.pos) else { return };
    let e = ctx.entity;
    world.commands().queue(move |w: &mut World| {
        let mut watch = w.get_resource_or_insert_with(HeartWatch::default);
        if let Some(v) = watch.0.get_mut(&pos) {
            v.retain(|x| *x != e);
            if v.is_empty() {
                watch.0.remove(&pos);
            }
        }
    });
}

/// Asks the host whether the heart of `e` still claims it (`MiscEvent::HeartAsked`). Nothing for an entity without a link, one that is
/// crumbling (already dying) or asleep (it is asked when it wakes).
pub fn ask_heart(w: &mut World, e: Entity) {
    let Some(pos) = w.get::<HeartLink>(e).map(|l| l.pos) else { return };
    if w.get::<Crumbling>(e).is_some() || w.get::<crate::cow::Dying>(e).is_some() || w.get::<crate::zone::Dormant>(e).is_some() {
        return;
    }
    w.get_resource_or_insert_with(crate::colony_register::MiscOut::default).0.push(crate::host_out_misc::MiscEvent::HeartAsked { entity: e, pos });
}

/// The malus the jar's `Creaking.setTransient` writes (`PathType` names as the block data spells them).
const MALUS: [(&str, f32); 5] = [("DAMAGING", 8.0), ("POWDER_SNOW", 8.0), ("LAVA", 8.0), ("FIRE", 0.0), ("FIRE_IN_NEIGHBOR", 0.0)];

/// `Creaking.CREAKING` species name.
const KIND: &str = crate::species::lists::CREAKING;
/// `SoundEvents.CREAKING_DEATH`.
const DEATH_SOUND: &str = "entity.creaking.death";

impl EcsWorld {
    /// `SpawnUtil.trySpawnMob` then `Creaking.setTransient(heart)`: a creaking at `at` turned to `yaw`, its follow range raised by the
    /// `bonus` the spawn drew (`finalizeSpawn`), linked to the heart at `heart`, with a random uuid so a save names it. None when the
    /// species is unknown.
    pub fn spawn_creaking(&mut self, mw: &MobWorld<'_>, at: [f64; 3], yaw: f32, bonus: f64, heart: [i32; 3], uuid: [i32; 4]) -> Option<Entity> {
        let e = self.spawn(mw, KIND, at, None)?;
        self.set_rotation(e, yaw, 0.0);
        let w = self.world_mut();
        if let Some(mut f) = w.get_mut::<FollowRange>(e) {
            f.0 += f.0 * bonus;
        }
        w.entity_mut(e).insert((HeartLink { pos: heart, stuck: 0, tearing_down: false }, DamageLink::default()));
        uuids::set_uuid(w, e, uuid);
        for (kind, value) in MALUS {
            self.set_nav_malus(e, kind, value);
        }
        Some(e)
    }

    /// The heart a creaking is linked to; None for any other entity.
    pub fn heart_link(&self, e: Entity) -> Option<HeartLink> {
        self.world().get::<HeartLink>(e).copied()
    }

    /// A creaking the heart still sees: alive, linked and not crumbling. A sleeping one (`Disabled`) counts: the heart protects it too.
    pub fn heart_present(&self, e: Entity) -> bool {
        let Ok(r) = self.world().get_entity(e) else { return false };
        r.contains::<HeartLink>() && !r.contains::<Crumbling>() && r.get::<Health>().is_some_and(|h| h.0 > 0.0)
    }

    /// The box of a linked creaking, asleep or not (`getBoundingBox` of the heart's creaking).
    pub fn heart_bounds(&self, e: Entity) -> Option<Aabb> {
        let r = self.world().get_entity(e).ok()?;
        r.contains::<HeartLink>().then(|| r.get::<Bounds>().map(|b| b.0))?
    }

    /// `Mob.isPersistenceRequired` of the creaking.
    pub fn is_persistent(&self, e: Entity) -> bool {
        self.world().get::<Persistent>(e).is_some()
    }

    /// The feet of the creaking.
    pub fn feet(&self, e: Entity) -> Option<Vec3> {
        self.world().get::<Pos>(e).map(|p| p.0)
    }

    /// `Creaking.setTearingDown`.
    pub fn set_tearing_down(&mut self, e: Entity) -> bool {
        self.world_mut().get_mut::<HeartLink>(e).map(|mut l| l.tearing_down = true).is_some()
    }

    /// `setHealth(0)` of a creaking: it twitches to death, the 46 ticks of `tear_down`, and goes with no loot (`MiscEvent::Crumbled`).
    pub fn kill_linked(&mut self, e: Entity) {
        crate::tear_down::start(self.world_mut(), e, DEATH_SOUND);
    }

    /// `Creaking.tick` with no heart of its own: `setHealth(0)`, then `Monster.tickDeath` (not the crumbling, which needs
    /// `isTearingDown`): 20 ticks, the smoke, no `die`: no loot, no experience, nothing said to mods (`tear_down::fade`).
    pub fn kill_orphan(&mut self, _mw: &MobWorld<'_>, e: Entity) {
        if self.heart_present(e) {
            self.fade_death(e);
        }
    }

    /// `setHealth(0)` with no damage source: the generic path, shared by every caller.
    pub fn fade_death(&mut self, e: Entity) {
        crate::tear_down::fade(self.world_mut(), e);
    }

    /// The creaking's voice (`Mob.voice`).
    pub fn creaking_voice(&mut self, e: Entity, name: &'static str) {
        crate::tear_down::voice(self.world_mut(), e, name);
    }

    /// `Creaking.tearDown`: the voice, then the creaking is gone. Gives the centre and the spread of the crumbling box.
    pub fn tear_down_creaking(&mut self, e: Entity) -> Option<([f64; 3], [f64; 3])> {
        let b = self.world().get::<Bounds>(e)?.0;
        let (lo, hi) = (b.min_position(), b.max_position());
        self.creaking_voice(e, DEATH_SOUND);
        self.despawn(e);
        Some(([(lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0], [(hi.x - lo.x) * 0.3, (hi.y - lo.y) * 0.3, (hi.z - lo.z) * 0.3]))
    }

    /// `Creaking.playerIsStuckInYou`: one of its nearest players has its eyes inside its box; the counter must pass 4.
    pub fn creaking_player_stuck(&mut self, e: Entity) -> bool {
        let w = self.world_mut();
        let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return false };
        let eyes: Vec<Vec3> = match brain::get(w, e, &NEAREST_PLAYERS) {
            Some(MemValue::Whos(l)) => l.iter().filter_map(|x| targeting::info(w, *x).map(|i| Vec3::new(i.pos.x, i.eye, i.pos.z))).collect(),
            _ => Vec::new(),
        };
        let inside = eyes.iter().any(|p| contains(&b, *p));
        let Some(mut l) = w.get_mut::<HeartLink>(e) else { return false };
        l.stuck = if inside { l.stuck + 1 } else { 0 };
        l.stuck > 4
    }

    /// `Level.getNearestPlayer(at, range, false)`: the closest non-spectator player within `range`.
    pub fn nearest_player_within(&self, at: [f64; 3], range: f64) -> Option<u32> {
        let w = self.world();
        let ps = w.resource::<PlayerState>();
        let mut best: Option<(f64, u32)> = None;
        w.resource::<Online>().for_near(at[0], at[2], range, |p| {
            let d = (p.x - at[0]).powi(2) + (p.y - at[1]).powi(2) + (p.z - at[2]).powi(2);
            if d < range * range && !ps.spectator(p.id) && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, p.id));
            }
        });
        best.map(|(_, id)| id)
    }
}

/// The creaking loses its heart (the key `home_pos` went): the link and the damage guard it brought go with it. A crumbling one keeps its
/// guard (it takes nothing while it dies).
pub fn unlink(w: &mut World, e: Entity) {
    let Ok(mut m) = w.get_entity_mut(e) else { return };
    m.remove::<HeartLink>();
    if !m.contains::<Crumbling>() {
        m.remove::<DamageLink>();
    }
}

/// `AABB.contains(Vec3)`: the far faces are outside.
fn contains(b: &Aabb, p: Vec3) -> bool {
    let (lo, hi) = (b.min_position(), b.max_position());
    p.x >= lo.x && p.x < hi.x && p.y >= lo.y && p.y < hi.y && p.z >= lo.z && p.z < hi.z
}
