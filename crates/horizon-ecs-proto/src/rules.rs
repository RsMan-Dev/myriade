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
pub static BUILTIN: &[&Rule] = &[&crate::behaviours::shear::SHEAR, &crate::behaviours::brush::RULE, &crate::behaviours::stew_flower::STEW_FLOWER, &crate::behaviours::stew_flower::STEW_BOWL, &crate::farm::LAY_EGG, &crate::farm::WING_FLAP, &crate::hostile::SUN_BURN, &crate::tame::TAME, &crate::loot::XP, &crate::loot::BABY_DROPS, &crate::loot::FLAT_XP, &crate::loot::NO_XP_DROP, &crate::mount::RIDE, &crate::leash::LEASH, &crate::equipment::EQUIP, &crate::behaviours::held_swap::HELD_SWAP, &crate::size::SIZE, &crate::effects::ON_HIT, &crate::behaviours::breed::LOVE, &crate::give_item::GIVE, &crate::hand_over::HAND_OVER, &crate::fit_chest::FIT_CHEST, &crate::coating::COAT, &crate::behaviours::contact_damage::VICTIM, &crate::gift_drop::PERIODIC_GIFT, &crate::gift_drop::GROW_GIFT, &crate::ignite::IGNITE, &crate::dye::DYE, &crate::spawn_offspring::SPAWN_BY_EGG, &crate::body_armor::WEAR];
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
pub mod area;
pub mod attached_face;
pub mod attachments;
pub mod attack_turtle_egg;
pub mod avoid_entity;
pub mod baby_follow_adult;
pub mod back_up_if_too_close;
pub mod bat_flutter;
pub mod bed;
pub mod bee;
pub mod beg;
pub mod bell_heard;
pub mod block_breaker;
pub mod block_reach;
pub mod boat;
pub mod bonemeal;
pub mod boss_bar;
pub mod bounce_drag;
pub mod break_door;
pub mod breath_air;
pub mod breath_cloud;
pub mod breed;
pub mod breed_love;
pub mod breeze;
pub mod brute_stroll_home;
pub mod burst_shot;
pub mod calm_down;
pub mod cat_lie_on_block;
pub mod cat_relax_on_owner;
pub mod cat_sit_on_block;
pub mod cat_state;
pub mod celebration;
pub mod charge_attack;
pub mod charged_shot;
pub mod climb;
pub mod climb_on_powder_snow;
pub mod climbable;
pub mod pushable;
pub mod bite_sound;
pub mod composter;
pub mod convert_timer;
pub mod cube_attack;
pub mod defend_leash_holder;
pub mod defend_trusted;
pub mod defend_village_target;
pub mod despawn;
pub mod dig;
pub mod door_interact;
pub mod dragon_breath;
pub mod dragon_crystals;
pub mod dragon_data;
pub mod dragon_death;
pub mod dragon_fireball;
pub mod dragon_hurt;
pub mod dragon_nodes;
pub mod dragon_parts;
pub mod dragon_perch;
pub mod dragon_phases;
pub mod dragon_portal;
pub mod dragon_respawn;
pub mod drink_item;
pub mod drink_potion;
pub mod drowned_go_to_beach;
pub mod drowned_go_to_water;
pub mod drowned_swim_up;
pub mod eat_block;
pub mod effect_cloud;
pub mod effects;
pub mod elder_curse;
pub mod end_crystal;
pub mod ender_teleport;
pub mod equipment;
pub mod equipment_drop;
pub mod equipment_enchant;
pub mod equipment_wear;
pub mod explosion;
pub mod eye;
pub mod falling_block;
pub mod farm;
pub mod fight;
pub mod find_liquid;
pub mod firework;
pub mod fish_swim;
pub mod fishing_hook;
pub mod flat;
pub mod flee_sun;
pub mod flier_goals;
pub mod flight;
pub mod float;
pub mod float_around;
pub mod fly;
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
pub mod fox_pick_up_loot;
pub mod fox_pounce;
pub mod fox_search_for_items;
pub mod fox_sleep;
pub mod fox_stalk_prey;
pub mod fox_state;
pub mod fox_stroll_village;
pub mod freeze_when_looked_at;
pub mod ghast_look;
pub mod give_to_owner;
pub mod go_to_disturbance;
pub mod go_to_wanted;
pub mod gossip;
pub mod guardian_thorns;
pub mod hanging;
pub mod harness_worn;
pub mod headbutt;
pub mod heal_raiders;
pub mod heal_weather;
pub mod health;
pub mod hide;
pub mod hit_recoil;
pub mod hold_ground_attack;
pub mod homing_projectile;
pub mod homing_shot;
pub mod hop;
pub mod hostile;
pub mod hostiles_near;
pub mod hunt_speed;
pub mod interact_with_door;
pub mod invulnerable_charge;
pub mod item;
pub mod item_pickup;
pub mod item_seek;
pub mod jump_arc;
pub mod jump_on_bed;
pub mod land_on_shoulder;
pub mod lava;
pub mod lay_egg;
pub mod lay_spawn;
pub mod leap_at_target;
pub mod leash;
pub mod leave_block;
pub mod level;
pub mod lightning;
pub mod locomotion;
pub mod long_distance_patrol;
pub mod look_and_interact;
pub mod look_at_entity;
pub mod look_at_player;
pub mod look_at_player_sometimes;
pub mod look_for_player;
pub mod lookout;
pub mod loot;
pub mod make_love;
pub mod melee;
pub mod melee_attack;
pub mod minecart;
pub mod mount;
pub mod move_through_village;
pub mod move_to_block;
pub mod move_to_item;
pub mod move_to_target_sink;
pub mod move_towards_restriction;
pub mod move_towards_target;
pub mod nbt_tag;
pub mod nearest_target;
pub mod neutral_anger;
pub mod obtain_banner;
pub mod ocelot_attack;
pub mod offer_flower;
pub mod ominous_spawner;
pub mod orb;
pub mod owner_target;
pub mod panda_lie_on_back;
pub mod panda_roll;
pub mod panda_sit;
pub mod panda_sneeze;
pub mod panda_state;
pub mod pathsearch;
pub mod peek;
pub mod phase_shield;
pub mod pick_up_gear;
pub mod play_dead;
pub mod play_with_items;
pub mod poi;
pub mod poi_assign;
pub mod poi_claim;
pub mod poi_competitor_scan;
pub mod poi_strolls;
pub mod poi_validate;
pub mod posture;
pub mod projectile_more;
pub mod pufferfish_puff;
pub mod push_on_touch;
pub mod raid_garden;
pub mod random_look;
pub mod random_sitting;
pub mod random_stand;
pub mod ranged_attack;
pub mod ranged_bow;
pub mod ranged_crossbow;
pub mod ravager_roar;
pub mod reach_block;
pub mod regen;
pub mod retreat_on_hurt;
pub mod ring_bell;
pub mod schedule;
pub mod shelter;
pub mod shield_stun;
pub mod shoot_tongue;
pub mod side_heads;
pub mod silverfish_merge;
pub mod silverfish_wake_up_friends;
pub mod sit_when_ordered;
pub mod size;
pub mod sky_spot;
pub mod sleep_in_bed;
pub mod sniff;
pub mod snow_trail;
pub mod social_gift_hero;
pub mod social_socialize_bell;
pub mod sonic_boom;
pub mod spawn_draw;
pub mod spawn_finalize;
pub mod spawn_spawner;
pub mod spawn_trial;
pub mod spear_use;
pub mod spell_use;
pub mod spots;
pub mod square_or_face;
pub mod squid_goals;
pub mod stand;
pub mod stare_freeze;
pub mod stay_close_to_owner;
pub mod stop_attacking_if_target_invalid;
pub mod strider_go_to_lava;
pub mod stroll;
pub mod stroll_near_block;
pub mod suspect_anger;
pub mod swell;
pub mod swim;
pub mod swim_stroll;
pub mod swim_to_treasure;
pub mod swim_with_player;
pub mod take_block;
pub mod tame;
pub mod teleport_on;
pub mod tempt;
pub mod timed_idle;
pub mod tnt;
pub mod touch;
pub mod trade_follow_player;
pub mod trade_offers;
pub mod trade_restock;
pub mod trade_session;
pub mod trade_show;
pub mod trade_show_offers;
pub mod trade_with_villager;
pub mod transport_items;
pub mod try_find_water;
pub mod turtle_go_home;
pub mod turtle_go_to_water;
pub mod turtle_state;
pub mod turtle_stroll;
pub mod turtle_travel;
pub mod variant;
pub mod vehicle;
pub mod vibration_listen;
pub mod victory_celebrate;
pub mod village_brownian_walk;
pub mod village_go_to_village;
pub mod village_home_walk;
pub mod village_stroll;
pub mod village_walk;
pub mod villager_interact;
pub mod walk_away;
pub mod wander_to_position;
pub mod warden_drop_invalid_target;
pub mod warden_look_target;
pub mod water_jump;
pub mod work_swim;
pub mod hook_wait;
pub mod push_rules;
pub mod seat_rules;
