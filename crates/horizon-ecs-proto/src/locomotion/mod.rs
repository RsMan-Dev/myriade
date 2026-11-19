//! `Locomotion`, the mother of the ways to move: ONE mode is active at a time, by medium (ground, water), changed by the event
//! of entering or leaving a fluid, never by a test each tick. A mode is a module of the same form as the goals (`Module { ..,
//! ..Module::DEFAULT }`, its `Params`, the registry): a species declares `locomotion: &[(Medium::Ground, walk), (Medium::Water,
//! swim)]` and no system knows a species. The active mode is a marker (`WalkMode`, `SwimMode`) that the systems of the mode
//! filter on; `walk` is the walk of the cow, as it was before the mother (`cow.rs`: `travel`, `move_control`, `pathfind`).
use crate::behaviours::{Entry, Module};
use bevy_ecs::prelude::*;

pub mod climb;
pub mod fly;
pub mod hop;
pub mod lava;
pub mod swim;
pub mod walk;

/// The medium a mode moves in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Medium {
    Ground,
    Water,
    Air,
    Lava,
}
/// The registration: one line per mode; the modes are modules of the registry like the goals.
pub static TABLE: &[&Module] = &[&walk::MODULE, &swim::MODULE, &fly::MODULE, &hop::MODULE, &lava::MODULE];

/// A mode the species declares: its medium and the module with its parameters (the `priority` of the entry is not used).
pub type ModeEntry = (Medium, Entry);
/// What a species without `locomotion` of its own walks with.
pub const DEFAULT_MODES: &[ModeEntry] = &[(Medium::Ground, Entry::new(&walk::MODULE, 0, &walk::Params::DEFAULT))];

/// The ground mode is declared and active (the default): the walk's `travel`, `move_control`, `pathfind` and `follow` visit every
/// entity that does not have `SwimMode`, so a body with no locomotion at all (an object) falls with `travel` too.
#[derive(Component, Clone, Copy, Debug)]
pub struct WalkMode;
/// The water mode is active: the systems of `swim` visit it (and the walk's do not).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct SwimMode;

/// The lava mode is active: the walk's `pathfind`, `follow` and `move_control` visit it, `lava::travel` moves it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct LavaMode;

/// The modes of the entity (what `equip` of each was called with): read at the events of entering and leaving a fluid.
#[derive(Component, Clone, Copy)]
#[component(on_add = modes_added, on_insert = modes_counted, on_discard = modes_uncounted)]
pub struct Modes(pub &'static [ModeEntry]);
impl Modes {
    /// Whether the entity has a mode for the medium.
    pub fn has(&self, m: Medium) -> bool {
        self.0.iter().any(|x| x.0 == m)
    }
}
fn modes_uncounted(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(m) = world.get::<Modes>(ctx.entity).map(|m| m.0) else { return };
    let entries: Vec<crate::behaviours::Entry> = m.iter().map(|x| x.1).collect();
    crate::goals::count_carriers(&mut world, &entries, -1);
}
fn modes_counted(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(m) = world.get::<Modes>(ctx.entity).map(|m| m.0) else { return };
    let entries: Vec<crate::behaviours::Entry> = m.iter().map(|x| x.1).collect();
    crate::goals::count_carriers(&mut world, &entries, 1);
}
fn modes_added(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let e = ctx.entity;
    world.commands().queue(move |w: &mut World| {
        let Some(m) = w.get::<Modes>(e).copied() else { return };
        for (_, en) in m.0 {
            (en.module.equip)(w, e, crate::behaviours::Inst { slot: 0, nth: 0 }, en.params);
        }
        // The first mode is the ground's when the species has one, else the first it declares.
        let first = if m.has(Medium::Ground) { Medium::Ground } else { m.0[0].0 };
        set_marker(w, e, first);
    });
}
/// The air mode is active (a flier: the walk's systems do not visit it).
#[derive(Component, Clone, Copy, Debug)]
pub struct FlyMode;

/// Puts the marker of the medium's mode on the entity and takes the others off (the systems of a mode visit what has its marker).
fn set_marker(w: &mut World, e: Entity, to: Medium) {
    let mut em = w.entity_mut(e);
    em.remove::<(WalkMode, SwimMode, FlyMode, LavaMode)>();
    match to {
        Medium::Ground => drop(em.insert(WalkMode)),
        Medium::Water => drop(em.insert(SwimMode)),
        Medium::Air => drop(em.insert(FlyMode)),
        Medium::Lava => drop(em.insert(LavaMode)),
    };
}
/// The service of the change of mode: the entity goes to the mode of `to` if it declares one (a goal asks it to take off or land; a
/// fluid asks it through `fluid_changed`). An event (`Events.mode_changed`) tells the rules. False: no such mode, or already in it.
pub fn switch(w: &mut World, e: Entity, to: Medium) -> bool {
    let Some(m) = w.get::<Modes>(e).copied() else { return false };
    let now = if w.get::<WalkMode>(e).is_some() { Some(Medium::Ground) } else if w.get::<SwimMode>(e).is_some() { Some(Medium::Water) } else if w.get::<FlyMode>(e).is_some() { Some(Medium::Air) } else if w.get::<LavaMode>(e).is_some() { Some(Medium::Lava) } else { None };
    if !m.has(to) || now == Some(to) {
        return false;
    }
    set_marker(w, e, to);
    // The physics looks again from the new mode.
    w.entity_mut(e).insert(crate::cow::Moving);
    w.resource_mut::<crate::goals::Events>().mode_changed.push(e);
    true
}
/// The entity enters (`true`) or leaves a fluid: an entity with a water mode takes it over in the water and gives it back on leaving to
/// its ground mode, or to its air mode (a flier that falls into a lake and comes out of it).
pub fn fluid_changed(cmd: &mut Commands, e: Entity, modes: &Modes, in_water: bool, in_lava: bool) {
    let water = modes.has(Medium::Water) && in_water;
    let lava = modes.has(Medium::Lava) && in_lava && !water;
    if !water && !lava && (in_water || in_lava) {
        return;
    }
    if !modes.has(Medium::Water) && !modes.has(Medium::Lava) {
        return;
    }
    let back = if modes.has(Medium::Ground) { Some(Medium::Ground) } else if modes.has(Medium::Air) { Some(Medium::Air) } else { None };
    cmd.queue(move |w: &mut World| {
        if modes_has_water(w, e) {
            swim::water_changed(w, e, water);
        }
        if water {
            switch(w, e, Medium::Water);
        } else if lava {
            switch(w, e, Medium::Lava);
        } else if let Some(b) = back {
            switch(w, e, b);
        }
    });
}
fn modes_has_water(w: &World, e: Entity) -> bool {
    w.get::<Modes>(e).is_some_and(|m| m.has(Medium::Water))
}
