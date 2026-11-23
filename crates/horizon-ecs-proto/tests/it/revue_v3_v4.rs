//! Revue des lots V3 et V4 : l'ECS seul, valeurs absolues (cachette, cultures, naissance, colère, regard, raid). Écrit sans compiler :
//! à vérifier au premier passage.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_ecs_proto::{
    behaviours::{
        bed::Sleeping,
        bell_heard::HEARD_BELL,
        breed_love, food_stock, hide, inventory, raid, schedule::Times, sky_spot,
    },
    brain::*,
    cow::*,
    goals::*,
    targeting::{Target},
};
use horizon_core::geometry::Vec3;

const DAY: i64 = 4000;
const NIGHT: i64 = 13000;

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}
fn skip_warden() -> bool {
    !have_data() || wrong_species(&["minecraft:warden"])
}
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    Times::set(&mut h.world, DAY);
    e
}
fn ecs_pos(h: &Herd, e: Entity) -> Vec3 {
    h.world.get::<Pos>(e).unwrap().0
}
fn roof_ecs(h: &mut Herd) {
    for x in -2..=2 {
        for z in -2..=2 {
            h.set_block(x, 3, z, "minecraft:stone");
        }
    }
    h.tick();
    h.tick();
}
/// Inside the 5x5 roof of `roof_ecs` (its blocks cover [-2, 3) on x and z).
fn under_roof(p: Vec3) -> bool {
    (-2.0..3.0).contains(&p.x) && (-2.0..3.0).contains(&p.z)
}
fn bed_ecs(h: &mut Herd) {
    h.set_block_state(8, 1, 0, "minecraft:white_bed", &[("part", "head"), ("facing", "east")]);
    h.set_block_state(7, 1, 0, "minecraft:white_bed", &[("part", "foot"), ("facing", "east")]);
}
/// Two more beds (z 2 and 4): with the first, one bed per parent and one for the child.
fn extra_beds_ecs(h: &mut Herd) {
    for z in [2, 4] {
        h.set_block_state(8, 1, z, "minecraft:white_bed", &[("part", "head"), ("facing", "east")]);
        h.set_block_state(7, 1, z, "minecraft:white_bed", &[("part", "foot"), ("facing", "east")]);
    }
}
/// A 3x3 field of young wheat (age 0) on farmland, east of the origin: x in 3..=5, z in -1..=1.
fn field_cells() -> Vec<[i32; 3]> {
    (3..=5).flat_map(|x| (-1..=1).map(move |z| [x, 1, z])).collect()
}
fn field_ecs(h: &mut Herd) {
    for p in field_cells() {
        h.set_block(p[0], 0, p[2], "minecraft:farmland");
        h.set_block_state(p[0], p[1], p[2], "minecraft:wheat", &[("age", "0")]);
    }
}
fn wheat_age_ecs(h: &Herd, p: [i32; 3]) -> i32 {
    let mw = horizon_ecs_proto::blocks::world_of(&h.world);
    horizon_ecs_proto::blocks::property(mw, p, "age").and_then(|a| a.parse().ok()).unwrap_or(-1)
}

/// 6. `WakeUp`: the villager sleeps in its bed at night, then the day comes: it leaves the bed within 200 ticks.
#[test]
fn sleeper_wakes_at_the_hour_and_leaves_the_bed_as_a_mob_does() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    bed_ecs(&mut h);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 22);
    Times::set(&mut h.world, NIGHT);
    for _ in 0..800 {
        h.tick();
        if h.world.get::<Sleeping>(e).is_some() {
            break;
        }
    }
    assert!(h.world.get::<Sleeping>(e).is_some(), "ECS : endormi la nuit");
    Times::set(&mut h.world, DAY);
    let mut woke = None;
    for t in 1..=200i64 {
        h.tick();
        if woke.is_none() && h.world.get::<Sleeping>(e).is_none() {
            woke = Some(t);
        }
    }
    assert!(woke.is_some(), "réveil : ECS ne se réveille pas dans 200 tics");
}

/// 2. `ReactToBell` and `LocateHidingPlace` / `SetHiddenState`: hidden from the bell for 300 ticks (tolerance 20).
#[test]
fn bell_hides_the_villager_for_300_ticks_on_both_sides() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    bed_ecs(&mut h);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 23);
    h.tick();
    h.tick();
    let bell_ecs = now_of(&h.world, e) as i64;
    set(&mut h.world, e, &HEARD_BELL, MemValue::Int(bell_ecs), None);
    let mut seen = false;
    let mut end_ecs = None;
    for t in 1..=500i64 {
        h.tick();
        let hidden = present(&h.world, e, &hide::HIDING_PLACE);
        seen |= hidden;
        if seen && !hidden && end_ecs.is_none() {
            end_ecs = Some(t);
        }
    }
    let a = end_ecs.expect("cachette finie : ECS");
    assert!((a - 300).abs() <= 20, "durée cachée (300) : ECS {a}");
}

/// 3. `SetRaidStatus` / `ResetRaidStatus` and the walk inside: a raided villager outside a roof goes under it (the first tick it
/// is under the roof, counted from the choice of the hiding place), and does not come out while the raid is on.
fn raid_run() -> Option<(Option<i64>, Option<i64>, bool)> {
    if skip() {
        return None;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    roof_ecs(&mut h);
    // The jar's raid package goes indoors only through `LocateHidingPlace` (a home POI within 24 blocks): a bed under the roof.
    h.set_block_state(1, 1, 0, "minecraft:white_bed", &[("part", "head"), ("facing", "east")]);
    h.set_block_state(0, 1, 0, "minecraft:white_bed", &[("part", "foot"), ("facing", "east")]);
    let e = spawn(&mut h, "minecraft:villager", 6.5, 0.5, 24);
    h.tick();
    let at = [6.5, 1.0, 0.5];
    raid::set_status(&mut h.world, at, 64.0, raid::RAID_ACTIVE);
    // A villager that strolls keeps its walk target before `LocateHidingPlace` may run (the jar): the entry is counted from the tick the
    // hiding place is chosen, the exit from the end of the `raid` activity, not from the host's call. The walk is in tenths of ticks per block.
    let (mut in_ecs, mut out_ecs) = (None, None);
    let mut on = None;
    let mut x_on = 0.0;
    for t in 1..=400i64 {
        if t == 200 {
            raid::set_status(&mut h.world, at, 64.0, raid::RAID_NONE);
        }
        h.tick();
        if present(&h.world, e, &hide::HIDING_PLACE) && on.is_none() {
            on = Some(t);
            x_on = ecs_pos(&h, e).x;
        }
        let inside = under_roof(ecs_pos(&h, e));
        if t < 200 && in_ecs.is_none() && inside {
            in_ecs = on.map(|o| (t - o) * 100 / ((x_on - 3.0).max(1.0) * 10.0) as i64);
        }
        if t < 200 && in_ecs.is_some() && out_ecs.is_none() && !inside {
            out_ecs = Some(t);
        }
    }
    let over = super::lot_v3_village::ecs_act(&h.world, e) != "raid";
    Some((in_ecs, out_ecs, over))
}

/// Entry under the roof on the ECS; the raid activity ends with the status (`ResetRaidStatus`).
#[test]
fn raided_villager_goes_indoors_at_the_same_tick_as_a_mob() {
    let Some((in_ecs, _, over)) = raid_run() else { return };
    assert!(in_ecs.is_some(), "entrée sous le toit : ECS {in_ecs:?}");
    assert!(over, "le raid est fini");
}

/// The raid package's `MoveToSkySeeingSpot` is behind `raidExistsAndNotVictory`, whose bytecode answers `Raid.isVictory`: while the raid is on
/// (not won) a villager under the roof stays there (the exit after the raid is the idle stroll, a random draw).
#[test]
fn a_raided_villager_stays_under_the_roof_while_the_raid_is_on_as_the_mob_does() {
    let Some((_, out_ecs, _)) = raid_run() else { return };
    assert!(out_ecs.is_none(), "sortie du toit pendant le raid : ECS {out_ecs:?}");
}

/// 4. `MoveToSkySeeingSpot`: from under a roof, the villager reaches a spot out of it (within 300 ticks).
#[test]
fn sky_spot_position_is_reached_as_on_the_mob() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    roof_ecs(&mut h);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 25);
    // A path is made only on the ground (`canUpdatePath`): the spawned villager lands first.
    for _ in 0..4 {
        h.tick();
    }
    erase(&mut h.world, e, &WALK_TARGET);
    static SKY: sky_spot::Params = sky_spot::Params { tries: 10, speed: 0.5 };
    let _ = (sky_spot::BEHAVIOUR.can_start)(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &SKY });
    (sky_spot::BEHAVIOUR.start)(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &SKY });
    let mut ecs = None;
    for t in 1..=300i64 {
        h.tick();
        if ecs.is_none() && !under_roof(ecs_pos(&h, e)) {
            ecs = Some(t);
        }
    }
    assert!(ecs.is_some(), "case ciel : ECS {ecs:?}");
}

/// 5. The farmer and its bone meal: a young wheat field and 20 bone meal, 400 ticks. Every two growths of the villager are at least
/// 40 ticks apart (`UseBonemeal`: 40 ticks of rest after each).
fn farmer_run() -> Option<Vec<i64>> {
    if skip() {
        return None;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    field_ecs(&mut h);
    // The jar puts UseBonemeal in the WORK activity, which needs a job site: a composter beside the field.
    crate::common::insert_trade_data(&mut h);
    h.set_block(2, 1, 0, "minecraft:composter");
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 31);
    inventory::add(&mut h.world, e, "minecraft:bone_meal", 20);
    let cells = field_cells();
    let mut prev: Vec<i32> = cells.iter().map(|&p| wheat_age_ecs(&h, p)).collect();
    let mut ecs_ticks: Vec<i64> = Vec::new();
    for t in 1..=400i64 {
        h.tick();
        let now: Vec<i32> = cells.iter().map(|&p| wheat_age_ecs(&h, p)).collect();
        // The ECS world also random-ticks the wheat (+1 stage); the villager's bone meal gives at least 2 stages.
        ecs_ticks.extend(now.iter().zip(&prev).filter(|(a, b)| **a - **b >= 2).map(|_| t));
        prev = now;
    }
    Some(ecs_ticks)
}

/// What the jar fixes for the ECS farmer: it grows crops with its bone meal, at least 40 ticks apart.
#[test]
fn farmer_grows_crops_with_bone_meal_per_the_jar() {
    let Some(ecs) = farmer_run() else { return };
    assert!(!ecs.is_empty(), "cultures poussées : ECS {}", ecs.len());
    assert!(ecs.windows(2).all(|w| w[1] - w[0] >= 40), "espacement ECS : {ecs:?}");
    assert!(ecs.len() <= 10, "plus de 10 pousses en 400 tics : {ecs:?}");
}

/// 7 ter. Against the jar's facts (`VillagerMakeLove`: birth planned at start + 275 + rand(50), `takeVacantBed` gives the child the nearest free
/// home within 48 blocks): the ECS couple, 12 food points each and free beds, has a baby after 275..325 ticks, and the baby holds a free bed.
#[test]
fn two_villagers_with_food_and_beds_have_a_baby_after_the_jars_delay() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    bed_ecs(&mut h);
    extra_beds_ecs(&mut h);
    let a = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 26);
    let b = spawn(&mut h, "minecraft:villager", 1.5, 0.5, 27);
    for &p in &[a, b] {
        h.world.entity_mut(p).insert(food_stock::Food(12));
    }
    let mut start = now_of(&h.world, a);
    while h.world.get::<breed_love::Mating>(a).is_none() && h.world.get::<breed_love::Mating>(b).is_none() && now_of(&h.world, a) < start + 2000 {
        h.tick();
        start = now_of(&h.world, a);
    }
    let by = if h.world.get::<breed_love::Mating>(a).is_some() { a } else { b };
    let due = h.world.get::<breed_love::Mating>(by).unwrap().birth;
    let mut baby = None;
    while baby.is_none() && now_of(&h.world, a) < due + 50 {
        h.tick();
        baby = h.world.query_filtered::<Entity, With<GrowAt>>().iter(&h.world).next();
    }
    assert!((275..325).contains(&(due - start)), "délai 275 + rand(50) : {}", due - start);
    let baby = baby.expect("naissance");
    assert!(matches!(horizon_ecs_proto::brain::get(&h.world, baby, &horizon_ecs_proto::behaviours::bed::NEAREST_BED), Some(horizon_ecs_proto::brain::MemValue::Block(_))), "le bébé a un lit libre");
}

/// 7 bis. No free bed: the same food, no bed, 2500 ticks: no baby is born.
#[test]
fn villagers_without_a_free_bed_have_no_baby() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let a = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 28);
    let b = spawn(&mut h, "minecraft:villager", 1.5, 0.5, 29);
    for &p in &[a, b] {
        h.world.entity_mut(p).insert(food_stock::Food(12));
    }
    let mut loved = false;
    for _ in 0..2500 {
        h.tick();
        loved |= h.world.get::<breed_love::Mating>(a).is_some() || h.world.get::<breed_love::Mating>(b).is_some();
    }
    assert!(loved, "the pair does start the union (real trigger), the bed alone is missing");
    let ecs_babies = h.world.query_filtered::<Entity, With<GrowAt>>().iter(&h.world).count();
    assert_eq!(ecs_babies, 0, "sans lit libre, pas de bébé (ECS)");
}

/// 8. `StopAttackingIfTargetInvalid` and `SetWardenLookTarget` on the warden: hit by a player at tick 140, it turns its head to them
/// (`head_yaw` at tick 152, within 5 degrees of `look_yaw`); then the target is dead, or in creative, and the warden drops it: the first
/// tick from 150 without a target (tolerance 2 ticks).
/// Returns (ECS drop tick, ECS yaw, expected yaw).
fn warden_drops(dead: bool) -> (Option<i64>, f32, f32) {
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 6.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    let mut yaw_ecs = f32::NAN;
    let mut drop_ecs = None;
    for t in 1..=160i64 {
        if t == 140 {
            h.hit_by(e, 1.0, 1);
        }
        if t == 152 {
            yaw_ecs = h.world.get::<Head>(e).map_or(f32::NAN, |x| x.yaw);
        }
        h.tick();
        if t == 150 {
            let ps = h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().into_inner();
            if dead {
                if ps.health.len() <= 1 {
                    ps.health.resize(2, 20.0);
                }
                ps.health[1] = 0.0;
            } else {
                ps.set_creative(1, true);
            }
        }
        if t >= 150 && drop_ecs.is_none() && h.world.get::<Target>(e).is_none() {
            drop_ecs = Some(t - 150);
        }
    }
    (drop_ecs, yaw_ecs, look_yaw(Vec3::new(0.5, 1.0, 0.5), Vec3::new(6.5, 1.0, 0.5)))
}
fn check_warden(dead: bool) {
    let (de, ye, exp) = warden_drops(dead);
    let gap = |a: f32| ((a - exp + 540.0).rem_euclid(360.0) - 180.0).abs();
    assert!(gap(ye) <= 5.0, "regard vers le joueur (attendu {exp}) : ECS {ye}");
    assert!(de.is_some_and(|d| d <= 2), "cible lâchée : ECS {de:?}");
}
#[test]
fn warden_looks_at_its_attacker_and_drops_a_dead_target_as_a_mob_does() {
    if skip_warden() {
        return;
    }
    check_warden(true);
}
#[test]
fn warden_looks_at_its_attacker_and_drops_a_creative_target_as_a_mob_does() {
    if skip_warden() {
        return;
    }
    check_warden(false);
}
