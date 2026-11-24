//! What `EntityState` reads of an entity, from its components, on demand (no cache). The entities whose view changed are queued by
//! the act that changes them (`mark`) and by the `NetId` hook; a sleeping entity (`Disabled`) is never queued, and nothing scans
//! the entities.
use crate::cow::{Contact, Health, Pos, SpeciesId, TypeName, Vel, Yaw};
use crate::goals::{Equipment, Head, Uid};
use crate::leash::{Holder, Leash};
use crate::mount::Mount;
use crate::targeting::Who;
use bevy_ecs::{entity::Entity, entity_disabling::Disabled, prelude::*};

/// What the tracker reads of one entity. `head`: the head's yaw and pitch. `uid`: the `Uid`, a counter of the world, not a UUID.
#[derive(Clone, Debug)]
pub struct EntityView {
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub yaw: f32,
    pub head: Option<(f32, f32)>,
    /// `Mob.on_ground` (`Contact.on_ground`), `Mob.horizontal_collision` (`Contact.horizontal`).
    pub on_ground: bool,
    pub horizontal_collision: bool,
    /// The species' name, as `TypeName` has it.
    pub kind: &'static str,
    pub uid: u64,
    pub equipment: Option<Equipment>,
    pub rider: Option<Who>,
    pub passenger: Option<Who>,
    pub leash: Option<Holder>,
    /// A creature: a species and its health. A vehicle: a boat or a cart (`minecart::Kind`). Read from the components, no scan.
    pub is_creature: bool,
    pub is_vehicle: bool,
    /// A fishing hook: what its add packet and metadata carry (the thrower, the hooked target, the bite).
    pub hook: Option<HookView>,
}

/// `FishingHook`: the thrower's network id (`data` of the add packet), DATA_HOOKED_ENTITY and DATA_BITING.
#[derive(Clone, Copy, Debug)]
pub struct HookView {
    pub owner: u32,
    pub hooked: Option<crate::fishing_hook::Target>,
    pub biting: bool,
}

/// The view of `e`, or None when it has no position or sleeps (`Disabled`: not ticked, not tracked).
pub fn view(world: &World, e: Entity) -> Option<EntityView> {
    let r = world.get_entity(e).ok()?;
    if r.contains::<Disabled>() {
        return None;
    }
    let p = r.get::<Pos>()?.0;
    // A display, a marker or an interaction has no movement: it is at rest.
    let v = r.get::<Vel>().map_or(horizon_core::geometry::Vec3::ZERO, |v| v.0);
    let mount = r.get::<Mount>();
    Some(EntityView {
        position: [p.x, p.y, p.z],
        velocity: [v.x, v.y, v.z],
        yaw: r.get::<Yaw>().map_or(0.0, |y| y.0),
        head: r.get::<Head>().map(|h| (h.yaw, h.pitch)),
        on_ground: r.get::<Contact>().is_some_and(|c| c.on_ground),
        horizontal_collision: r.get::<Contact>().is_some_and(|c| c.horizontal),
        kind: r.get::<TypeName>().map_or("", |t| t.0),
        uid: r.get::<Uid>().map_or(0, |u| u.0),
        equipment: r.get::<Equipment>().copied(),
        rider: mount.and_then(|m| m.rider),
        passenger: mount.and_then(|m| m.passenger),
        leash: r.get::<Leash>().map(|l| l.holder),
        hook: r.get::<crate::fishing_hook::Hook>().map(|h| HookView {
            owner: h.owner,
            hooked: match h.phase {
                crate::fishing_hook::Phase::Hooked(t) => Some(t),
                _ => None,
            },
            biting: h.nibble > 0,
        }),
        is_creature: r.contains::<SpeciesId>() && r.contains::<Health>(),
        is_vehicle: r.contains::<crate::boat::Boat>() || r.contains::<crate::minecart::Kind>(),
    })
}

/// The position of `e` alone (a live entity), for the events that need where it stands.
pub fn position(world: &World, e: Entity) -> Option<[f64; 3]> {
    let r = world.get_entity(e).ok()?;
    if r.contains::<Disabled>() {
        return None;
    }
    let p = r.get::<Pos>()?.0;
    Some([p.x, p.y, p.z])
}

/// The entities whose view changed since the last `drain`: `.0` those whose synchronized data may have changed (the default: read all of
/// it again), `.1` those that only moved or turned (`reindex`, `look_control`: position and rotation, no metadata).
#[derive(Resource, Default)]
pub struct Modified(Vec<Entity>, Vec<Entity>);

/// The entities that entered the world (a `Pos` was added) since the start of the last step: the host looks once at the block each stands
/// in (an entity in a portal when it spawns or loads is taken by it). Two generations: at the start of a step the new ones become the old
/// and the old go, so each is read by the passes around two steps (`portal_entries`).
#[derive(Resource, Default)]
pub struct Entered(pub Vec<Entity>, pub Vec<Entity>);

impl Entered {
    pub fn rotate(&mut self) {
        self.1 = std::mem::take(&mut self.0);
    }
}

impl Modified {
    pub fn push(&mut self, e: Entity) {
        self.0.push(e);
    }
    /// `e` only moved or turned: its position and rotation are read again, its metadata is not.
    pub fn push_motion(&mut self, e: Entity) {
        self.1.push(e);
    }
}

/// Queues `e`: its view changed. A sleeping entity is not queued (it is not ticked, so nothing moves in it).
pub fn mark(world: &mut World, e: Entity) {
    if world.get_entity(e).is_ok_and(|r| !r.contains::<Disabled>()) {
        world.init_resource::<Modified>();
        world.resource_mut::<Modified>().push(e);
    }
}

/// `mark` for a change of position or rotation only: nothing synchronized was written (vanilla `SynchedEntityData.set` is dirty only when the
/// value changes; the position goes by its own codec).
pub fn mark_motion(world: &mut World, e: Entity) {
    if world.get_entity(e).is_ok_and(|r| !r.contains::<Disabled>()) {
        world.init_resource::<Modified>();
        world.resource_mut::<Modified>().push_motion(e);
    }
}

/// The queued entities, each once, by index, with whether their synchronized data may have changed (queued by `mark`); empty when nothing
/// changed. A queued entity may have been despawned since: `view` says.
pub fn drain(world: &mut World) -> Vec<(Entity, bool)> {
    let Some((mut full, mut motion)) = world.get_resource_mut::<Modified>().map(|mut m| (std::mem::take(&mut m.0), std::mem::take(&mut m.1))) else {
        return Vec::new();
    };
    full.sort_unstable();
    full.dedup();
    if motion.is_empty() {
        return full.into_iter().map(|e| (e, true)).collect();
    }
    motion.sort_unstable();
    motion.dedup();
    let mut out: Vec<(Entity, bool)> = Vec::with_capacity(full.len() + motion.len());
    out.extend(full.iter().map(|&e| (e, true)));
    out.extend(motion.into_iter().filter(|e| full.binary_search(e).is_err()).map(|e| (e, false)));
    out.sort_unstable_by_key(|x| x.0);
    out
}
