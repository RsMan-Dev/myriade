//! The rules of a species that are not goals (`Chicken.aiStep`'s egg timer, `Sheep`'s wool, the wings, the sun that burns the
//! undead): ONE form, a `Rule` with an `equip` that gives the entity its state (its systems read that state, filtered by it),
//! and a `RuleEntry { rule, params }` in the `rules` section of a species configuration. A species that has none of them has
//! no cost for them; nothing here knows a species.
use bevy_ecs::prelude::*;
use std::any::Any;

pub struct Rule {
    pub name: &'static str,
    /// Once at creation: the state components of the rule from its parameters (a rule reads the entity's own components: body
    /// attributes, the random stream; never a species).
    pub equip: fn(&mut World, Entity, &'static (dyn Any + Sync)),
    /// The body attributes the rule gives (read when the bundles are made, so that a bundle made without `equip` has them).
    pub attrs: fn(&'static (dyn Any + Sync)) -> Attrs,
    /// A player used an item on the entity (true: handled).
    pub interact: fn(&mut World, Entity, &'static (dyn Any + Sync), &crate::behaviours::Interaction) -> bool,
}
pub fn no_interact(_: &mut World, _: Entity, _: &'static (dyn Any + Sync), _: &crate::behaviours::Interaction) -> bool {
    false
}
/// What a rule changes of the attributes of the body and the immunities.
#[derive(Clone, Copy, Debug, Default)]
pub struct Attrs {
    pub fall_factor: Option<f64>,
    pub immune: u32,
}
pub fn no_attrs(_: &'static (dyn Any + Sync)) -> Attrs {
    Attrs::default()
}
/// The attributes of all the rules of a species, folded.
pub fn attrs_of(rules: &[RuleEntry]) -> Attrs {
    rules.iter().fold(Attrs::default(), |a, r| {
        let b = (r.rule.attrs)(r.params);
        Attrs { fall_factor: b.fall_factor.or(a.fall_factor), immune: a.immune | b.immune }
    })
}
#[derive(Clone, Copy)]
pub struct RuleEntry {
    pub rule: &'static Rule,
    pub params: &'static (dyn Any + Sync),
}
impl RuleEntry {
    pub const fn new(rule: &'static Rule, params: &'static (dyn Any + Sync)) -> Self {
        Self { rule, params }
    }
}
pub fn params<T: 'static>(p: &'static (dyn Any + Sync)) -> &'static T {
    let p: &dyn Any = p;
    p.downcast_ref::<T>().expect("the parameters of another rule")
}
/// The rules of the base, in the order of their registration (a mod adds its own to `species::Builder`).
pub static BUILTIN: &[&Rule] = &[&crate::farm::WOOL, &crate::farm::LAY_EGG, &crate::farm::WING_FLAP, &crate::hostile::SUN_BURN, &crate::tame::TAME, &crate::loot::XP, &crate::mount::RIDE, &crate::leash::LEASH, &crate::equipment::EQUIP, &crate::behaviours::held_swap::HELD_SWAP, &crate::size::SIZE, &crate::effects::ON_HIT, &crate::behaviours::breed::LOVE];
/// Gives an entity the state of every rule of its configuration.
pub fn equip_all(world: &mut World, e: Entity, rules: &[RuleEntry]) {
    for r in rules {
        (r.rule.equip)(world, e, r.params);
    }
}

// Pure calculations taken out of the systems (no ECS type in a signature): the public API a mod reuses for its own entities.
pub mod admire;
pub mod admire_seen;
pub mod ambient_sound;
pub mod animal_panic;
pub mod attached_face;
pub mod bat_flutter;
pub mod bed;
pub mod beg;
pub mod bell_heard;
pub mod block_breaker;
pub mod block_reach;
pub mod bonemeal;
pub mod bounce_drag;
pub mod break_door;
pub mod breeze;
pub mod cat_state;
pub mod celebration;
pub mod climb;
pub mod composter;
pub mod convert_timer;
pub mod defend_leash_holder;
pub mod defend_trusted;
pub mod defend_village_target;
pub mod door_interact;
pub mod drink_item;
pub mod drink_potion;
pub mod drowned_go_to_beach;
pub mod drowned_go_to_water;
pub mod drowned_swim_up;
pub mod eat_block;
pub mod elder_curse;
pub mod ender_teleport;
pub mod fight;
pub mod find_liquid;
pub mod flee_sun;
pub mod flight;
pub mod float_around;
pub mod float;
pub mod fly;
pub mod fly_steer;
pub mod fly_stroll;
pub mod follow_adult;
pub mod follow_flock_leader;
pub mod follow_mob;
pub mod follow_owner;
pub mod follow_parent;
pub mod follow_player_vehicle;
pub mod follow_temptation;
pub mod food_gift;
pub mod food_inventory;
pub mod food_stock;
pub mod forget_target;
pub mod fox_eat_berries;
pub mod fox_faceplant;
pub mod fox_pounce;
pub mod fox_stalk_prey;
pub mod freeze_when_looked_at;
pub mod ghast_look;
pub mod give_to_owner;
pub mod gossip;
pub mod go_to_disturbance;
pub mod go_to_wanted;
pub mod headbutt;
pub mod heal_raiders;
pub mod heal_weather;
pub mod hit_recoil;
pub mod homing_projectile;
pub mod homing_shot;
pub mod hop;
pub mod hostiles_near;
pub mod hunt_speed;
pub mod interact_with_door;
pub mod invulnerable_charge;
pub mod item_seek;
pub mod jump_on_bed;
pub mod lava;
pub mod lay_egg;
pub mod lay_spawn;
pub mod leap_at_target;
pub mod leave_block;
pub mod look_and_interact;
pub mod look_at_entity;
pub mod look_at_player;
pub mod look_for_player;
pub mod make_love;
pub mod melee_attack;
pub mod move_through_village;
pub mod move_to_item;
pub mod move_to_target_sink;
pub mod move_towards_restriction;
pub mod move_towards_target;
pub mod nearest_target;
pub mod neutral_anger;
pub mod obtain_banner;
pub mod ocelot_attack;
pub mod offer_flower;
pub mod owner_target;
pub mod panda_lie_on_back;
pub mod panda_roll;
pub mod panda_sit;
pub mod panda_sneeze;
pub mod panda_state;
pub mod peek;
pub mod phase_shield;
pub mod pick_up_gear;
pub mod play_with_items;
pub mod poi_assign;
pub mod poi_claim;
pub mod poi_validate;
pub mod posture;
pub mod pufferfish_puff;
pub mod push_on_touch;
pub mod raid_garden;
pub mod random_look;
pub mod random_stand;
pub mod ranged_bow;
pub mod schedule;
pub mod shelter;
pub mod shield_stun;
pub mod shoot_tongue;
pub mod side_heads;
pub mod silverfish_merge;
pub mod silverfish_wake_up_friends;
pub mod sit_when_ordered;
pub mod sky_spot;
pub mod sleep_in_bed;
pub mod sniff;
pub mod snow_trail;
pub mod social_gift_hero;
pub mod social_socialize_bell;
pub mod sonic_boom;
pub mod spear_use;
pub mod spell_use;
pub mod swim;
pub mod swim_stroll;
pub mod trade_follow_player;
pub mod trade_show_offers;
pub mod transport_items;
pub mod village_home_walk;
pub mod villager_interact;
pub mod work_swim;
