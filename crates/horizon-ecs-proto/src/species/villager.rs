//! Le villageois : un cerveau (adulte ou bébé selon `GrowAt`), horaire vanilla (`villager.rs:131`). Ordre des activités = index de `Brain.activity`.
//! Hors de ce fichier : capteurs de lit, de bébés, de golem et d'hostiles, poste et cloche (`JOB_SITE`, `MEETING_POINT` à
//! alimenter par la partie POI), le jeu des bébés (`play`), le commerce, la reproduction, la panique du raid (voir villager.md).
use super::Def;
use crate::behaviours::Cap;
use crate::behaviours::{bed::NEAREST_BED, breed_love, composter, crop_harvest, nearest_bed, poi_strolls, gossip, reach_block, schedule, sleep_in_bed, stroll_near_block, summon_defender};
use crate::brain::behaviours::{do_nothing, interact_with_door, look_at_player_sometimes, look_at_target_sink, move_to_target_sink, panic, run_one, stroll, walk_away};
use crate::brain::sensors::{hurt_by, living, secondary_job_site::{self, SECONDARY_JOB_SITE}};
use crate::brain::*;

pub static SCHEDULE_IDLE: MemKey = MemKey::new("minecraft:schedule/idle", Shape::Unit);
pub static SCHEDULE_WORK: MemKey = MemKey::new("minecraft:schedule/work", Shape::Unit);
pub static SCHEDULE_MEET: MemKey = MemKey::new("minecraft:schedule/meet", Shape::Unit);
pub static SCHEDULE_PLAY: MemKey = MemKey::new("minecraft:schedule/play", Shape::Unit);
pub static SCHEDULE_REST: MemKey = MemKey::new("minecraft:schedule/rest", Shape::Unit);
pub static JOB_SITE: MemKey = MemKey::new("minecraft:job_site", Shape::Block);
pub static MEETING_POINT: MemKey = MemKey::new("minecraft:meeting_point", Shape::Block);

static ACTIVITIES: &[ActivityDef] = &[
    ActivityDef { name: "core", slot: 1, requires: &[] },
    ActivityDef { name: "play", slot: 0, requires: &[(&SCHEDULE_PLAY, Need::Present)] },
    ActivityDef { name: "rest", slot: 2, requires: &[(&SCHEDULE_REST, Need::Present)] },
    ActivityDef { name: "hide", slot: 3, requires: &[] },
    ActivityDef { name: "meet", slot: 4, requires: &[(&SCHEDULE_MEET, Need::Present)] },
    ActivityDef { name: "idle", slot: 5, requires: &[(&SCHEDULE_IDLE, Need::Present)] },
    ActivityDef { name: "pre_raid", slot: 6, requires: &[] },
    ActivityDef { name: "work", slot: 7, requires: &[(&SCHEDULE_WORK, Need::Present)] },
    ActivityDef { name: "panic", slot: 8, requires: &[(&HURT_BY, Need::Present)] },
    ActivityDef { name: "raid", slot: 9, requires: &[] },
];
static PICK: &[&str] = &["panic", "rest", "meet", "work", "play", "idle"];

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

/// Inactivité : flâner ou ne rien faire, au hasard, poids 1 chacun (`villager_brain.rs` ~4151).
static IDLE_KIDS: &[Kid] = &[
    Kid { behaviour: &stroll::BEHAVIOUR, weight: 1, params: &stroll::Params { speed: 0.5, h: 10, v: 7, dry_only: false }, needs: &[], duration: None },
    Kid { behaviour: &do_nothing::BEHAVIOUR, weight: 1, params: &(), needs: &[], duration: Some((30, 60)) },
];

static CORE: &[BEntry] = &[
    BEntry { behaviour: &interact_with_door::BEHAVIOUR, activity: "core", priority: 0, params: &interact_with_door::Params::DEFAULT, needs: &[], duration: None, kids: &[] },
    BEntry { behaviour: &look_at_target_sink::BEHAVIOUR, activity: "core", priority: 1, params: &(), needs: &[(&LOOK_TARGET, Need::Present)], duration: Some((45, 90)), kids: &[] },
    BEntry { behaviour: &move_to_target_sink::BEHAVIOUR, activity: "core", priority: 2, params: &(), needs: &[(&PATH, Need::Absent), (&WALK_TARGET, Need::Present)], duration: None, kids: &[] },
    BEntry { behaviour: &walk_away::BEHAVIOUR, activity: "panic", priority: 0, params: &walk_away::Params { speed: 0.75, desired: 6.0, memory: &HURT_BY_ENTITY }, needs: &[(&HURT_BY_ENTITY, Need::Present)], duration: None, kids: &[] },
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

/// The work `RunOne` of `VillagerGoalPackages.getWorkPackage` for a farmer (weights 7, 2, 5, 5, 2).
static WORK_KIDS: &[Kid] = &[
    Kid { behaviour: &composter::BEHAVIOUR, weight: 7, params: &composter::Params { composter: true }, needs: &[(&JOB_SITE, Need::Present)], duration: None },
    Kid { behaviour: &stroll_near_block::BEHAVIOUR, weight: 2, params: &stroll_near_block::Params { memory: &JOB_SITE, speed: 0.4, max_dist: 4.0 }, needs: &[(&JOB_SITE, Need::Present)], duration: None },
    Kid { behaviour: &poi_strolls::TO_POI, weight: 5, params: &(), needs: &[(&JOB_SITE, Need::Present)], duration: None },
    Kid { behaviour: &poi_strolls::TO_POI_LIST, weight: 5, params: &(), needs: &[(&JOB_SITE, Need::Present)], duration: None },
    Kid { behaviour: &crop_harvest::BEHAVIOUR, weight: 2, params: &crop_harvest::Params { ripe: crop_ripe, replant: crop_leaves }, needs: &[(&JOB_SITE, Need::Present), (&SECONDARY_JOB_SITE, Need::Present)], duration: None },
];

pub static DEF: Def = Def {
    name: "minecraft:villager",
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Brain]),
    brain: BrainDef {
        sensors: &[SensorEntry::new(&living::SENSOR, 20), SensorEntry::new(&hurt_by::SENSOR, 20), SensorEntry::new(&secondary_job_site::SENSOR, 40), SensorEntry::new(&nearest_bed::SENSOR, 20).with(&nearest_bed::Params { nearest: &NEAREST_BED, candidate: &nearest_bed::BED_CANDIDATE, follow_range: 48.0 })],
        activities: ACTIVITIES,
        pick: PICK,
        initial: &[],
        behaviours: &[
            BEntry { behaviour: &schedule::BEHAVIOUR, activity: "core", priority: 0, params: &schedule::Params { adult: ADULT_SLICES, baby: BABY_SLICES }, needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &reach_block::BEHAVIOUR, activity: "work", priority: 0, params: &reach_block::Params { memory: &JOB_SITE, speed: 0.5, range: 9.0 }, needs: &[(&JOB_SITE, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &run_one::BEHAVIOUR, activity: "work", priority: 5, params: &(), needs: &[], duration: None, kids: WORK_KIDS },
            BEntry { behaviour: &reach_block::BEHAVIOUR, activity: "meet", priority: 0, params: &reach_block::Params { memory: &MEETING_POINT, speed: 0.5, range: 6.0 }, needs: &[(&MEETING_POINT, Need::Present)], duration: None, kids: &[] },
            BEntry { behaviour: &stroll_near_block::BEHAVIOUR, activity: "meet", priority: 1, params: &stroll_near_block::Params { memory: &MEETING_POINT, speed: 0.4, max_dist: 40.0 }, needs: &[(&MEETING_POINT, Need::Present)], duration: None, kids: &[] },
            CORE[0], CORE[1], CORE[2], CORE[3], CORE[4], CORE[5], CORE[6],
            BEntry { behaviour: &gossip::DECAY, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &summon_defender::MEMORY, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
            BEntry { behaviour: &breed_love::BIRTH, activity: "core", priority: 3, params: &(), needs: &[], duration: None, kids: &[] },
        ],
    },
    ..Def::EMPTY
};
