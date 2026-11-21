//! Le villageois : un cerveau (adulte ou bébé selon `GrowAt`), horaire vanilla (`villager.rs:131`). Ordre des activités = index de `Brain.activity`.
//! Hors de ce fichier : capteurs de lit, de bébés, de golem et d'hostiles, poste et cloche (`JOB_SITE`, `MEETING_POINT` à
//! alimenter par la partie POI), le jeu des bébés (`play`), le commerce, la reproduction, la panique du raid (voir villager.md).
use bevy_ecs::prelude::{Entity, World};
use super::Def;
use crate::behaviours::Cap;
use crate::behaviours::{bed::NEAREST_BED, composter, crop_harvest, jump_on_bed, nearest_bed, nearby_babies, panda_state, play_tag, poi_strolls, gossip, raid, reach_block, ring_bell, schedule, sleep_in_bed, stroll_near_block, summon_defender, victory_celebrate};
use crate::brain::behaviours::{do_nothing, interact_with_door, look_at_player_sometimes, look_at_target_sink, move_to_target_sink, panic, run_one, stroll, walk_away, walk_to_look_target};
use crate::brain::sensors::{hurt_by, living, secondary_job_site::{self, SECONDARY_JOB_SITE}};
use crate::brain::*;
// lot 6
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::brain::behaviours::{calm_down, look_and_interact, look_at_entity, villager_interact};
// lot V4
use crate::behaviours::bonemeal;
use crate::behaviours::breed_love;
use crate::brain::behaviours::{work_swim, work_wake_up};
// lot V3
use crate::brain::behaviours::{raid_status, village_brownian_walk, village_go_to_village, village_home_walk, village_stroll};
use crate::brain::sensors::village_hostile;
use crate::behaviours::{hide, sky_spot};
// lot V2
use crate::behaviours::trade_offers::{self, Merchant};
use crate::brain::behaviours::{social_gift_hero, social_socialize_bell, trade_follow_player, trade_go_to_wanted, trade_restock_work, trade_show_offers, trade_with_villager};
// lot V1
use crate::behaviours::poi_claim;
use crate::brain::behaviours::{poi_acquire, poi_assign, poi_competitor_scan, poi_go_to_site, poi_validate, poi_yield_site};
use horizon_world::poi::PoiType;

pub static SCHEDULE_IDLE: MemKey = MemKey::new("minecraft:schedule/idle", Shape::Unit);
pub static SCHEDULE_WORK: MemKey = MemKey::new("minecraft:schedule/work", Shape::Unit);
pub static SCHEDULE_MEET: MemKey = MemKey::new("minecraft:schedule/meet", Shape::Unit);
pub static SCHEDULE_PLAY: MemKey = MemKey::new("minecraft:schedule/play", Shape::Unit);
pub static SCHEDULE_REST: MemKey = MemKey::new("minecraft:schedule/rest", Shape::Unit);
pub static JOB_SITE: MemKey = MemKey::new("minecraft:job_site", Shape::Block);
pub static MEETING_POINT: MemKey = MemKey::new("minecraft:meeting_point", Shape::Block);
// lot V1
pub static POTENTIAL_JOB_SITE: MemKey = MemKey::new("minecraft:potential_job_site", Shape::Block);
pub static HOME: MemKey = MemKey::new("minecraft:home", Shape::Block);

static ACTIVITIES: &[ActivityDef] = &[
    ActivityDef { name: "core", slot: 1, requires: &[] },
    ActivityDef { name: "play", slot: 0, requires: &[(&SCHEDULE_PLAY, Need::Present)] },
    ActivityDef { name: "rest", slot: 2, requires: &[(&SCHEDULE_REST, Need::Present)] },
    // lot V3 : `hide`, `pre_raid` et `raid` tiennent tant que leur mémoire (posée par `ReactToBell` / `SetRaidStatus`, ôtée par `SetHiddenState` / `ResetRaidStatus`) est posée
    ActivityDef { name: "hide", slot: 3, requires: &[(&hide::HIDE_ON, Need::Present)] },
    // `Villager.registerBrainGoals` (jar): work needs JOB_SITE, meet needs MEETING_POINT; idle is the fallback (no requirement).
    ActivityDef { name: "meet", slot: 4, requires: &[(&SCHEDULE_MEET, Need::Present), (&MEETING_POINT, Need::Present)] },
    ActivityDef { name: "idle", slot: 5, requires: &[] },
    ActivityDef { name: "pre_raid", slot: 6, requires: &[(&raid::PRE_RAID_ON, Need::Present)] },
    ActivityDef { name: "work", slot: 7, requires: &[(&SCHEDULE_WORK, Need::Present), (&JOB_SITE, Need::Present)] },
    ActivityDef { name: "panic", slot: 8, requires: &[(&HURT_BY, Need::Present)] },
    ActivityDef { name: "raid", slot: 9, requires: &[(&raid::RAID_ON, Need::Present)] },
];
static PICK: &[&str] = &["panic", "raid", "pre_raid", "hide", "rest", "meet", "work", "play", "idle"];

static ADULT_SLICES: &[schedule::Slice] = &[
    schedule::Slice { key: &SCHEDULE_IDLE, from: 10, to: 2000 },
    schedule::Slice { key: &SCHEDULE_WORK, from: 2000, to: 9000 },
    schedule::Slice { key: &SCHEDULE_MEET, from: 9000, to: 11000 },
    schedule::Slice { key: &SCHEDULE_IDLE, from: 11000, to: 12000 },
    schedule::Slice { key: &SCHEDULE_REST, from: 12000, to: 24000 },
    schedule::Slice { key: &SCHEDULE_REST, from: 0, to: 10 },
];
static BABY_SLICES: &[schedule::Slice] = &[
    schedule::Slice { key: &SCHEDULE_IDLE, from: 10, to: 3000 },
    schedule::Slice { key: &SCHEDULE_PLAY, from: 3000, to: 6000 },
    schedule::Slice { key: &SCHEDULE_IDLE, from: 6000, to: 10000 },
    schedule::Slice { key: &SCHEDULE_PLAY, from: 10000, to: 12000 },
    schedule::Slice { key: &SCHEDULE_REST, from: 12000, to: 24000 },
    schedule::Slice { key: &SCHEDULE_REST, from: 0, to: 10 },
];

/// The look target's walk (`WalkToLookTarget`: speed 0.5, close 2) and the bed jump of a baby (`JumpOnBed`, `is_baby` on the Mob).
const WALK_LOOK: walk_to_look_target::Params = walk_to_look_target::Params { speed: 0.5, close: 2, ..walk_to_look_target::Params::DEFAULT };
const BABY_JUMP: jump_on_bed::Params = jump_on_bed::Params { baby: panda_state::is_baby };
const LAND_STROLL: stroll::Params = stroll::Params { speed: 0.5, h: 10, v: 7, dry_only: false };
// lot V3 : les hostiles et leur distance (`vanilla_villager_distance`, `neoforge_mob_maps.rs`), le repos sans lit (`Gate.runOne`, absent Home)
const HOSTILES: village_hostile::Params = village_hostile::Params {
    types: &[
        ("minecraft:drowned", 8.0), ("minecraft:husk", 8.0), ("minecraft:vex", 8.0), ("minecraft:zombie", 8.0), ("minecraft:zombie_villager", 8.0),
        ("minecraft:evoker", 12.0), ("minecraft:illusioner", 12.0), ("minecraft:ravager", 12.0), ("minecraft:pillager", 15.0),
        ("minecraft:vindicator", 10.0), ("minecraft:zoglin", 10.0),
    ],
};
static HOMELESS_KIDS: &[Kid] = &[
    Kid { behaviour: &village_home_walk::BEHAVIOUR, weight: 1, params: &village_home_walk::Params { range: 48, retry: 20, home: &HOME }, needs: &[], duration: None },
    Kid { behaviour: &village_brownian_walk::BEHAVIOUR, weight: 4, params: &village_brownian_walk::Params { speed: 0.5 }, needs: &[], duration: None },
    Kid { behaviour: &village_go_to_village::BEHAVIOUR, weight: 2, params: &village_go_to_village::Params { speed: 0.5, h: 15, v: 7, close: 4 }, needs: &[], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 2, params: &(), needs: &[], duration: Some((20, 40)) },
];
/// The pre-raid `TriggerGate` (run_one: the first kid that starts): the meeting point (weight 6) or a stroll (weight 2).
static PRE_RAID_KIDS: &[Kid] = &[
    Kid { behaviour: &reach_block::BEHAVIOUR, weight: 6, params: &reach_block::Params { memory: &MEETING_POINT, speed: 0.75, range: 2.0, too_far: 150, too_long: 200 }, needs: &[(&MEETING_POINT, Need::Present)], duration: None },
    Kid { behaviour: &village_stroll::BEHAVIOUR, weight: 2, params: &village_stroll::Params { speed: 0.75, xz: 10, y: 7 }, needs: &[], duration: None },
];
/// The victory's trigger gate: a sky-seeing spot (weight 5) or a stroll (weight 2).
static RAID_WIN_KIDS: &[Kid] = &[
    Kid { behaviour: &sky_spot::BEHAVIOUR, weight: 5, params: &sky_spot::Params { tries: 10, speed: 0.5 }, needs: &[], duration: None },
    Kid { behaviour: &village_stroll::BEHAVIOUR, weight: 2, params: &village_stroll::Params { speed: 0.55, xz: 10, y: 7 }, needs: &[], duration: None },
];

/// Inactivité : flâner ou ne rien faire, au hasard, poids 1 chacun (`villager_brain.rs` ~4151).
static IDLE_KIDS: &[Kid] = &[
    Kid { behaviour: &stroll::BEHAVIOUR, weight: 1, params: &LAND_STROLL, needs: &[], duration: None },
    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 1, params: &WALK_LOOK, needs: &[(&LOOK_TARGET, Need::Present)], duration: None },
    Kid { behaviour: &jump_on_bed::BEHAVIOUR, weight: 1, params: &BABY_JUMP, needs: &[(&NEAREST_BED, Need::Present)], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
    // lot 6
    Kid { behaviour: &villager_interact::BEHAVIOUR, weight: 2, params: &INTERACT_VILLAGER, needs: &[], duration: None },
    Kid { behaviour: &villager_interact::BEHAVIOUR, weight: 1, params: &INTERACT_BREED, needs: &[], duration: None },
    Kid { behaviour: &villager_interact::BEHAVIOUR, weight: 1, params: &INTERACT_CAT, needs: &[], duration: None },
];

/// The baby's play (`RunOne` of `VillagerGoalPackages.getPlayPackage`, gated by `VisibleBabies`; the interactions are not ported).
static PLAY_KIDS: &[Kid] = &[
    Kid { behaviour: &stroll::BEHAVIOUR, weight: 1, params: &LAND_STROLL, needs: &[], duration: None },
    Kid { behaviour: &walk_to_look_target::BEHAVIOUR, weight: 1, params: &WALK_LOOK, needs: &[(&LOOK_TARGET, Need::Present)], duration: None },
    Kid { behaviour: &jump_on_bed::BEHAVIOUR, weight: 2, params: &BABY_JUMP, needs: &[(&NEAREST_BED, Need::Present)], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 2, params: &(), needs: &[], duration: Some((20, 40)) },
    // lot 6
    Kid { behaviour: &villager_interact::BEHAVIOUR, weight: 2, params: &INTERACT_VILLAGER, needs: &[], duration: None },
    Kid { behaviour: &villager_interact::BEHAVIOUR, weight: 1, params: &INTERACT_CAT, needs: &[], duration: None },
];
const PLAY_TAG: play_tag::Params = play_tag::Params { friends: &nearby_babies::NEARBY_BABIES };
const VICTORY: victory_celebrate::Params = victory_celebrate::Params { duration: 600, firework_every: 200 };
// lot 6 : interactions (`InteractWith`, `villager_brain.rs:1963`), regards (`getMinimalLookBehavior` / `getFullLookBehavior`, `:1135`)
const INTERACT_VILLAGER: villager_interact::Params = villager_interact::Params { species: "minecraft:villager", in_love: false, breed: false };
const INTERACT_BREED: villager_interact::Params = villager_interact::Params { species: "minecraft:villager", in_love: true, breed: true };
const INTERACT_CAT: villager_interact::Params = villager_interact::Params { species: "minecraft:cat", in_love: false, breed: false };
const LOOK_VILLAGER: look_at_entity::Params = look_at_entity::Params { species: &["minecraft:villager"], players: false };
const LOOK_CAT: look_at_entity::Params = look_at_entity::Params { species: &["minecraft:cat"], players: false };
const LOOK_PLAYER: look_at_entity::Params = look_at_entity::Params { species: &[], players: true };
/// `getFullLookBehavior` without its creature categories (the entity info carries no category): cat 8, villager 2, player 2, nothing 2.
static LOOK_FULL_KIDS: &[Kid] = &[
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 8, params: &LOOK_CAT, needs: &[], duration: None },
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 2, params: &LOOK_VILLAGER, needs: &[], duration: None },
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 2, params: &LOOK_PLAYER, needs: &[], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 2, params: &(), needs: &[], duration: Some((30, 60)) },
];
/// `getMinimalLookBehavior`: villager 2, player 2, nothing 8.
static LOOK_MINIMAL_KIDS: &[Kid] = &[
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 2, params: &LOOK_VILLAGER, needs: &[], duration: None },
    Kid { behaviour: &look_at_entity::BEHAVIOUR, weight: 2, params: &LOOK_PLAYER, needs: &[], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 8, params: &(), needs: &[], duration: Some((30, 60)) },
];

static CORE: &[BEntry] = &[
    BEntry { behaviour: &interact_with_door::BEHAVIOUR, activity: "core", priority: 0, params: &interact_with_door::Params::DEFAULT, needs: &[], duration: None, kids: &[] },
    // lot V4 : `LookAtTargetSink` à la priorité 0 du Mob (`villager_brain.rs:4042`)
    BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 0,params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
    BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((150, 250)), kids: &[] },
    BEntry { behaviour: &walk_away::BEHAVIOUR, activity: "panic", priority: 1, params: &walk_away::Params { speed: 0.75, desired: 6.0, memory: &HURT_BY_ENTITY }, needs: &[(&HURT_BY_ENTITY, Need::Present)], duration: None, kids: &[] },
    BEntry { behaviour: &panic::BEHAVIOUR, activity: "panic", priority: 1, params: &panic::Params { speed: 0.75, threat: Some(&HURT_BY_ENTITY) }, needs: &[], duration: Some((60, 60)), kids: &[] },
    BEntry { behaviour: &sleep_in_bed::BEHAVIOUR, activity: "rest", priority: 0, params: &sleep_in_bed::Params::DEFAULT, needs: &[(&NEAREST_BED, Need::Present)], duration: None, kids: &[] },
    BEntry { behaviour: &run_one::BEHAVIOUR, activity: "idle", priority: 2, params: &(), needs: &[], duration: None, kids: IDLE_KIDS },
    BEntry { behaviour: &look_at_player_sometimes::BEHAVIOUR, activity: "idle", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Absent), (&VISIBLE_MOBS, Need::Present)], duration: None, kids: &[] },
];

/// The profession of a farmer, in configuration: the crops it harvests (full grown: the `age` of the crop at its largest) and what it leaves
/// (it carries no seeds: the crop is broken; a villager with seeds would replant it at age 0).
const CROPS: &[(&str, i32)] = &[("minecraft:wheat", 7), ("minecraft:carrots", 7), ("minecraft:potatoes", 7), ("minecraft:beetroots", 3)];
fn crop_ripe(mw: &'static horizon_client_lib::mobs::MobWorld<'static>, p: [i32; 3]) -> bool {
    let name = mw.block_name(p[0], p[1], p[2]);
    CROPS.iter().any(|&(n, max)| n == name && crate::blocks::int_property(mw, p, "age") == Some(max))
}
fn crop_leaves(mw: &'static horizon_client_lib::mobs::MobWorld<'static>, _: [i32; 3]) -> Option<horizon_world::registry::BlockStateId> {
    crate::blocks::resolve(mw, "minecraft:air", &[])
}
// lot V4 : `UseBonemeal.validPos` (a crop below its max age) et la pousse de la poudre (2 à 5 ages, tirés par le module).
const AGE_NAMES: [&str; 8] = ["0", "1", "2", "3", "4", "5", "6", "7"];
fn crop_growable(mw: &'static horizon_client_lib::mobs::MobWorld<'static>, p: [i32; 3]) -> bool {
    let name = mw.block_name(p[0], p[1], p[2]);
    CROPS.iter().any(|&(n, max)| n == name && crate::blocks::int_property(mw, p, "age").is_some_and(|a| a < max))
}
fn crop_grow(mw: &'static horizon_client_lib::mobs::MobWorld<'static>, p: [i32; 3], ages: i32) -> Option<horizon_world::registry::BlockStateId> {
    let name = mw.block_name(p[0], p[1], p[2]);
    let age = crate::blocks::int_property(mw, p, "age")?;
    let max = CROPS.iter().find(|&&(n, _)| n == name)?.1;
    let next = (age + ages).min(max);
    crate::blocks::resolve(mw, name, &[("age", AGE_NAMES[next as usize])])
}
const BONEMEAL: bonemeal::Params = bonemeal::Params { growable: crop_growable, grow: crop_grow };
// lot V2 : le cadeau du héros (`villager.rs:480-495`) : bébé, puis le métier (`unemployed` sans métier)
fn gift_table(w: &World, e: Entity) -> String {
    if w.get::<crate::cow::GrowAt>(e).is_some() {
        return "minecraft:gameplay/hero_of_the_village/baby_gift".to_string();
    }
    let profession = w.get::<Merchant>(e).map_or(0, |m| m.profession);
    let name = match trade_offers::profession_name(w, profession).as_str() {
        "none" | "nitwit" | "" => "unemployed".to_string(),
        other => other.to_string(),
    };
    format!("minecraft:gameplay/hero_of_the_village/{name}_gift")
}
const GIFT: social_gift_hero::Params = social_gift_hero::Params { table: gift_table };
const SOCIALIZE: social_socialize_bell::Params = social_socialize_bell::Params { bell: &MEETING_POINT };
/// The `trigger_gate` of MEET (`villager_brain.rs:4084`): `SocializeAtBell` weight 2 (the stroll is the `meet` entry of this file).
static SOCIAL_KIDS: &[Kid] = &[Kid { behaviour: &social_socialize_bell::BEHAVIOUR, weight: 2, params: &SOCIALIZE, needs: &[(&MEETING_POINT, Need::Present)], duration: None }];
// lot V1 : prise et perte sans effet (le métier naît à `AssignProfession`, il part à `ResetProfession`, `villager_jobs.rs:189`)
fn no_acquired(_: &mut World, _: Entity, _: [i32; 3]) {}
fn no_lost(_: &mut World, _: Entity) {}
fn is_home(t: PoiType) -> bool {
    t == PoiType::Home
}
fn is_meeting(t: PoiType) -> bool {
    t == PoiType::Meeting
}
/// Le métier donné par un point de travail (`villager_professions.rs`, `job` : POI vanilla → nom du métier).
static JOB_PROFESSIONS: &[(PoiType, &str)] = &[
    (PoiType::Armorer, "armorer"),
    (PoiType::Butcher, "butcher"),
    (PoiType::Cartographer, "cartographer"),
    (PoiType::Cleric, "cleric"),
    (PoiType::Farmer, "farmer"),
    (PoiType::Fisherman, "fisherman"),
    (PoiType::Fletcher, "fletcher"),
    (PoiType::Leatherworker, "leatherworker"),
    (PoiType::Librarian, "librarian"),
    (PoiType::Mason, "mason"),
    (PoiType::Shepherd, "shepherd"),
    (PoiType::Toolsmith, "toolsmith"),
    (PoiType::Weaponsmith, "weaponsmith"),
];
const CLAIM_POTENTIAL: poi_claim::Params = poi_claim::Params { memory: &POTENTIAL_JOB_SITE, accept: PoiType::is_job, range: 48, filter: poi_claim::Filter::OwnJob, on_acquired: no_acquired, on_lost: no_lost };
const CLAIM_HOME: poi_claim::Params = poi_claim::Params { memory: &HOME, accept: is_home, range: 48, filter: poi_claim::Filter::FreeBed, on_acquired: no_acquired, on_lost: no_lost };
const CLAIM_MEETING: poi_claim::Params = poi_claim::Params { memory: &MEETING_POINT, accept: is_meeting, range: 48, filter: poi_claim::Filter::Any, on_acquired: no_acquired, on_lost: no_lost };
const VALIDATE_JOB: poi_validate::Params = poi_validate::Params { memory: &JOB_SITE, accept: PoiType::is_job, held: true, bed: false };
const VALIDATE_POTENTIAL: poi_validate::Params = poi_validate::Params { memory: &POTENTIAL_JOB_SITE, accept: PoiType::is_job, held: false, bed: false };
const VALIDATE_HOME: poi_validate::Params = poi_validate::Params { memory: &HOME, accept: is_home, held: false, bed: true };
const VALIDATE_MEETING: poi_validate::Params = poi_validate::Params { memory: &MEETING_POINT, accept: is_meeting, held: false, bed: false };
const COMPETE: poi_competitor_scan::Params = poi_competitor_scan::Params { job: &JOB_SITE };
const GO_TO_SITE: poi_go_to_site::Params = poi_go_to_site::Params { potential: &POTENTIAL_JOB_SITE, speed: 0.5 };
const YIELD_SITE: poi_yield_site::Params = poi_yield_site::Params { job: &JOB_SITE, potential: &POTENTIAL_JOB_SITE, baby: panda_state::is_baby };
const ASSIGN_SITE: poi_assign::Params = poi_assign::Params { job: &JOB_SITE, potential: &POTENTIAL_JOB_SITE, professions: JOB_PROFESSIONS };
const ACQUIRE_POTENTIAL: poi_acquire::Params = poi_acquire::Params { claim: &CLAIM_POTENTIAL, adult: true, event: false, baby: panda_state::is_baby };
const ACQUIRE_HOME: poi_acquire::Params = poi_acquire::Params { claim: &CLAIM_HOME, adult: false, event: true, baby: panda_state::is_baby };
const ACQUIRE_MEETING: poi_acquire::Params = poi_acquire::Params { claim: &CLAIM_MEETING, adult: true, event: true, baby: panda_state::is_baby };

/// The work `RunOne` of `VillagerGoalPackages.getWorkPackage` for a farmer (weights 7, 2, 5, 5, 2).
static WORK_KIDS: &[Kid] = &[
    // lot V4 : `WorkAtPoi` (7 pour tous) ; le composteur est celui du fermier (`farmer`, ici le poste secondaire)
    Kid { behaviour: &composter::BEHAVIOUR, weight: 7, params: &composter::Params { composter: true }, needs: &[(&JOB_SITE, Need::Present), (&SECONDARY_JOB_SITE, Need::Present)], duration: None },
    Kid { behaviour: &composter::BEHAVIOUR, weight: 7, params: &composter::Params { composter: false }, needs: &[(&JOB_SITE, Need::Present), (&SECONDARY_JOB_SITE, Need::Absent)], duration: None },
    // lot V4 : `UseBonemeal` (4 fermier, 7 autres)
    Kid { behaviour: &bonemeal::BEHAVIOUR, weight: 4, params: &BONEMEAL, needs: &[(&SECONDARY_JOB_SITE, Need::Present)], duration: Some((60, 60)) },
    Kid { behaviour: &bonemeal::BEHAVIOUR, weight: 7, params: &BONEMEAL, needs: &[(&SECONDARY_JOB_SITE, Need::Absent)], duration: Some((60, 60)) },
    Kid { behaviour: &stroll_near_block::BEHAVIOUR, weight: 2, params: &stroll_near_block::Params { memory: &JOB_SITE, speed: 0.4, max_dist: 4.0 }, needs: &[(&JOB_SITE, Need::Present)], duration: None },
    Kid { behaviour: &poi_strolls::TO_POI, weight: 5, params: &(), needs: &[(&JOB_SITE, Need::Present)], duration: None },
    Kid { behaviour: &poi_strolls::TO_POI_LIST, weight: 5, params: &(), needs: &[(&JOB_SITE, Need::Present)], duration: None },
    Kid { behaviour: &crop_harvest::BEHAVIOUR, weight: 2, params: &crop_harvest::Params { ripe: crop_ripe, replant: crop_leaves }, needs: &[(&JOB_SITE, Need::Present), (&SECONDARY_JOB_SITE, Need::Present)], duration: None },
];

pub static DEF: Def = Def {
    name: "minecraft:villager",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    brain: BrainDef {
        sensors: &[SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&hurt_by::SENSOR, 20), SensorEntry::new(&secondary_job_site::SENSOR, 40), SensorEntry::new(&nearby_babies::SENSOR, 20).with(&nearby_babies::Params::SAME), SensorEntry::new(&nearest_bed::SENSOR, 20).with(&nearest_bed::Params { nearest: &NEAREST_BED, candidate: &nearest_bed::BED_CANDIDATE, follow_range: 48.0 }), SensorEntry::new(&village_hostile::SENSOR, 20).with(&HOSTILES)],
        activities: ACTIVITIES,
        pick: PICK,
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &schedule::BEHAVIOUR, activity: "core", priority: 0, params: &schedule::Params { adult: ADULT_SLICES, baby: BABY_SLICES }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &reach_block::BEHAVIOUR, activity: "work", priority: 0, params: &reach_block::Params { memory: &JOB_SITE, speed: 0.5, range: 9.0, too_far: 100, too_long: 1200 }, needs: &[(&JOB_SITE, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "work", priority: 5, params: &(), needs: &[], duration: None, kids: WORK_KIDS },
            BEntry { behaviour: &reach_block::BEHAVIOUR, activity: "meet", priority: 0, params: &reach_block::Params { memory: &MEETING_POINT, speed: 0.5, range: 6.0, too_far: 100, too_long: 200 }, needs: &[(&MEETING_POINT, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &stroll_near_block::BEHAVIOUR, activity: "meet", priority: 1, params: &stroll_near_block::Params { memory: &MEETING_POINT, speed: 0.4, max_dist: 40.0 }, needs: &[(&MEETING_POINT, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "play", priority: 0, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: Some((80, 120)), kids: &[] },
            BEntry { behaviour: &play_tag::BEHAVIOUR, activity: "play", priority: 5, params: &PLAY_TAG, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "play", priority: 5, params: &(), needs: &[(&nearby_babies::NEARBY_BABIES, Need::Absent)], duration: None, kids: PLAY_KIDS },
            BEntry { behaviour: &ring_bell::BEHAVIOUR, activity: "pre_raid", priority: 0, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &victory_celebrate::BEHAVIOUR, activity: "raid", priority: 0, params: &VICTORY, needs: &[(&raid::RAID_STATUS, Need::Present)], duration: None, kids: &[] },
            CORE[0], CORE[1], CORE[2], CORE[3], CORE[4], CORE[5], CORE[6],
            BEntry { behaviour: &gossip::DECAY, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &summon_defender::MEMORY, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            // lot V4 : `Swim(60,60)` du cœur (prio 0, `villager_brain.rs:4032`), `MakeLove` ordonné de l'inactivité (prio 3, `:4161`)
            BEntry { behaviour: &work_swim::BEHAVIOUR, activity: "core", priority: 0, params: &(), needs: &[], duration: Some((60, 60)), kids: &[] },
            BEntry { behaviour: &work_wake_up::BEHAVIOUR, activity: "core", priority: 0, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &breed_love::BIRTH, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &breed_love::MAKE_LOVE, activity: "idle", priority: 3, params: &(), needs: &[(&BREED_TARGET, Need::Present)], duration: Some((350, 350)), kids: &[] },
            // lot 6 : regards, interactions et calme (un `SetLookAndInteract` par activité du Mob, à sa priorité)
            BEntry { behaviour: &look_and_interact::BEHAVIOUR, activity: "work", priority: 10, params: &(), needs: &[(&INTERACTION_TARGET, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &look_and_interact::BEHAVIOUR, activity: "meet", priority: 10, params: &(), needs: &[(&INTERACTION_TARGET, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &look_and_interact::BEHAVIOUR, activity: "idle", priority: 3, params: &(), needs: &[(&INTERACTION_TARGET, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "play", priority: 5, params: &(), needs: &[], duration: None, kids: LOOK_FULL_KIDS },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "meet", priority: 5, params: &(), needs: &[], duration: None, kids: LOOK_FULL_KIDS },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "idle", priority: 5, params: &(), needs: &[], duration: None, kids: LOOK_FULL_KIDS },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "panic", priority: 5, params: &(), needs: &[], duration: None, kids: LOOK_MINIMAL_KIDS },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "pre_raid", priority: 5, params: &(), needs: &[], duration: None, kids: LOOK_MINIMAL_KIDS },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "raid", priority: 5, params: &(), needs: &[], duration: None, kids: LOOK_MINIMAL_KIDS },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "hide", priority: 5, params: &(), needs: &[], duration: None, kids: LOOK_MINIMAL_KIDS },
            BEntry { behaviour: &calm_down::BEHAVIOUR, activity: "panic", priority: 0, params: &calm_down::Params { range_sqr: 36.0 }, needs: &[], duration: None, kids: &[] },
            // lot V1 : points, aux priorités et activités du `Mob` (`villager_brain.rs:4046-4071`, `:4097`, `:4104`)
            BEntry { behaviour: &poi_validate::BEHAVIOUR, activity: "core", priority: 0, params: &VALIDATE_JOB, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &poi_validate::BEHAVIOUR, activity: "core", priority: 0, params: &VALIDATE_POTENTIAL, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &poi_competitor_scan::BEHAVIOUR, activity: "core", priority: 2, params: &COMPETE, needs: &[(&JOB_SITE, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &poi_acquire::BEHAVIOUR, activity: "core", priority: 6, params: &ACQUIRE_POTENTIAL, needs: &[(&JOB_SITE, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &poi_go_to_site::BEHAVIOUR, activity: "core", priority: 7, params: &GO_TO_SITE, needs: &[(&POTENTIAL_JOB_SITE, Need::Present)], duration: Some((1200, 1200)), kids: &[] },
            BEntry { behaviour: &poi_yield_site::BEHAVIOUR, activity: "core", priority: 8, params: &YIELD_SITE, needs: &[(&POTENTIAL_JOB_SITE, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &poi_acquire::BEHAVIOUR, activity: "core", priority: 10, params: &ACQUIRE_HOME, needs: &[(&HOME, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &poi_acquire::BEHAVIOUR, activity: "core", priority: 10, params: &ACQUIRE_MEETING, needs: &[(&MEETING_POINT, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &poi_assign::ASSIGN, activity: "core", priority: 10, params: &ASSIGN_SITE, needs: &[(&POTENTIAL_JOB_SITE, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &poi_assign::RESET, activity: "core", priority: 10, params: &ASSIGN_SITE, needs: &[(&JOB_SITE, Need::Absent)], duration: None, kids: &[] },
            BEntry { behaviour: &poi_validate::BEHAVIOUR, activity: "meet", priority: 3, params: &VALIDATE_MEETING, needs: &[(&MEETING_POINT, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &poi_validate::BEHAVIOUR, activity: "rest", priority: 3, params: &VALIDATE_HOME, needs: &[(&HOME, Need::Present)], duration: None, kids: &[] },
            // lot V3 : la promenade de la panique est liée aux villages (`VillageBoundStroll`, remplace le `RandomStroll` approximatif)
            BEntry { behaviour: &village_stroll::BEHAVIOUR, activity: "panic", priority: 3, params: &village_stroll::Params { speed: 0.75, xz: 2, y: 2 }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "rest", priority: 5, params: &(), needs: &[(&NEAREST_BED, Need::Absent)], duration: None, kids: HOMELESS_KIDS },
            BEntry { behaviour: &walk_away::BEHAVIOUR, activity: "panic", priority: 1, params: &walk_away::Params { speed: 0.75, desired: 6.0, memory: &village_hostile::NEAREST_HOSTILE }, needs: &[(&village_hostile::NEAREST_HOSTILE, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "pre_raid", priority: 0, params: &(), needs: &[], duration: None, kids: PRE_RAID_KIDS },
            BEntry { behaviour: &hide::HIDDEN_STATE, activity: "hide", priority: 0, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &hide::BEHAVIOUR, activity: "hide", priority: 1, params: &hide::Params { homes: village_home_walk::homes, radius: 32, speed: 0.625, close: 2, home: &HOME }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &hide::REACT_TO_BELL, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &raid_status::SET, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &raid_status::RESET, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "raid", priority: 1, params: &(), needs: &[], duration: None, kids: RAID_WIN_KIDS },
            // lot V2 : commerce et vie sociale (`FollowTradingPlayer` core 3 ; `GoToWantedItem` core 5 ; `ShowTrades` work 10, meet 10, idle 3 ;
            // `trade_gate` = `TradeWithVillager` meet 3, idle 3 ; `RestockIfDue` dans `WorkAtPoi` (run_one work 5) ; `SocializeAtBell` dans un trigger_gate meet 2 ;
            // `GiveGiftToHero` work, meet, idle 3)
            BEntry { behaviour: &trade_follow_player::BEHAVIOUR, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &trade_go_to_wanted::BEHAVIOUR, activity: "core", priority: 5, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &trade_show_offers::BEHAVIOUR, activity: "work", priority: 10, params: &(), needs: &[], duration: Some((400, 1600)), kids: &[] },
            BEntry { behaviour: &trade_show_offers::BEHAVIOUR, activity: "meet", priority: 10, params: &(), needs: &[], duration: Some((400, 1600)), kids: &[] },
            BEntry { behaviour: &trade_show_offers::BEHAVIOUR, activity: "idle", priority: 3, params: &(), needs: &[], duration: Some((400, 1600)), kids: &[] },
            BEntry { behaviour: &trade_restock_work::BEHAVIOUR, activity: "work", priority: 5, params: &(), needs: &[(&JOB_SITE, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &trade_with_villager::BEHAVIOUR, activity: "meet", priority: 3, params: &(), needs: &[(&INTERACTION_TARGET, Need::Present)], duration: Some((60, 60)), kids: &[] },
            BEntry { behaviour: &trade_with_villager::BEHAVIOUR, activity: "idle", priority: 3, params: &(), needs: &[(&INTERACTION_TARGET, Need::Present)], duration: Some((60, 60)), kids: &[] },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "meet", priority: 2, params: &(), needs: &[(&MEETING_POINT, Need::Present)], duration: None, kids: SOCIAL_KIDS },
            BEntry { behaviour: &social_gift_hero::BEHAVIOUR, activity: "work", priority: 3, params: &GIFT, needs: &[], duration: Some((100, 100)), kids: &[] },
            BEntry { behaviour: &social_gift_hero::BEHAVIOUR, activity: "meet", priority: 3, params: &GIFT, needs: &[], duration: Some((100, 100)), kids: &[] },
            BEntry { behaviour: &social_gift_hero::BEHAVIOUR, activity: "idle", priority: 3, params: &GIFT, needs: &[], duration: Some((100, 100)), kids: &[] },
        ],
    },
    ..Def::EMPTY
};
