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
    pub equip: fn(&mut World, Entity, Inst, &'static (dyn Any + Sync)),
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
    /// A porte-armure's state (`stand.rs`): needs `Living`.
    Stand,
    /// A projectile in flight (`projectile_more.rs`): needs `Transform`.
    Projectile,
    /// An entity with no tick (display, interaction, marker): `Transform` only.
    Passive,
    /// An entity hung on a block (`hanging.rs`): `Transform` only.
    Hanging,
}
/// The damage an entity just took (the reactions step fills and sends it).
pub struct Damaged {
    pub amount: f32,
    /// Who hurt it (a player or a creature); `direct`: by its own blow, not a projectile.
    pub who: Option<crate::targeting::Who>,
    pub direct: bool,
    /// The damage type (`Hit.cause`): what `#avoids_guardian_thorns` and the like read.
    pub cause: crate::cow::Cause,
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
pub fn no_equip(_: &mut World, _: Entity, _: Inst, _: &'static (dyn Any + Sync)) {}
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
pub fn params<T: 'static>(p: &'static (dyn Any + Sync)) -> &'static T {
    let p: &dyn Any = p;
    p.downcast_ref::<T>().expect("the parameters of another module")
}

pub mod avoid_entity;
pub mod stand_pose;
pub mod breath_air;
pub mod stare_freeze;
pub mod breed;
pub mod swim_with_player;
pub mod climb_on_powder_snow;
pub mod do_nothing;
pub mod timed_idle;
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
pub mod held_swap;
pub mod pick_up_gear;
pub mod follow_player_vehicle;
pub mod follow_parent;
pub mod look_at_player;
pub mod melee;
pub mod panic;
pub mod random_look;
pub mod random_stand;
pub mod elder_curse;
pub mod guardian_beam;
pub mod guardian_thorns;
pub mod limited_life;
pub mod drink_potion;
pub mod ranged_bow;
pub mod pathfind_to_raid;
pub mod door_interact;
pub mod break_door;
pub mod raider_open_door;
pub mod interact_with_door;
pub mod hold_ground_attack;
pub mod heal_raiders;
pub mod spell_casting;
pub mod spell_use;
pub mod ranged_crossbow;
pub mod stroll;
pub mod hurt_by;
pub mod retreat_on_hurt;
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
pub mod croak;
pub mod lay_spawn;
pub mod shoot_tongue;
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
pub mod lay_egg;
pub mod spear_use;
pub mod back_up_if_too_close;
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
pub mod drowned_go_to_water;
pub mod move_to_item;
pub mod drowned_swim_up;
pub mod strike_pause;
pub mod shield_stun;
pub mod ravager_roar;
pub mod hunt_speed;
pub mod headbutt;
pub mod ambient_sound;
pub mod trust;
pub mod variant;
pub mod defend_trusted;
pub mod shelter;
pub mod lookout;
pub mod harness_worn;
pub mod still_under_riders;
pub mod heal_weather;
pub mod square_or_face;
pub mod fly_steer;
pub mod equip_decision;
pub mod equip_source;

/// The registration: one line per module; the order is the order of its systems (all run before `arbitrate`).
pub static TABLE: &[&Module] = &[
    &admire::MODULE,
    &admire_seen::MODULE,
    &crate::dragon::flight::MODULE,
    &vibration_listen::MODULE,
    &block_breaker::MODULE,
    &boss_bar::MODULE,
    &bounce_drag::MODULE,
    &burn_on_touch::MODULE,
    &celebrate::MODULE,
    &convert_timer::MODULE,
    &darkness_pulse::MODULE,
    &dig::MODULE,
    &emerge::MODULE,
    &flee_repellent::MODULE,
    &go_to_disturbance::MODULE,
    &go_to_wanted::MODULE,
    &homing_shot::MODULE,
    &hunt_prey::MODULE,
    &invulnerable_charge::MODULE,
    &item_seek::MODULE,
    &look_at_held::MODULE,
    &peek::MODULE,
    &push_on_touch::MODULE,
    &regen::MODULE,
    &roar_at_target::MODULE,
    &side_heads::MODULE,
    &sniff::MODULE,
    &sonic_boom::MODULE,
    &suspect_anger::MODULE,
    &swallow_item::MODULE,
    &teleport_on::MODULE,
    &trade_restock::MODULE,
    &trade_session::MODULE,
    &trade_show::MODULE,
    &trample::MODULE,
    &walk_away::MODULE,
    &tempt::MODULE,
    &stare_freeze::MODULE,
    &breed::MODULE,
    &breath_air::MODULE,
    &swim_with_player::MODULE,
    &follow_caravan::MODULE,
    &pick_up_gear::MODULE,
    &defend_leash_holder::MODULE,
    &follow_player_vehicle::MODULE,
    &follow_parent::MODULE,
    &look_at_player::MODULE,
    &look_at_player::INTERACT,
    &random_look::MODULE,
    &random_stand::MODULE,
    &float::MODULE,
    &panic::MODULE,
    &hurt_by::MODULE,
    &retreat_on_hurt::MODULE,
    &nearest_target::MODULE,
    &melee::MODULE,
    &elder_curse::MODULE,
    &guardian_beam::MODULE,
    &guardian_thorns::MODULE,
    &limited_life::MODULE,
    &drink_potion::MODULE,
    &ranged_bow::MODULE,
    &spell_casting::MODULE,
    &spell_use::MODULE,
    &ranged_crossbow::MODULE,
    &eat_block::MODULE,
    &stroll::MODULE,
    &stroll::MODULE_LOOK,
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
    &timed_idle::MODULE,
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
    &lay_egg::MODULE,
    &spear_use::MODULE,
    &back_up_if_too_close::MODULE,
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
    &croak::MODULE,
    &lay_spawn::MODULE,
    &shoot_tongue::MODULE,
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
    &drowned_go_to_water::MODULE,
    &move_to_item::MODULE,
    &drowned_swim_up::MODULE,
    &strike_pause::MODULE,
    &shield_stun::MODULE,
    &ravager_roar::MODULE,
    &hunt_speed::MODULE,
    &headbutt::MODULE,
    &ambient_sound::MODULE,
    &defend_trusted::MODULE,
    &shelter::MODULE,
    &lookout::MODULE,
    &still_under_riders::MODULE,
    &heal_weather::MODULE,
    &square_or_face::MODULE,
    &land_on_shoulder::MODULE,
    &stay_close_to_owner::MODULE,
    &give_to_owner::MODULE,
    // The modules of the raids, last: the slots of the modules before them stay where they were.
    &pathfind_to_raid::MODULE,
    &celebration::MODULE,
    &long_distance_patrol::MODULE,
    &move_through_village::MODULE,
    &obtain_banner::MODULE,
    &offer_flower::MODULE,
    &raid_move_through_village::MODULE,
    &defend_village_target::MODULE,
    &break_door::MODULE,
    &break_door::RAID,
    &raider_open_door::MODULE,
    &interact_with_door::MODULE,
    &hold_ground_attack::MODULE,
    &drink_item::MODULE,
    &wander_to_position::MODULE,
    // lot V4
    &warden_look_target::MODULE,
    &warden_drop_invalid_target::MODULE,
];
pub mod act_phase;
pub mod admire;
pub mod admire_seen;
pub mod attached_face;
pub mod bed;
pub mod bell_heard;
pub mod blast_fuse;
pub mod block_breaker;
pub mod block_reach;
pub mod bonemeal;
pub mod boss_bar;
pub mod bounce_drag;
pub mod breed_love;
pub mod burn_on_touch;
pub mod celebrate;
pub mod composter;
pub mod convert_timer;
pub mod crop_harvest;
pub mod darkness_pulse;
pub mod dig;
pub mod emerge;
pub mod flee_repellent;
pub mod food_gift;
pub mod food_inventory;
pub mod food_stock;
pub mod golem_recent;
pub mod summon_defender;
pub mod celebration;
pub mod long_distance_patrol;
pub mod drink_item;
pub mod wander_to_position;
pub mod move_through_village;
pub mod obtain_banner;
pub mod offer_flower;
pub mod raid_move_through_village;
pub mod defend_village_target;
pub mod gossip;
pub mod go_to_disturbance;
pub mod go_to_wanted;
pub mod hide;
pub mod hit_recoil;
pub mod homing_projectile;
pub mod homing_shot;
pub mod hostiles_near;
pub mod hunt_prey;
pub mod inventory;
pub mod invulnerable_charge;
pub mod item_seek;
pub mod jump_on_bed;
pub mod look_at_held;
pub mod nearby_babies;
pub mod nearest_bed;
pub mod nearest_poi;
pub mod peek;
pub mod phase_shield;
pub mod play_tag;
pub mod poi_claim;
pub mod profession;
pub mod push_on_touch;
pub mod raid;
pub mod reach_block;
pub mod regen;
pub mod ring_bell;
pub mod roar_at_target;
// lot V4
pub mod warden_drop_invalid_target;
pub mod warden_look_target;
pub mod schedule;
pub mod side_heads;
pub mod sky_spot;
pub mod sleep_in_bed;
pub mod sniff;
pub mod sonic_boom;
pub mod stroll_near_block;
pub mod poi_strolls;
pub mod suspect_anger;
pub mod swallow_hold;
pub mod swallow_item;
pub mod teleport_on;
pub mod trade_offers;
pub mod trade_restock;
pub mod trade_session;
pub mod trade_show;
pub mod trample;
pub mod vibration_listen;
pub mod victory_celebrate;
pub mod walk_away;
