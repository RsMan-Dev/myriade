//! The save and the resolution of a lead (`Leash`, `Leashable`). The holder is written as the uuid of an entity (`EntityUuid`) or as a
//! knot's coordinates, under the `leash` key of the creature's NBT. Read back, the lead waits as `PendingLeash` (a sparse component laid
//! by the load only) and `resolve_pending` ties it when its holder is there: only the pending leads are visited. A uuid holder still
//! absent after 101 ticks of the creature's clock is dropped as a lead (`Leashable.restoreLeashFromSave`: `tickCount > 100`).
use crate::bridge::{host::EcsWorld, uuids, view};
use crate::cow::{after, Bounds, Pos};
use crate::goals::Online;
use crate::item::drop_item;
use crate::leash::{Holder, Leash, Leashable, attach_from_save};
use crate::player_uuids;
use crate::rules::leash::center_of;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::{geometry::Vec3, nbt::Tag};

/// A lead read from a save, not yet tied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Knot([i32; 3]),
    Uuid([i32; 4]),
}

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct PendingLeash(pub Pending);

/// Writes the `leash` key of `e`'s lead into `tag`. Nothing when `e` is not held, when its holder has no uuid, or when a player holds it.
pub fn save(w: &World, e: Entity, tag: &mut Tag) {
    let Some(l) = w.get::<Leash>(e) else { return };
    let value = match l.holder {
        Holder::Knot(p) => Tag::IntArray(p.to_vec()),
        Holder::Entity(h) => {
            let Some(u) = w.get::<uuids::EntityUuid>(h) else { return };
            Tag::compound([("UUID", Tag::IntArray(uuids::to_words(u.0).to_vec()))])
        }
        Holder::Player(id) => {
            let Some(u) = player_uuids::uuid_of(w, id) else { return };
            Tag::compound([("UUID", Tag::IntArray(uuids::to_words(u).to_vec()))])
        }
    };
    if let Tag::Compound(map) = tag {
        map.insert("leash".into(), value);
    }
}

/// Reads the `leash` key of a saved `tag` as a pending lead on `e`. A tag with no readable holder leaves `e` alone.
pub fn load(w: &mut World, e: Entity, tag: &Tag) {
    let p = match tag.get("leash") {
        Some(Tag::IntArray(v)) if v.len() == 3 => Pending::Knot([v[0], v[1], v[2]]),
        Some(t) => match t.get("UUID") {
            Some(Tag::IntArray(v)) if v.len() == 4 => Pending::Uuid([v[0], v[1], v[2], v[3]]),
            _ => return,
        },
        None => return,
    };
    w.entity_mut(e).insert(PendingLeash(p));
    if let Pending::Uuid(_) = p {
        after(w, e, LEASH_DROP_TICKS, drop_pending);
    }
}

/// The deadline of a lead whose holder never came (`tickCount > 100`): the lead is dropped as an item, the holder is not waited for anymore.
const LEASH_DROP_TICKS: i32 = 101;
/// `Entity.spawnAtLocation`: the pickup delay of a dropped item.
const DROP_DELAY: i64 = 10;

fn drop_pending(w: &mut World, e: Entity) {
    if w.get::<PendingLeash>(e).is_none() {
        return;
    }
    w.entity_mut(e).remove::<PendingLeash>();
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return };
    drop_item(w, ItemStack::new("minecraft:lead", 1), at, Vec3::ZERO, DROP_DELAY);
}

/// Ties each pending lead whose holder is there (a knot, or the entity of its uuid); the others wait for the next call.
pub fn resolve_pending(w: &mut World) {
    let waiting: Vec<(Entity, Pending)> = w.query::<(Entity, &PendingLeash)>().iter(w).map(|(e, p)| (e, p.0)).collect();
    for (e, p) in waiting {
        let holder = match p {
            Pending::Knot(k) => Holder::Knot(k),
            Pending::Uuid(u) => match uuids::entity_by_uuid(w, u) {
                Some(h) => Holder::Entity(h),
                None => match player_uuids::id_of(w, uuids::from_words(u)) {
                    Some(id) if w.get_resource::<Online>().is_some_and(|o| o.get(id).is_some()) => Holder::Player(id),
                    _ => continue,
                },
            },
        };
        w.entity_mut(e).remove::<PendingLeash>();
        attach_from_save(w, e, holder);
    }
}

/// `Leashable.canHaveALeashAttachedTo`: the holder's centre `center` is within the snapping distance of `e`'s centre.
pub fn can_attach(w: &World, e: Entity, center: [f64; 3]) -> bool {
    let (Some(l), Some(b)) = (w.get::<Leashable>(e), w.get::<Bounds>(e)) else { return false };
    center_of(b.0).distance(Vec3::new(center[0], center[1], center[2])) < l.snap
}

/// `LeashFenceKnotEntity.getOrCreate`: the knot of the fence at `fence`, made at its centre when there is none.
pub fn tie_knot(w: &mut World, fence: [i32; 3]) -> Entity {
    crate::species::leash_knot::tie(w, fence)
}

/// The fence cell of `e`, when `e` is a knot.
pub fn knot_fence(w: &World, e: Entity) -> Option<[i32; 3]> {
    w.get::<crate::species::leash_knot::Knot>(e).map(|k| k.fence)
}

/// `e` accepts a lead and holds none.
pub fn can_be_leashed(w: &World, e: Entity) -> bool {
    w.get::<Leashable>(e).is_some() && w.get::<Leash>(e).is_none()
}

impl EcsWorld {
    /// Writes the `leash` key of `e` into `tag` (see `save`).
    pub fn leash_save(&self, e: Entity, tag: &mut Tag) {
        save(self.world(), e, tag);
    }

    /// Reads the `leash` key of `tag` as a pending lead on `e` (see `load`).
    pub fn leash_load(&mut self, e: Entity, tag: &Tag) {
        load(self.world_mut(), e, tag);
    }

    /// Ties the pending leads whose holder has appeared (see `resolve_pending`).
    pub fn leash_resolve_pending(&mut self) {
        resolve_pending(self.world_mut());
    }

    pub fn leash_can_attach(&self, e: Entity, center: [f64; 3]) -> bool {
        can_attach(self.world(), e, center)
    }

    pub fn leash_can_be_leashed(&self, e: Entity) -> bool {
        can_be_leashed(self.world(), e)
    }

    /// The knot of the fence `fence` (see `tie_knot`).
    pub fn leash_tie_knot(&mut self, fence: [i32; 3]) -> Entity {
        tie_knot(self.world_mut(), fence)
    }

    /// A leashee left the knot of `fence` (see `leash_knot::released`).
    pub fn leash_knot_released(&mut self, fence: [i32; 3]) {
        crate::species::leash_knot::released(self.world_mut(), fence);
    }

    /// The fence cell of the knot `e` (see `knot_fence`).
    pub fn leash_knot_fence(&self, e: Entity) -> Option<[i32; 3]> {
        knot_fence(self.world(), e)
    }

    /// Marks the view of the holder `e`: its lead moved.
    pub fn notify_holder(&mut self, e: Entity) {
        view::mark(self.world_mut(), e);
    }
}
