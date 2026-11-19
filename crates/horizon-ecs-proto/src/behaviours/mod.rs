//! The goal modules. A behaviour is ONE file here that exports `pub static MODULE: Module` (its entry points and its
//! flags) and its `Params` (typed, with a `DEFAULT`); the selector (`goals.rs`) knows no behaviour, and a species
//! (`species/<name>.rs`) is only a list of `Entry { module, priority, params }`. Adding a behaviour: see LISEZMOI.md.
use crate::goals::GoalSlot;
use bevy_ecs::{prelude::*, schedule::Schedule};
use std::any::Any;

/// The identity of one entry of an animal's configuration: `slot` is its rank in the list (the identifier the wheel and
/// the requests use), `nth` counts the entries of the same module before it (the pig's two tempt goals: 0 and 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Inst {
    pub slot: u8,
    pub nth: u8,
}
/// The entry points of a behaviour. Every point takes the world (the module reads and writes its own components);
/// a goal that does not run costs nothing: its systems query the marker `start` inserts.
pub struct Module {
    pub name: &'static str,
    /// MOVE | LOOK | JUMP (0: the goal takes no flag, like a target selector).
    pub flags: u8,
    /// The end of the path this goal asked for ends the goal (stroll, panic, melee).
    pub ends_with_path: bool,
    /// A goal of an equal or higher priority that cannot start because a flag is taken waits (and starts when the flag
    /// frees) unless the module says what to do instead (stroll: ask again later).
    pub blocked: Option<fn(&mut World, Entity, Inst)>,
    /// The selector wants `can_use` asked again when this goal ends (the panic that is still wanted).
    pub retry_on_end: bool,
    /// Called (for the first entry of the module) after any goal of the animal started or ended: a goal whose clock
    /// stands still while it could not be asked (tempt's calm-down) follows the flags.
    pub after_change: Option<fn(&mut World, Entity, Change)>,
    /// Resources the module's systems need, inserted with the world.
    pub resources: fn(&mut World),
    /// Once at creation, for each entry of the species that has this module, with its parameters (downcast to
    /// this module's `Params`): creates or completes the module's state components (by `inst.nth` when several).
    pub equip: fn(&mut World, Entity, Inst, &(dyn Any + Sync)),
    /// The first deadlines: creation, entry into the active zone.
    pub arm: fn(&mut World, Entity, Inst),
    /// A wheel deadline of this entry came up: true = ask `can_use` now.
    pub fired: fn(&mut World, Entity, Inst) -> bool,
    /// `canUse`: yes or no; what it picked (a spot, a player...) is written in the module's own state for `start`.
    pub can_use: fn(&mut World, Entity, Inst) -> bool,
    pub start: fn(&mut World, Entity, Inst),
    pub stop: fn(&mut World, Entity, Inst),
    /// Its bevy systems (each filtered by the module's own marker) and the events that wake it, `.in_set(slot)`.
    pub systems: fn(&mut Schedule, GoalSlot),
    /// The capabilities the module supposes of the entity it is on (what its state components and its systems read).
    /// A species whose configuration has the module without one of them is a startup error (`species::registry`).
    pub requires: &'static [Cap],
    /// Optional, not called yet (the `Damaged` event of the reactions step): the entity took damage.
    pub damaged: fn(&mut World, Entity, Inst, &Damaged),
    /// Optional, not called yet (the interactions step): a player used an item on the entity; true = handled (the first
    /// module that answers true is the only one).
    pub interact: fn(&mut World, Entity, Inst, &Interaction) -> bool,
}
impl Module {
    /// The module that does nothing: write `Module { name, flags, <your entry points>, ..Module::DEFAULT }`, so a field
    /// added later never touches an existing module.
    pub const DEFAULT: Module = Module {
        name: "",
        flags: 0,
        ends_with_path: false,
        blocked: None,
        retry_on_end: false,
        after_change: None,
        resources: no_resources,
        equip: no_equip,
        arm: no_arm,
        fired: always,
        can_use: always,
        start: no_inst,
        stop: no_inst,
        systems: no_systems,
        requires: &[],
        damaged: no_damaged,
        interact: no_interact,
    };
}
/// What an entity can be made of (the sections of its configuration): an entity has the capabilities its configuration
/// declares and nothing else; no capability supposes another except what `Module::requires` states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cap {
    /// A physical body (position, box, gravity, contacts, fluids, index).
    Body,
    /// Life: health, damage, knockback, death, immunities.
    Living,
    Goals,
    Targets,
    /// Walking, paths, the move control.
    Locomotion,
}
/// The damage an entity just took (the reactions step fills and sends it).
pub struct Damaged {
    pub amount: f32,
    pub attacker: Option<Entity>,
}
/// A player's use of an item on an entity (the interactions step fills and sends it).
pub struct Interaction {
    pub player: u32,
    pub item: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Started,
    Ended,
}
/// One line of a species' configuration.
#[derive(Clone, Copy)]
pub struct Entry {
    pub module: &'static Module,
    pub priority: u8,
    pub params: &'static (dyn Any + Sync),
}
impl Entry {
    pub const fn new(module: &'static Module, priority: u8, params: &'static (dyn Any + Sync)) -> Self {
        Self { module, priority, params }
    }
}
pub fn same(a: &'static Module, b: &'static Module) -> bool {
    std::ptr::eq(a, b)
}
/// Default entry points.
pub fn no_resources(_: &mut World) {}
pub fn no_equip(_: &mut World, _: Entity, _: Inst, _: &(dyn Any + Sync)) {}
pub fn no_arm(_: &mut World, _: Entity, _: Inst) {}
pub fn always(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
pub fn no_inst(_: &mut World, _: Entity, _: Inst) {}
pub fn no_damaged(_: &mut World, _: Entity, _: Inst, _: &Damaged) {}
pub fn no_interact(_: &mut World, _: Entity, _: Inst, _: &Interaction) -> bool {
    false
}
pub fn no_systems(_: &mut Schedule, _: GoalSlot) {}
pub fn params<T: 'static>(p: &(dyn Any + Sync)) -> &T {
    let p: &dyn Any = p;
    p.downcast_ref::<T>().expect("the parameters of another module")
}

pub mod breed;
#[cfg(test)]
pub mod example;
pub mod eat_block;
pub mod float;
pub mod follow_parent;
pub mod look_at_player;
pub mod melee;
pub mod panic;
pub mod random_look;
pub mod ranged_bow;
pub mod stroll;
pub mod hurt_by;
pub mod nearest_target;
pub mod tempt;
pub mod fox_state;
pub mod fox_faceplant;
pub mod fox_stalk_prey;
pub mod fox_pounce;

/// The registration: one line per module; the order is the order of its systems (all run before `arbitrate`).
pub static TABLE: &[&Module] = &[
    &tempt::MODULE,
    &breed::MODULE,
    &follow_parent::MODULE,
    &look_at_player::MODULE,
    &random_look::MODULE,
    &float::MODULE,
    &panic::MODULE,
    &hurt_by::MODULE,
    &nearest_target::MODULE,
    &melee::MODULE,
    &ranged_bow::MODULE,
    &eat_block::MODULE,
    &stroll::MODULE,
    &fox_faceplant::MODULE,
    &fox_stalk_prey::MODULE,
    &fox_pounce::MODULE,
];
