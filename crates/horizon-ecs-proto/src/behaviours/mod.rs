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
    /// The tick of a module that needs no place of its own in the schedule: called by `arbitrate`, in the order of the registry, only while
    /// some live animal carries the module (`Carriers`): a module that does not run costs one test, not a system and its run condition.
    /// A module has `systems` OR `tick` (`goals::tick_state` runs a bevy system function as that tick, its parameters kept in a `SystemState` of the world).
    pub tick: Option<fn(&mut World)>,
    /// The capabilities the module supposes of the entity it is on (what its state components and its systems read).
    /// A species whose configuration has the module without one of them is a startup error (`species::registry`).
    pub requires: &'static [Cap],
    /// A running goal that is not interruptible keeps its flags against any goal, whatever the priority (`Goal.isInterruptable`:
    /// the fox's pounce, the panda's roll).
    pub interruptable: bool,
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
        tick: None,
        requires: &[],
        interruptable: true,
        damaged: no_damaged,
        interact: no_interact,
    };
}
/// What an entity can be made of (the sections of its configuration): an entity has the capabilities its configuration
/// declares and nothing else; no capability supposes another except what `Module::requires` states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cap {
    /// Position, velocity, own time and identity: all a projectile or a display has.
    Transform,
    /// A box in the spatial index (pushed, searched, targeted): `Bounds`, `Cell`.
    Solid,
    /// A body that falls, lands, floats and is pushed by blocks and fluids (`travel`): needs `Transform` and `Solid`.
    Body,
    /// Life: health, damage, knockback, death, immunities.
    Living,
    Goals,
    /// The head: where it looks and how it turns (`Look`, `Head`); the look goals and every goal that aims need it.
    Head,
    Targets,
    /// Memories, sensors, activities and behaviours (`brain`): needs `Goals` (its random stream, its look).
    Brain,
    /// Walking, paths, the move control.
    Locomotion,
    /// A rider can sit on it (`mount.rs`): the component `Mount`.
    Rideable,
    /// An owner (`tame.rs`): the component `Owner`; taming and sitting are a rule and an occupation.
    Tamable,
    /// A stack on the ground (`item.rs`): needs `Transform`; no life, no goals, no box in the creature index.
    Item,
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
    /// The condition of the species for this entry, asked by the framework before the module's `can_use`.
    pub when: Option<&'static Cond>,
}
impl Entry {
    pub const fn new(module: &'static Module, priority: u8, params: &'static (dyn Any + Sync)) -> Self {
        Self { module, priority, params, when: None }
    }
    /// The same entry that starts only while `cond` holds: `Entry::new(..).when(&Cond::Is("horizon:is_baby"))`.
    pub const fn when(self, cond: &'static Cond) -> Self {
        Self { when: Some(cond), ..self }
    }
}
/// A condition on an entity, in DATA: named predicates (registered with the registry: `horizon:is_baby`, `horizon:is_day`,
/// `horizon:in_water`...) combined with not / all / any. A predicate reads components of the entity, never a species. The
/// names are checked when the registry is frozen (an unknown one is a startup error naming the species).
#[derive(Debug)]
pub enum Cond {
    Is(&'static str),
    Not(&'static Cond),
    All(&'static [Cond]),
    Any(&'static [Cond]),
}
/// A named predicate: a plain function of the world and the entity.
pub type Pred = fn(&World, Entity) -> bool;
impl Cond {
    pub fn eval(&self, w: &World, e: Entity) -> bool {
        match self {
            Cond::Is(name) => crate::species::registry().predicate(name).is_some_and(|p| p(w, e)),
            Cond::Not(c) => !c.eval(w, e),
            Cond::All(cs) => cs.iter().all(|c| c.eval(w, e)),
            Cond::Any(cs) => cs.iter().any(|c| c.eval(w, e)),
        }
    }
    /// The predicate names in it.
    pub fn names(&self, out: &mut Vec<&'static str>) {
        match self {
            Cond::Is(n) => out.push(n),
            Cond::Not(c) => c.names(out),
            Cond::All(cs) | Cond::Any(cs) => cs.iter().for_each(|c| c.names(out)),
        }
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

pub mod avoid_entity;
pub mod breath_air;
pub mod breed;
pub mod swim_with_player;
pub mod climb_on_powder_snow;
pub mod do_nothing;
pub mod silverfish_wake_up_friends;
#[cfg(test)]
pub mod example;
pub mod eat_block;
pub mod flee_sun;
pub mod fish_swim;
pub mod float_around;
pub mod bat_flutter;
pub mod flier_goals;
pub mod bee;
pub mod squid_goals;
pub mod fly_stroll;
pub mod float;
pub mod defend_leash_holder;
pub mod follow_caravan;
pub mod follow_player_vehicle;
pub mod follow_parent;
pub mod look_at_player;
pub mod melee;
pub mod panic;
pub mod random_look;
pub mod random_stand;
pub mod ranged_bow;
pub mod stroll;
pub mod hurt_by;
pub mod nearest_target;
pub mod panda_lie_on_back;
pub mod panda_roll;
pub mod panda_sneeze;
pub mod panda_state;
pub mod tempt;
pub mod fox_state;
pub mod fox_faceplant;
pub mod fox_stalk_prey;
pub mod fox_pounce;
pub mod fox_sleep;
pub mod fox_eat_berries;
pub mod long_jump;
pub mod ram;
pub mod fox_search_for_items;
pub mod fox_pick_up_loot;
pub mod armadillo_state;
pub mod baby_follow_adult;
pub mod rolled;
pub mod swell;
pub mod look_for_player;
pub mod take_block;
pub mod leave_block;
pub mod freeze_when_looked_at;
pub mod strider_go_to_lava;
pub mod ghast_look;
pub mod move_towards_target;
pub mod ranged_attack;
pub mod leap_at_target;
pub mod attack_turtle_egg;
pub mod brute_stroll_home;
pub mod silverfish_merge;
pub mod follow_owner;
pub mod stay_close_to_owner;
pub mod give_to_owner;
pub mod land_on_shoulder;
pub mod burst_shot;
pub mod ender_teleport;
pub mod charged_shot;
pub mod restrict_sun;
pub mod village_walk;
pub mod run_around_like_crazy;
pub mod owner_target;
pub mod sit_when_ordered;
pub mod neutral_anger;
pub mod reset_universal_anger;
pub mod panda_sit;
pub mod follow_flock_leader;
pub mod pufferfish_puff;
pub mod raid_garden;
pub mod follow_mob;
pub mod turtle_state;
pub mod turtle_go_to_water;
pub mod turtle_travel;
pub mod turtle_stroll;
pub mod turtle_lay_egg;
pub mod turtle_go_home;
pub mod snow_trail;
pub mod beg;
pub mod forget_target;
pub mod cat_state;
pub mod cat_relax_on_owner;
pub mod cat_lie_on_block;
pub mod cat_sit_on_block;
pub mod ocelot_attack;
pub mod posture;
pub mod random_sitting;
pub mod animal_panic;
pub mod water_jump;
pub mod play_with_items;
pub mod charge_attack;
pub mod swim_to_treasure;
pub mod item_pickup;
pub mod cube_attack;
pub mod move_towards_restriction;
pub mod fox_stroll_village;
pub mod try_find_water;
pub mod drowned_go_to_beach;
pub mod drowned_swim_up;

/// The registration: one line per module; the order is the order of its systems (all run before `arbitrate`).
pub static TABLE: &[&Module] = &[
    &tempt::MODULE,
    &breed::MODULE,
    &breath_air::MODULE,
    &swim_with_player::MODULE,
    &follow_caravan::MODULE,
    &defend_leash_holder::MODULE,
    &follow_player_vehicle::MODULE,
    &follow_parent::MODULE,
    &look_at_player::MODULE,
    &random_look::MODULE,
    &random_stand::MODULE,
    &float::MODULE,
    &panic::MODULE,
    &hurt_by::MODULE,
    &nearest_target::MODULE,
    &melee::MODULE,
    &ranged_bow::MODULE,
    &eat_block::MODULE,
    &stroll::MODULE,
    &avoid_entity::MODULE,
    &flee_sun::MODULE,
    &panda_sneeze::MODULE,
    &panda_lie_on_back::MODULE,
    &panda_roll::MODULE,
    &fox_faceplant::MODULE,
    &fox_stalk_prey::MODULE,
    &fox_pounce::MODULE,
    &swell::MODULE,
    &move_towards_target::MODULE,
    &ranged_attack::MODULE,
    &water_jump::MODULE,
    &play_with_items::MODULE,
    &leap_at_target::MODULE,
    &baby_follow_adult::MODULE,
    &rolled::MODULE,
    &neutral_anger::MODULE,
    &reset_universal_anger::MODULE,
    &panda_sit::MODULE,
    &climb_on_powder_snow::MODULE,
    &silverfish_wake_up_friends::MODULE,
    &do_nothing::MODULE,
    &fish_swim::MODULE,
    &look_for_player::MODULE,
    &take_block::MODULE,
    &leave_block::MODULE,
    &freeze_when_looked_at::MODULE,
    &strider_go_to_lava::MODULE,
    &ghast_look::MODULE,
    &fox_sleep::MODULE,
    &fox_eat_berries::MODULE,
    &fox_search_for_items::MODULE,
    &fox_pick_up_loot::MODULE,
    &attack_turtle_egg::MODULE,
    &brute_stroll_home::MODULE,
    &silverfish_merge::MODULE,
    &fly_stroll::MODULE,
    &float_around::MODULE,
    &bat_flutter::MODULE,
    &squid_goals::SQUID_RANDOM_MOVEMENT,
    &squid_goals::SQUID_FLEE,
    &bee::POLLINATE,
    &bee::LOCATE_HIVE,
    &bee::GO_TO_HIVE,
    &bee::ENTER_HIVE,
    &bee::VALIDATE_HIVE,
    &bee::VALIDATE_FLOWER,
    &bee::GO_TO_KNOWN_FLOWER,
    &bee::GROW_CROP,
    &flier_goals::VEX_CHARGE,
    &flier_goals::VEX_RANDOM_MOVE,
    &flier_goals::VEX_COPY_OWNER_TARGET,
    &flier_goals::PHANTOM_ATTACK_PLAYER_TARGET,
    &flier_goals::PHANTOM_ATTACK_STRATEGY,
    &flier_goals::PHANTOM_SWEEP_ATTACK,
    &flier_goals::PHANTOM_CIRCLE_AROUND_ANCHOR,
    &follow_owner::MODULE,
    &land_on_shoulder::MODULE,
    &stay_close_to_owner::MODULE,
    &give_to_owner::MODULE,
    &village_walk::MODULE,
    &restrict_sun::MODULE,
    &burst_shot::MODULE,
    &ender_teleport::MODULE,
    &charged_shot::MODULE,
    &run_around_like_crazy::MODULE,
    &owner_target::MODULE,
    &sit_when_ordered::MODULE,
    &follow_flock_leader::MODULE,
    &pufferfish_puff::MODULE,
    &raid_garden::MODULE,
    &follow_mob::MODULE,
    &turtle_go_to_water::MODULE,
    &turtle_travel::MODULE,
    &turtle_stroll::MODULE,
    &turtle_lay_egg::MODULE,
    &turtle_go_home::MODULE,
    &snow_trail::MODULE,
    &beg::MODULE,
    &forget_target::MODULE,
    &cat_relax_on_owner::MODULE,
    &cat_lie_on_block::MODULE,
    &cat_sit_on_block::MODULE,
    &ocelot_attack::MODULE,
    &long_jump::MODULE,
    &ram::MODULE,
    &posture::MODULE,
    &random_sitting::MODULE,
    &animal_panic::MODULE,
    &charge_attack::MODULE,
    &swim_to_treasure::MODULE,
    &item_pickup::MODULE,
    &cube_attack::MODULE,
    &move_towards_restriction::MODULE,
    &move_towards_restriction::MODULE_LOOK,
    &fox_stroll_village::MODULE,
    &try_find_water::MODULE,
    &drowned_go_to_beach::MODULE,
    &drowned_swim_up::MODULE,
];
