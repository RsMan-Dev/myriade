//! `Locomotion`, the mother of the ways to move: ONE mode is active at a time, by medium (ground, water), changed by the event
//! of entering or leaving a fluid, never by a test each tick. A mode is a module of the same form as the goals (`Module { ..,
//! ..Module::DEFAULT }`, its `Params`, the registry): a species declares `locomotion: &[(Medium::Ground, walk), (Medium::Water,
//! swim)]` and no system knows a species. The active mode is a marker (`WalkMode`, `SwimMode`) that the systems of the mode
//! filter on; `walk` is the walk of the cow, as it was before the mother (`cow.rs`: `travel`, `move_control`, `pathfind`).
use crate::behaviours::{Entry, Module};
use bevy_ecs::prelude::*;

pub mod swim;
pub mod walk;

/// The medium a mode moves in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Medium {
    Ground,
    Water,
}
/// The registration: one line per mode; the modes are modules of the registry like the goals.
pub static TABLE: &[&Module] = &[&walk::MODULE, &swim::MODULE];

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

/// The modes of the entity (what `equip` of each was called with): read at the events of entering and leaving a fluid.
#[derive(Component, Clone, Copy)]
#[component(on_add = modes_added)]
pub struct Modes(pub &'static [ModeEntry]);
impl Modes {
    /// Whether the entity has a mode for the medium.
    pub fn has(&self, m: Medium) -> bool {
        self.0.iter().any(|x| x.0 == m)
    }
}
fn modes_added(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let e = ctx.entity;
    world.commands().queue(move |w: &mut World| {
        let Some(m) = w.get::<Modes>(e).copied() else { return };
        for (_, en) in m.0 {
            (en.module.equip)(w, e, crate::behaviours::Inst { slot: 0, nth: 0 }, en.params);
        }
    });
}
/// The entity enters (`true`) or leaves a fluid: the mode of the medium becomes the active one. Only an entity with a water mode
/// changes; the others keep their walk.
pub fn fluid_changed(cmd: &mut Commands, e: Entity, modes: &Modes, in_water: bool) {
    if !modes.has(Medium::Water) {
        return;
    }
    if in_water {
        cmd.entity(e).remove::<WalkMode>().insert(SwimMode);
    } else if modes.has(Medium::Ground) {
        cmd.entity(e).remove::<SwimMode>().insert(WalkMode);
    }
}
