//! Lot 16c, the riders of the old Mob (`riders.rs`) that the creature mounts carry: the flags a steering creature holds on its
//! vehicle (none under NoAI), a shulker lid goes DOWN on boarding, a seat turns with its mount, a ranged rider keeps its distance.
//! Expected values are the old Mob's tests and vanilla 26.3 (`Mob.updateControlFlags`, `Shulker.startRiding`, `Entity.positionRider`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::attached_face::Attached;
use horizon_ecs_proto::behaviours::random_stand::{Standing, StandAnim};
use horizon_ecs_proto::cow::{spawn_species, Herd, Pos, SpeciesTable, Yaw};
use horizon_ecs_proto::goals::{GoalSet, Online, JUMP, LOOK, MOVE};
use horizon_ecs_proto::mount::{self, Mount, PassengerSeats};
use horizon_ecs_proto::targeting::Who;
use horizon_ecs_proto::type_tags;
use horizon_ecs_proto::rules::attachments::{attachment, camel_attachment, rear_slide, rider_offset, scaled, seat};
use horizon_ecs_proto::species::attachments::{passenger_seats, rider_seat};
use std::borrow::Cow;

fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// `Mob.updateControlFlags` with `getControllingPassenger`: a creature that steers its vehicle holds the vehicle's move, look and
/// jump goals until it gets off; a vehicle under NoAI is steered by nobody, so its goals stay as they were.
#[test]
fn a_steering_creature_holds_its_vehicles_control_flags_until_it_gets_off() {
    if !have_data() || wrong_species(&["minecraft:spider"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let spider = spawn(&mut h, "minecraft:spider", 0.5, 0.5);
    let skeleton = spawn(&mut h, "minecraft:skeleton", 8.5, 0.5);
    assert!(mount::ride(&mut h.world, spider, Who::Entity(skeleton)));
    assert_eq!(h.world.get::<GoalSet>(spider).unwrap().disabled(), MOVE | LOOK | JUMP);
    mount::dismount(&mut h.world, spider);
    assert_eq!(h.world.get::<GoalSet>(spider).unwrap().disabled(), 0);

    let idle = spawn(&mut h, "minecraft:spider", 4.5, 0.5);
    h.world.entity_mut(idle).insert(horizon_ecs_proto::dragon::flight::NoAi);
    let husk = spawn(&mut h, "minecraft:husk", 8.5, 4.5);
    assert!(mount::ride(&mut h.world, idle, Who::Entity(husk)));
    assert_eq!(h.world.get::<GoalSet>(idle).unwrap().disabled(), 0, "NoAI: the vehicle keeps its goals");
}

/// `Shulker.startRiding`: a shulker that boards lies lid up whatever face it was on, and one whose board is refused lies lid up too.
#[test]
fn a_shulker_that_boards_sits_lid_up() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let horse = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let shulker = spawn(&mut h, "minecraft:shulker", 8.5, 0.5);
    h.world.entity_mut(shulker).insert(Attached { face: 5, color: 0, raw: 0, eased: 0.0 });
    assert!(mount::ride(&mut h.world, horse, Who::Entity(shulker)), "the shulker boards");
    assert_eq!(h.world.get::<Attached>(shulker).unwrap().face, 0, "riding lid up");

    let full = spawn(&mut h, "minecraft:horse", 4.5, 0.5);
    let cow = spawn(&mut h, "minecraft:cow", 4.5, 4.5);
    let refused = spawn(&mut h, "minecraft:shulker", 8.5, 4.5);
    h.world.entity_mut(refused).insert(Attached { face: 3, color: 0, raw: 0, eased: 0.0 });
    assert!(mount::ride(&mut h.world, full, Who::Entity(cow)));
    assert!(!mount::ride(&mut h.world, full, Who::Entity(refused)), "the one seat is taken");
    assert_eq!(h.world.get::<Attached>(refused).unwrap().face, 0, "refused, still lid up");
}

/// `Entity.positionRider`: a seat sits its offset along the mount's facing: a seat a sixteenth of a block behind the centre lies
/// to the east of a mount facing west (yaw 90).
#[test]
fn a_seat_turns_with_its_mount() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let horse = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let rider = spawn(&mut h, "minecraft:cow", 8.5, 0.5);
    h.world.entity_mut(horse).insert(PassengerSeats { adult: Cow::Borrowed(&[[0.0, 0.0, -0.0625]]), baby: None });
    h.world.get_mut::<Yaw>(horse).unwrap().0 = 90.0;
    assert!(mount::ride(&mut h.world, horse, Who::Entity(rider)));
    h.tick();
    let (m, r) = (h.world.get::<Pos>(horse).unwrap().0, h.world.get::<Pos>(rider).unwrap().0);
    assert!((r.x - m.x - 0.0625).abs() < 1e-6, "east of a mount facing west");
    assert!((r.z - m.z).abs() < 1e-6);
}

/// `Mob.getControllingPassenger` of a shooter: a ranged rider keeps its distance from its target (`rider_ranged`), a melee one closes in.
#[test]
fn a_ranged_rider_keeps_its_distance_and_a_melee_one_does_not() {
    if !have_data() || wrong_species(&["minecraft:spider"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let spider = spawn(&mut h, "minecraft:spider", 0.5, 0.5);
    let skeleton = spawn(&mut h, "minecraft:skeleton", 8.5, 0.5);
    let zombie = spawn(&mut h, "minecraft:zombie", 8.5, 4.5);
    assert!(mount::ride(&mut h.world, spider, Who::Entity(skeleton)));
    assert!(h.world.get::<Mount>(spider).unwrap().rider_ranged);
    mount::dismount(&mut h.world, spider);
    assert!(mount::ride(&mut h.world, spider, Who::Entity(zombie)));
    assert!(!h.world.get::<Mount>(spider).unwrap().rider_ranged);
}

/// `Entity.positionRider` with `Vec3.yRot(-yaw)`: the ravager's seat lies a sixteenth of a block behind its centre, and turns with the
/// ravager: to the east of a mount facing west (yaw 90), not behind it.
#[test]
fn attachments_turn_with_the_vehicle() {
    let at = horizon_core::geometry::Vec3::new(0.0, 0.0, 0.0);
    let ravager = [0.0, 2.2625, -0.0625];
    let facing_east = seat(ravager, 0.0, at, 0.0);
    assert!(facing_east.x.abs() < 1e-9 && (facing_east.z + 0.0625).abs() < 1e-9);
    let facing_west = seat(ravager, 0.0, at, 90.0);
    assert!((facing_west.x - 0.0625).abs() < 1e-9 && facing_west.z.abs() < 1e-9);
}

/// The attachments are the entity type's floats (vanilla's `float`, widened to double): the pig's seat, and the zombie's rider offset
/// that puts a rider 2,0125 - 0,7 = 1,3125 above the ground of a zombie-sized vehicle.
#[test]
fn seats_are_the_entity_type_floats() {
    let pig = passenger_seats("minecraft:pig").unwrap();
    assert!((pig.adult[0][1] - 0.868749976158142).abs() < 1e-12);
    let zombie = rider_seat("minecraft:zombie").unwrap();
    assert!((zombie.adult - 0.699999988079071).abs() < 1e-12);
    let zombie_seat = passenger_seats("minecraft:zombie").unwrap();
    let at = seat(zombie_seat.adult[0], zombie.adult, horizon_core::geometry::Vec3::new(0.0, 0.0, 0.0), 0.0);
    assert!((at.y - 1.3125000596).abs() < 1e-6);
}

/// `Pig.getPassengerAttachmentPoint`: a baby pig takes its own attachment (0,5), an adult its own (0,86875).
#[test]
fn a_baby_pig_uses_its_own_passenger_attachment() {
    let pig = passenger_seats("minecraft:pig").unwrap();
    assert_eq!(attachment(&pig.adult, pig.baby, true, 0)[1], 0.5);
    assert!((attachment(&pig.adult, pig.baby, false, 0)[1] - 0.868749976158142).abs() < 1e-12);
}

/// `Spider.getVehicleAttachmentPoint`: a spider sits 0,3125 down on a vehicle no wider than itself (a chicken), and not at all on a
/// wide one; a cave spider sits 0,21875 down on a narrow vehicle.
#[test]
fn a_spider_sits_by_its_own_rule_on_a_narrow_vehicle() {
    let spider = rider_seat("minecraft:spider").unwrap();
    assert_eq!(rider_offset(spider, 0.4, 1.4, false, 1.0), 0.3125);
    assert_eq!(rider_offset(spider, 2.0, 1.4, false, 1.0), 0.0);
    let cave = rider_seat("minecraft:cave_spider").unwrap();
    assert_eq!(rider_offset(cave, 0.4, 0.7, false, 1.0), 0.21875);
}

/// `Entity.getDefaultPassengerAttachmentPoint` scales with the box (`EntityAttachments.scale`): a cow at scale 2 has its seat at 2,7375.
#[test]
fn a_scaled_vehicle_seat_stays_scaled() {
    let cow = passenger_seats("minecraft:cow").unwrap();
    let scaled_cow = scaled(cow.adult[0], 2.0);
    assert!((scaled_cow[1] - 2.7375).abs() < 1e-9);
}

/// `Player` rider in a scale: its own seat (0,6) times its scale (1,5 gives 0,9), so a seat at y 55 with no attachment sits at 54,1.
#[test]
fn a_scaled_player_carries_and_sits_in_its_scale() {
    let player = rider_seat("minecraft:player").unwrap();
    let offset = rider_offset(player, 1.0, 0.6, false, 1.5);
    assert!((offset - 0.9).abs() < 1e-9);
    let at = seat([0.0, 0.0, 0.0], offset, horizon_core::geometry::Vec3::new(0.0, 55.0, 0.0), 0.0);
    assert!((at.y - 54.1).abs() < 1e-9);
}

/// `HappyGhast`: its four passenger attachments are four distinct seats at the corners of its back, the first at (0, 4, 1,7), and a
/// passenger past the fourth takes the last one.
#[test]
fn passengers_take_separate_seats() {
    let ghast = passenger_seats("minecraft:happy_ghast").unwrap();
    let seats: Vec<[f64; 3]> = (0..4).map(|i| attachment(&ghast.adult, ghast.baby, false, i)).collect();
    assert_eq!(seats[0], [0.0, 4.0, 1.7]);
    for i in 0..4 {
        for j in i + 1..4 {
            assert_ne!(seats[i], seats[j]);
        }
    }
    assert_eq!(attachment(&ghast.adult, ghast.baby, false, 9), seats[3]);
}

/// A mod's birth attachments (`SpawnSheet.passenger_attachments`, turned into `PassengerSeats` by `apply_sheet`): the second seat is the
/// second attachment, and a seat past the list takes the last one. `apply_sheet` is crate-private, so this checks the seat choice only.
#[test]
fn a_mod_attachment_list_gives_the_seats_of_the_birth() {
    let seats = PassengerSeats { adult: Cow::Owned(vec![[0.0, 1.0, 0.5], [0.0, 1.0, -0.5]]), baby: None };
    assert_eq!(attachment(&seats.adult, seats.baby, false, 1), [0.0, 1.0, -0.5]);
    assert_eq!(attachment(&seats.adult, seats.baby, false, 5), [0.0, 1.0, -0.5]);
}

/// `Camel.getBodyAnchorAnimationYOffset` sitting (`camel.rs:190-192`, `:242-243`): a sitting camel's seats sink by the bottom point, 1,43 -
/// 1,23 = 0,2 at scale 1. The driver is half a block ahead, the second seat 0,7 behind, and an animal 0,2 further (`camel.rs:577-580`).
#[test]
fn a_sitting_camel_sinks_its_seats_and_keeps_its_offsets() {
    let stood = camel_attachment(2.0, false, 0, 1, false, false, None);
    let sat = camel_attachment(2.0, false, 0, 1, false, true, None);
    assert!((sat[1] - stood[1] - 0.2).abs() < 1e-6, "sinks by the bottom point");
    assert!((camel_attachment(2.0, false, 0, 2, false, false, None)[2] - 0.5).abs() < 1e-6);
    assert!((camel_attachment(2.0, false, 1, 2, false, false, None)[2] + 0.7).abs() < 1e-6);
    assert!((camel_attachment(2.0, false, 1, 2, true, false, None)[2] + 0.5).abs() < 1e-6);
    assert!((camel_attachment(2.0, false, 0, 2, true, false, None)[2] - 0.7).abs() < 1e-6);
}

/// `AbstractHorse.animate` and `getPassengerAttachmentPoint` (`equine.rs:150-161`, `:1025-1031`): a rearing horse's stand value rises 0,45 then
/// 0,72 (`v + (1 - v) * 0,4 + 0,05`), and its rider's seat slides back by `rear_offset` of the value the last tick left (`standAnimO`).
#[test]
fn a_rearing_horse_lifts_and_slides_its_rider() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let horse = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let rider = spawn(&mut h, "minecraft:cow", 8.5, 0.5);
    h.world.entity_mut(horse).insert(PassengerSeats { adult: Cow::Borrowed(&[[0.0, 0.0, 0.0]]), baby: None });
    h.world.get_mut::<Yaw>(horse).unwrap().0 = 0.0;
    assert!(mount::ride(&mut h.world, horse, Who::Entity(rider)));
    h.world.entity_mut(horse).insert(Standing { until: u64::MAX });
    h.tick();
    assert!((h.world.get::<StandAnim>(horse).unwrap().now - 0.45).abs() < 1e-6);
    h.tick();
    let a = *h.world.get::<StandAnim>(horse).unwrap();
    assert!((a.now - 0.72).abs() < 1e-6 && (a.last - 0.45).abs() < 1e-6, "{a:?}");
    h.tick();
    // Read order: `animate` runs in `arbitrate`, before `pathfind`; `apply_pushes` (which calls `seats`) runs last in the chain. So the seat
    // of this tick reads `last`, the value the tick before left (0,72), not this tick's `now`.
    let (m, r) = (h.world.get::<Pos>(horse).unwrap().0, h.world.get::<Pos>(rider).unwrap().0);
    let slide = (r.z - m.z, r.y - m.y);
    let expected = rear_slide(0.72, false);
    assert!((slide.0 - expected[2]).abs() < 1e-6 && (slide.1 - expected[1]).abs() < 1e-6, "slide {slide:?}");
}

/// `Mob.getControllingPassenger` (`riders.rs:463`): a rider of a species in the tag `minecraft:non_controlling_rider` holds no control of
/// its vehicle. The tag is declared at run time before the rider is born, so the rider carries the marker the tag gives it.
#[test]
fn a_tagged_rider_leaves_its_vehicles_goals_alone() {
    if !have_data() || wrong_species(&["minecraft:spider"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    // The species the game's own data lists (the table of tags is the process's: a test does not write to it).
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let tagged = table.iter().find(|s| type_tags::lists("minecraft:non_controlling_rider", s.id)).expect("the data's tag names a species").name;
    let spider = spawn(&mut h, "minecraft:spider", 0.5, 0.5);
    let rider = spawn(&mut h, tagged, 8.5, 0.5);
    assert!(h.world.get::<mount::NonControllingRider>(rider).is_some(), "the tag gives the marker");
    assert!(mount::ride(&mut h.world, spider, Who::Entity(rider)));
    assert_eq!(h.world.get::<GoalSet>(spider).unwrap().disabled(), 0);
}

/// `Mob.controlsVehicle` with `getControllingPassenger`: only the first rider steers; a second passenger boarding leaves the flags of
/// the first one as they are.
#[test]
fn only_the_first_passenger_steers_its_vehicle() {
    if !have_data() || wrong_species(&["minecraft:camel"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let camel = spawn(&mut h, "minecraft:camel", 0.5, 0.5);
    let first = spawn(&mut h, "minecraft:skeleton", 8.5, 0.5);
    let second = spawn(&mut h, "minecraft:zombie", 8.5, 4.5);
    assert!(mount::ride(&mut h.world, camel, Who::Entity(first)));
    let steered = h.world.get::<GoalSet>(camel).unwrap().disabled();
    assert!(mount::ride(&mut h.world, camel, Who::Entity(second)), "the second seat takes the second passenger");
    assert_eq!(h.world.get::<GoalSet>(camel).unwrap().disabled(), steered, "the first passenger keeps the steering");
}

/// `Entity.positionRider` for a player aboard: the player's position in `Online` is the horse's attachment (1,44375, `species/attachments.rs:13`)
/// above the horse, less the player's own seat (0,6), at the horse's x and z.
#[test]
fn a_player_aboard_sits_at_the_horse_seat_less_its_own() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let horse = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    h.set_online(&[(0.5, 0.5)]);
    assert!(mount::ride(&mut h.world, horse, Who::Player(1)));
    h.tick();
    let m = h.world.get::<Pos>(horse).unwrap().0;
    let p = h.world.resource::<Online>().get(1).unwrap();
    let horse_seat = 1.44375_f32 as f64;
    assert!((p.y - (m.y + horse_seat - 0.6)).abs() < 1e-6, "y {} from the mount at {}", p.y, m.y);
    assert!((p.x - m.x).abs() < 1e-9 && (p.z - m.z).abs() < 1e-9);
}
