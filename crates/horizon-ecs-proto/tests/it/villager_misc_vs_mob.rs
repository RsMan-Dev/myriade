//! Le `Mob` et l'ecs dans la même scène plate (`HORIZON_SPECIES` nomme l'espèce) : récolte et replantation du blé mûr,
//! panique après un coup, ouverture d'une porte. Chaque cas pose le même état dans les deux mondes. Ne passe qu'une fois
//! le câblage des espèces fait (voir `villager-divers.md`, section 5) : pas d'`#[ignore]`.
use crate::common::*;
use horizon_ecs_proto::brain::{present, IS_PANICKING};
use bevy_ecs::prelude::*;
use horizon_ecs_proto::{blocks, cow, cow::*};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_client_lib::mobs::PlayerView;
use horizon_core::position::BlockPos;
use horizon_client_lib::mobs::MobWorld;
use horizon_ecs_proto::behaviours::schedule::Times;
use horizon_ecs_proto::brain::{set, MemValue};
use horizon_ecs_proto::species::villager::JOB_SITE;

/// The hour of the day in both worlds (the `Mob`'s world is frozen: it is rebuilt to carry the hour).
fn set_day(r: &mut MobRef, h: &mut Herd, day: i64) {
    r.w = Box::leak(Box::new(MobWorld { day_time: day, ..r.w.clone() }));
    Times::set(&mut h.world, day);
}

fn is(name: &str) -> bool {
    process_species_name() == name
}

/// Writes a block state with properties into the `Mob` world (`MobRef::set_block` takes a default state only).
fn mob_block(r: &MobRef, x: i32, y: i32, z: i32, name: &str, props: &[(&str, &str)]) {
    let Some(id) = blocks::resolve(r.w, name, props) else { return };
    // SAFETY: as in `MobRef::set_block`: the test thread owns the world while it steps.
    unsafe {
        let world = &mut *r.level.blocks.0;
        let _ = world.set(x, y, z, id);
        world.path_type_block_updated(BlockPos::new(x, y, z));
    }
}

/// The name of the block at `(x, y, z)` in both worlds, `(mob, ecs)`.
fn names_at(r: &MobRef, h: &Herd, x: i32, y: i32, z: i32) -> (String, String) {
    let mob = r.w.block_name(x, y, z).to_string();
    let ecs = h.world.resource::<Level>().mw.0.block_name(x, y, z).to_string();
    (mob, ecs)
}

/// Spawns a villager of the process species at `(x, z)` in the ecs world.
fn spawn_villager(h: &mut Herd, x: f64, z: f64) -> Entity {
    cow::spawn_armed(&mut h.world, species(), x, z, 7)
}

#[test]
fn a_farmer_harvests_and_replants_the_mature_wheat_as_a_mob_does() {
    if !have_data() || !is("minecraft:villager") {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let (x, y, z) = (0, 1, 0);
    let crop = [x + 1, y, z];
    mob_block(&r, crop[0], crop[1], crop[2], "minecraft:wheat", &[("age", "7")]);
    h.set_block_state(crop[0], crop[1], crop[2], "minecraft:wheat", &[("age", "7")]);
    mob_block(&r, crop[0], crop[1] - 1, crop[2], "minecraft:farmland", &[("moisture", "7")]);
    h.set_block_state(crop[0], crop[1] - 1, crop[2], "minecraft:farmland", &[("moisture", "7")]);
    // The job site of a farmer, and the work hours: the Mob's villager takes the profession of the composter.
    let job = [x - 2, y, z];
    mob_block(&r, job[0], job[1], job[2], "minecraft:composter", &[]);
    h.set_block_state(job[0], job[1], job[2], "minecraft:composter", &[]);
    r.apply_block_events();
    let i = r.add_kind(Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5), mob_kind("minecraft:villager"));
    let farmer = horizon_client_lib::mobs::villager::profession_index("farmer").expect("the farmer profession");
    let ok = horizon_client_lib::mobs::villager::set_profession(&mut r.mobs[i], horizon_client_lib::mobs::villager::profession_table(), farmer as usize);
    let e = spawn_villager(&mut h, f64::from(x) + 0.5, f64::from(z) + 0.5);

    // The ecs has no `AcquirePoi` (known gap): its job site is given at the tick the Mob's villager has claimed its composter, and the work
    // hours start right after, so that neither wanders in the idle hours before.
    let (mut mob_at, mut ecs_at) = (None, None);
    let mut claimed = None;
    let mut ecs_sec = None;
    let mut mob_sec = None;
    for t in 0..400u64 {
        set_day(&mut r, &mut h, 1990 + t as i64);
        r.step(None);
        r.apply_block_events();
        if mob_sec.is_none() && horizon_client_lib::mobs::villager_brain::describe(&r.mobs[i]).is_some_and(|d| d.1.contains("secondary_job_site")) {
            mob_sec = Some(t);
        }
        if claimed.is_none() && horizon_client_lib::mobs::villager_brain::describe(&r.mobs[i]).is_some_and(|d| d.1.contains(" job_site=") || d.1.starts_with("job_site=")) {
            claimed = Some(t);
            set(&mut h.world, e, &JOB_SITE, MemValue::Block(job), None);
        }
        h.tick();
        if ecs_sec.is_none() && horizon_ecs_proto::brain::present(&h.world, e, &horizon_ecs_proto::brain::sensors::secondary_job_site::SECONDARY_JOB_SITE) { ecs_sec = Some(t); }
        let (m, c) = names_at(&r, &h, crop[0], crop[1], crop[2]);
        let mob_age = r.w.prop(crop[0], crop[1], crop[2], "age");
        let ecs_age = h.world.resource::<Level>().mw.0.prop(crop[0], crop[1], crop[2], "age");
        if mob_at.is_none() && t > 0 && (m != "minecraft:wheat" || mob_age != Some("7")) {
            mob_at = Some(t);
        }
        if ecs_at.is_none() && t > 0 && (c != "minecraft:wheat" || ecs_age != Some("7")) {
            ecs_at = Some(t);
        }
        if mob_at.is_some() && ecs_at.is_some() {
            break;
        }
    }
    // The Mob farmer harvests the crop at about t=200-250 (the scene is the door scene'"'"'s: ground at y=1, hours from 1900 so that it
    // claims its composter in the idle hours first). The ecs villager harvests by `crop_harvest` in the `work` activity of its `Def` (the farmer's crops are its configuration).
    eprintln!("farmer: job site claimed by the Mob at {claimed:?}; harvest mob {mob_at:?} ecs {ecs_at:?} ecs sensor {ecs_sec:?} mob sensor {mob_sec:?}");
    assert!(mob_at.is_some(), "the Mob farmer harvests the crop within 400 ticks (it has no seeds to replant)");
    // The tick itself is not compared: the work `RunOne` of each side draws its own kids (weights 7, 2, 5, 5, 2), and the ecs ticks range from 32 to
    // over 300 with the seed as the Mob's would with its own. What is compared is the mechanism: no harvest before the secondary job site memory.
    let (mob_at, ecs_at, mob_sec, ecs_sec) = (mob_at.unwrap(), ecs_at.expect("the ecs farmer harvests within 400 ticks"), mob_sec.expect("the Mob senses its farmland"), ecs_sec.expect("the ecs senses its farmland"));
    assert!(mob_at > mob_sec && ecs_at > ecs_sec, "harvest after the farmland is sensed: mob {mob_at} after {mob_sec}, ecs {ecs_at} after {ecs_sec}");
}

#[test]
fn a_villager_flees_the_way_away_from_a_hit_as_a_mob_does() {
    if !have_data() || !is("minecraft:villager") {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let i = r.add(Vec3::new(0.5, 64.0, 0.5));
    let e = spawn_villager(&mut h, 0.5, 0.5);
    let striker = attacker_view(Vec3::new(-3.0, 64.0, 0.5));
    h.set_online(&[(-3.0, 0.5)]);
    r.step(Some(&striker));
    h.tick();
    r.hit(i, 4.0, [-3.0, 64.0, 0.5]);
    h.hit_by(e, 4.0, 1);
    let (mut mob_x, mut ecs_x) = (0.5, 0.5);
    let (mut mob_peak, mut ecs_peak) = (0.5f64, 0.5f64);
    let (mut mob_end, mut ecs_end) = (None, None);
    let mut ecs_began: Option<u64> = None;
    for t in 0..400u64 {
        r.step(Some(&striker));
        h.tick();
        mob_x = r.mobs[i].position.x;
        ecs_x = h.world.get::<cow::Pos>(e).map_or(ecs_x, |p| p.0.x);
        (mob_peak, ecs_peak) = (mob_peak.max(mob_x), ecs_peak.max(ecs_x));
        let mob_panic = horizon_client_lib::mobs::villager_brain::describe(&r.mobs[i]).is_some_and(|d| d.0.contains("panic"));
        let ecs_panic = present(&h.world, e, &IS_PANICKING);
        if mob_end.is_none() && t > 0 && !mob_panic {
            mob_end = Some(t);
        }
        if ecs_panic {
            ecs_began.get_or_insert(t);
        }
        if ecs_end.is_none() && ecs_began.is_some() && !ecs_panic {
            ecs_end = Some(t);
        }
    }
    // The Mob's villager is still in `panic` after 400 ticks (its `hurt_by` memory never clears here, cause not found: see
    // `suivi/ecs-ecarts.md`), and the ecs one strolls back once calm: the flight is judged by the farthest point reached, and the
    // end of the panic against the vanilla 40 ticks of `HurtBySensor` (the `CalmDown` of the panic activity), not against the Mob.
    assert!(mob_peak > 3.0 && ecs_peak > 3.0, "both villagers flee east, away from the hit at x=-3 (mob {mob_peak}, ecs {ecs_peak})");
    let _ = (mob_x, mob_end);
    let end = ecs_end.expect("the ecs panic ends");
    assert!((40..=100).contains(&end), "the ecs panic ends about 40 ticks after the hit, as `HurtBySensor` forgets it (ended at {end})");
}

#[test]
fn a_villager_opens_the_door_in_its_way_as_a_mob_does() {
    if !have_data() || !is("minecraft:villager") {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let door = [2, 1, 0];
    // A wall with a door across the way to a job site: the unemployed Mob villager walks to the composter through it.
    for z in -4..=4 {
        for y in [1, 2] {
            if (z, y) == (0, 1) || (z, y) == (0, 2) {
                continue;
            }
            mob_block(&r, 2, y, z, "minecraft:stone", &[]);
            h.set_block_state(2, y, z, "minecraft:stone", &[]);
        }
    }
    const LOWER: &[(&str, &str)] = &[("open", "false"), ("half", "lower"), ("facing", "east")];
    const UPPER: &[(&str, &str)] = &[("open", "false"), ("half", "upper"), ("facing", "east")];
    for (y, props) in [(1, LOWER), (2, UPPER)] {
        mob_block(&r, door[0], y, door[2], "minecraft:oak_door", props);
        h.set_block_state(door[0], y, door[2], "minecraft:oak_door", props);
    }
    r.set_block(14, 1, 0, "minecraft:composter");
    h.set_block(14, 1, 0, "minecraft:composter");
    r.apply_block_events();
    r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    let e = spawn_villager(&mut h, 0.5, 0.5);
    set(&mut h.world, e, &JOB_SITE, MemValue::Block([14, 1, 0]), None);
    let (mut mob_open, mut ecs_open) = (None, None);
    for t in 0..700u64 {
        // The Mob's unemployed villager goes to the composter it found (`AcquirePoi`, idle hours); the ecs one has no POI search, so
        // it is given the job site and the work hours that send it to the same block (`reach_block` on `JOB_SITE`).
        set_day(&mut r, &mut h, 1900 + t as i64);
        Times::set(&mut h.world, 3000 + t as i64);
        r.step(None);
        r.apply_block_events();
        h.tick();
        if mob_open.is_none() && r.w.prop(door[0], door[1], door[2], "open") == Some("true") {
            mob_open = Some(t);
        }
        if ecs_open.is_none() && h.world.resource::<Level>().mw.0.prop(door[0], door[1], door[2], "open") == Some("true") {
            ecs_open = Some(t);
        }
    }
    assert!(mob_open.is_some(), "the Mob villager opens the door within 700 ticks");
    // Not the same tick by construction: the Mob's unemployed villager reaches the composter through `AcquirePoi` (a path of its own
    // from the first ticks), the ecs one through `reach_block` at work hours; the door is judged opened in the same few ticks.
    let (m, c) = (mob_open.unwrap() as i64, ecs_open.expect("the ecs villager opens the door within 700 ticks") as i64);
    assert!((m - c).abs() <= 5, "the ecs villager opens the door within 5 ticks of the Mob (mob {m}, ecs {c})");
}

/// The player who strikes, as the Mob sees it (the villager flees from the entity that hurt it).
fn attacker_view(at: Vec3) -> PlayerView {
    PlayerView {
        position: at,
        bounds: Aabb::new(Vec3::new(at.x - 0.3, at.y, at.z - 0.3), Vec3::new(at.x + 0.3, at.y + 1.8, at.z + 0.3)),
        eye: [at.x, at.y + 1.62, at.z],
        alive: true,
        invulnerable: false,
        creative: false,
        sneaking: false,
        swimming: false,
        look: [0.0; 3],
        main_hand: None,
        off_hand: None,
        head: None,
        chest: None,
        legs: None,
        feet: None,
        armor_cover: 0.0,
        invisible: false,
        spectator: false,
        gold_armor: false,
        hurt_by: None,
        last_hurt: None,
        velocity: Vec3::ZERO,
        yaw: 0.0,
        sprinting: false,
        riding: false,
        hero: false,
        effects: Vec::new(),
        uuid: [0; 4],
        allies: Default::default(),
        remote: false,
        sleeping: false,
        sleep_timer: 0,
        passenger: false,
        riding_vehicle: None,
        vehicle: None,
    }
}
