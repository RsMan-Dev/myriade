
/// A deadline of ANY length lands on its exact tick: the wheel keeps what is beyond its horizon and brings it in by itself.
#[test]
fn a_wait_of_ten_thousand_ticks_falls_at_the_exact_tick() {
    let mut w = Wheel::default();
    let e = Entity::from_raw_u32(7).unwrap();
    for at in [5_u64, 4095, 4096, 4097, 8192, 10_000, 20_000] {
        w.push(at, e, Due::Burn);
    }
    let mut fired = Vec::new();
    for t in 1..=20_001_u64 {
        w.advance(t);
        let n = w.due_len(t);
        for _ in 0..n {
            fired.push(t);
        }
        w.slots[(t % WHEEL as u64) as usize].clear();
    }
    assert_eq!(fired, vec![5, 4095, 4096, 4097, 8192, 10_000, 20_000]);
    assert!(w.far.is_empty());
}
use super::*;

/// Two cows standing in each other are pushed apart; a sleeping cow they overlap is woken; a baby grows up.
#[test]
fn cows_push_each_other_and_babies_grow() {
if crate::cow::skip_unless_small_body("cows_push_each_other_and_babies_grow") {
    return;
}
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut herd = Herd::new(0, 6.0, true, false, 3);
            let a = herd.world.spawn(cow_bundle(0.5, 0.5, 11)).id();
    let b = herd.world.spawn(cow_bundle(0.8, 0.5, 12)).id();
    let baby = herd.world.spawn(cow_bundle(10.5, 10.5, 13)).id();
    herd.make_baby(baby, 30);
    for _ in 0..4 {
        herd.tick();
    }
    let gap = |herd: &Herd| {
        let (pa, pb) = (herd.world.get::<Pos>(a).unwrap().0, herd.world.get::<Pos>(b).unwrap().0);
        ((pa.x - pb.x).powi(2) + (pa.z - pb.z).powi(2)).sqrt()
    };
    let start = 0.3;
    for _ in 0..20 {
        herd.tick();
    }
    assert!(gap(&herd) > start, "the cows were not pushed apart: {}", gap(&herd));
    assert!(herd.world.get::<Body>(baby).unwrap().width < species().width * 0.75);
    for _ in 0..40 {
        herd.tick();
    }
    assert!(herd.world.get::<Body>(baby).unwrap().width > species().width * 0.9);
}

/// Palier 2 on single cows (no cross reading): a fall, lava, fire, powder snow, a cactus, by events.
#[test]
fn falls_fire_lava_snow_and_cactus_hurt_a_cow() {
if crate::cow::skip_unless_small_body("falls_fire_lava_snow_and_cactus_hurt_a_cow") {
    return;
}
if crate::cow::skip_unless_walker("falls_fire_lava_snow_and_cactus_hurt_a_cow") {
    return;
}
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    // half = 56: the patches are at (-48, -48) and (0, 0), 3 x 3 each (lava, pond, snow in the floor; fire, cactus on it).
    let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let mut cow = |herd: &mut Herd, x: f64, y: f64, z: f64| {
        let e = herd.world.spawn(cow_bundle(x, z, 77)).id();
        herd.world.get_mut::<Pos>(e).unwrap().0.y = y;
        herd.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(x, y, z), species().width, species().height);
        herd.world.entity_mut(e).insert(ActiveUntil(0));
        e
    };
    let health = |herd: &Herd, e: Entity| herd.world.get::<Health>(e).map(|h| h.0);
    let (hurt, dies) = (cow(&mut herd, 5.5, 1.0 + (0.8 * f64::from(species().max_health)).floor() + 3.0, 5.5), cow(&mut herd, 8.5, 1.0 + f64::from(species().max_health) + 5.0, 5.5));
    let lava = cow(&mut herd, -47.0, 1.0, -47.0);
    let fire = cow(&mut herd, -47.0, 1.0, -39.0);
    let snow = cow(&mut herd, -31.0, 1.0, -47.0);
    let cactus = cow(&mut herd, -39.0, 1.0, -39.0);
    let pond = cow(&mut herd, -39.0, 3.0, -47.0);
    // No float goal for this cow (the Mob case has none either): it sinks and drowns.
    herd.world.entity_mut(pond).remove::<crate::goals::Jump>();
    let wall = cow(&mut herd, 20.5, 1.0, 20.5);
    // The eyes inside the stone floor (y = 0.7), whatever the height of the species.
    herd.teleport(wall, Vec3::new(20.5, 0.7 - species().eye_height, 20.5));
    let show = |herd: &Herd, t: u32| {
        let h = |e: Entity| health(herd, e).map_or("dead".to_string(), |h| format!("{h}"));
        eprintln!("tick {t}: fall 11: {}, fall 39: {}, lava: {}, fire: {}, powder snow: {}, cactus: {}, pond: {}, in wall: {}", h(hurt), h(dies), h(lava), h(fire), h(snow), h(cactus), h(pond), h(wall));
    };
    for _ in 0..40 {
        herd.tick();
    }
    show(&herd, 40);
    // The fall: 11 blocks hurt by 8 (ceil(11 - 3)); 39 kill.
    if species().rules.iter().any(|r| r.rule.name == "wing_flap") {
        // A hen takes no fall damage (`causeFallDamage` answers no).
        assert!(health(&herd, hurt) == Some(species().max_health) && health(&herd, dies) == Some(species().max_health), "{:?} {:?}", health(&herd, hurt), health(&herd, dies));
    } else {
        assert!(health(&herd, hurt).is_some_and(|h| h > 0.0 && h < species().max_health), "{:?}", health(&herd, hurt));
        assert!(health(&herd, dies).is_none(), "the cow that fell 39 blocks lives");
    }
    assert!(health(&herd, cactus).is_none_or(|h| h < species().max_health), "the cactus did not hurt");
    for _ in 0..100 {
        herd.tick();
    }
    show(&herd, 140);
    assert!(health(&herd, lava).is_none(), "the cow in lava lives: {:?}", health(&herd, lava));
    assert!(health(&herd, fire).is_none_or(|h| h <= species().max_health - 4.0), "the fire: {:?}", health(&herd, fire));
    for _ in 0..200 {
        herd.tick();
    }
    show(&herd, 340);
    assert!(health(&herd, snow).is_none_or(|h| h < species().max_health), "the powder snow: {:?}", health(&herd, snow));
    // Still burning or dead at tick 340: a species with the panic goal flees the fire burning; one without wanders
    // out of it by chance, and the fire ends (160 ticks) before the tick 340.
    if species().goals.has(&crate::behaviours::panic::MODULE) {
        assert!(herd.world.get::<Burning>(fire).is_some() || health(&herd, fire).is_none());
    }
    // The pond is 2 blocks deep: only an animal whose eyes sit lower than 1.4 blocks goes under (a zombie's are at 1.74).
    // (A hen's glide cuts her sinking speed in water too; whether she drowns as a `Mob` hen does was not compared.)
    // (An amphibian or an aquatic animal does not drown, as in the `Mob`: `breathes_water`, the frog, the axolotl, the fish.)
    if species().eye_height <= 1.4 && !species().breathes_water && !species().rules.iter().any(|r| r.rule.name == "wing_flap") {
        assert!(health(&herd, pond).is_none_or(|h| h < species().max_health), "the cow in the pond did not drown");
    }
    assert!(health(&herd, wall).is_none(), "the cow in the wall lives");
}

/// The cactus cow of the hazard test, tick by tick (position, health, whether a walk or a path started).
#[test]
#[ignore = "trace"]
fn trace_the_cactus_cow() {
    let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let e = herd.world.spawn(cow_bundle(-39.0, -39.0, 77)).id();
    herd.world.get_mut::<Pos>(e).unwrap().0.y = 1.0;
    herd.world.entity_mut(e).insert(ActiveUntil(0));
    for t in 1..=40 {
        herd.tick();
        let p = herd.world.get::<Pos>(e).map(|p| p.0);
        let h = herd.world.get::<Health>(e).map(|h| h.0);
        let walking = herd.world.get::<Stroll>(e).map(|s| s.walking);
        let hz = herd.world.get::<Hazard>(e).map(|h| h.0);
        eprintln!("ecs cactus t={t:3} pos={p:?} health={h:?} walking={walking:?} hazard={hz:?}");
    }
}

/// A cow driven east into the cactus patch (the gesture that counts in play), tick by tick, to compare with `Mob`.
#[test]
#[ignore = "trace"]
fn trace_a_cow_walking_into_a_cactus() {
    let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let e = herd.world.spawn(cow_bundle(-42.5, -39.0, 77)).id();
    herd.world.entity_mut(e).insert(ActiveUntil(0));
    let speed: f32 = std::env::var("SPEED").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0) * 0.2;
    herd.world.get_mut::<Pos>(e).unwrap().0 = Vec3::new(-53.0, 1.0, -20.0);
    herd.world.get_mut::<Bounds>(e).unwrap().0 = box_at(Vec3::new(-53.0, 1.0, -20.0), species().width, species().height);
    herd.world.get_mut::<Yaw>(e).unwrap().0 = -90.0;
    herd.world.get_mut::<Drive>(e).unwrap().speed = speed;
    herd.world.get_mut::<Drive>(e).unwrap().zza = speed;
    let mut last = -53.0;
    for t in 1..=100 {
        herd.tick();
        let Some(p) = herd.world.get::<Pos>(e).map(|p| p.0) else {
            eprintln!("ecs walk t={t:3}: the cow is gone (last x = {last:.2})");
            break;
        };
        let x = p.x;
        if t % 10 == 0 {
            eprintln!("  health {:?} hazard {:?} y {:.2}", herd.world.get::<Health>(e).map(|h| h.0), herd.world.get::<Hazard>(e).map(|h| h.0), p.y);
        }
        if [10, 20, 30, 40, 60, 80, 100].contains(&t) {
            eprintln!("ecs walk t={t:3} x={x:.4} step={:.4} vel.x={:.4}", x - last, herd.world.get::<Vel>(e).unwrap().0.x);
        }
        last = x;
    }
}

/// A cow that keeps jumping under a low ceiling, tick by tick (to compare with `Mob`: the jump delay).
#[test]
#[ignore = "trace"]
fn trace_a_cow_jumping_under_a_ceiling() {
    let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, true);
    let e = herd.world.spawn(cow_bundle(-19.0, -19.0, 77)).id();
    herd.world.entity_mut(e).insert(ActiveUntil(0));
    herd.world.entity_mut(e).remove::<crate::goals::Jump>();
    herd.world.get_mut::<Drive>(e).unwrap().jump = true;
    let ys: Vec<String> = (1..=40)
        .map(|_| {
            herd.tick();
            format!("{:.2}", herd.world.get::<Pos>(e).unwrap().0.y)
        })
        .collect();
    eprintln!("ecs jump y: {}", ys.join(" "));
}

/// Cows and pigs in one world: each keeps its own values (size, panic speed), is tempted by its own item, the
/// two kinds push each other, and they do not breed with each other.
#[test]
fn cows_and_pigs_share_a_world_and_keep_their_own_values() {
    use crate::goals::{carrot, Online, Panic, Player, Tempting, wheat, feed};
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut herd = Herd::with_mix(16, 3.0, true, false, 5, false, 2);
    let table = herd.world.resource::<SpeciesTable>().0.clone();
    let (cow, pig) = (table[0], table[1]);
    assert_ne!((cow.width, cow.height), (pig.width, pig.height));
    let all: Vec<(Entity, u8)> =
        herd.world.query::<(Entity, &SpeciesId)>().iter(&herd.world).map(|(e, i)| (e, i.0)).collect();
    assert!(all.iter().filter(|a| a.1 == 0).count() == 8 && all.iter().filter(|a| a.1 == 1).count() == 8);
    for &(e, id) in &all {
        let sp = table[id as usize];
        let b = herd.world.get::<Body>(e).unwrap();
        assert_eq!((b.width, b.height, b.speed), (sp.width, sp.height, sp.speed), "{}", sp.name);
        assert_eq!(herd.world.get::<Panic>(e).unwrap().speed, sp.goals.param::<crate::behaviours::panic::Params>(&crate::behaviours::panic::MODULE, 0).unwrap().speed);
    }
    // A player with a carrot in the middle: the pigs are tempted, the cows are not; then with wheat, the reverse.
    let tempted = |herd: &mut Herd, id: u8| {
        all.iter().filter(|a| a.1 == id && herd.world.get::<Tempting>(a.0).is_some()).count()
    };
    herd.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: carrot(), off: 0, ..Default::default() });
    for _ in 0..8 {
        herd.tick();
    }
    let (cows_c, pigs_c) = (tempted(&mut herd, 0), tempted(&mut herd, 1));
    herd.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: wheat(), off: 0, ..Default::default() });
    for _ in 0..8 {
        herd.tick();
    }
    let (cows_w, pigs_w) = (tempted(&mut herd, 0), tempted(&mut herd, 1));
    eprintln!("carrot: cows {cows_c} pigs {pigs_c}; wheat: cows {cows_w} pigs {pigs_w}");
    assert!(cows_c == 0 && pigs_c > 0 && pigs_w == 0 && cows_w > 0);
    // A cow and a pig in love next to each other: no baby; two cows in love: a baby.
    let mut herd = Herd::with_mix(0, 3.0, true, false, 5, false, 0);
    let spawn = |herd: &mut Herd, sp: &Species, x: f64| {
        let e = herd.world.spawn(spawn_bundle(sp, x, 0.5, 9 + x as u64)).id();
        herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
        e
    };
    let (c1, p1, c2, c3) = (spawn(&mut herd, &cow, 0.5), spawn(&mut herd, &pig, 2.5), spawn(&mut herd, &cow, -10.5), spawn(&mut herd, &cow, -8.5));
    for e in [c1, p1, c2, c3] {
        assert!(feed(&mut herd.world, e));
    }
    let count = |herd: &mut Herd| herd.world.query::<&SpeciesId>().iter(&herd.world).count();
    for _ in 0..300 {
        herd.tick();
    }
    let babies: Vec<u8> = herd
        .world
        .query_filtered::<&SpeciesId, With<GrowAt>>()
        .iter(&herd.world)
        .map(|i| i.0)
        .collect();
    assert_eq!(count(&mut herd), 5, "one baby, from the two cows only");
    assert_eq!(babies, vec![0], "a cow calf");
}

/// Block changes as events: a floor removed wakes the cow on it only; a stone put at head height suffocates; lava
/// put into a cow's cell adds the hazard module; a path is dropped only by a change near its middle.
#[test]
fn block_changes_wake_only_what_they_touch() {
if crate::cow::skip_unless_small_body("block_changes_wake_only_what_they_touch") {
    return;
}
if crate::cow::skip_unless_walker("block_changes_wake_only_what_they_touch") {
    return;
}
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut herd = Herd::with_hazards(1, 100.0, true, false, 5, false);
    let mut spawn = |herd: &mut Herd, x: f64, z: f64| {
        let e = herd.world.spawn(cow_bundle(x, z, 3 + x as u64)).id();
        herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
        e
    };
    let (a, far, c, d) = (spawn(&mut herd, 10.5, 10.5), spawn(&mut herd, 30.5, 10.5), spawn(&mut herd, 20.5, 20.5), spawn(&mut herd, -20.5, 20.5));
    for _ in 0..10 {
        herd.tick();
    }
    let moving = |herd: &Herd, e: Entity| herd.world.get::<Moving>(e).is_some();
    assert!(!moving(&herd, a) && !moving(&herd, far), "the cows are not at rest");
    let y0 = herd.world.get::<Pos>(a).unwrap().0.y;
    // The floor under `a` is removed: it wakes and falls a block; the cow far away is not touched.
    herd.set_block(10, 0, 10, "minecraft:air");
    herd.tick();
    assert!(moving(&herd, a) && !moving(&herd, far));
    // The fall of a block: 6 ticks, more for a hen (her glide cuts the fall speed).
    for _ in 0..60 {
        herd.tick();
        if !moving(&herd, a) {
            break;
        }
    }
    let y1 = herd.world.get::<Pos>(a).unwrap().0.y;
    assert!((y0 - y1 - 1.0).abs() < 1.0e-6 && !moving(&herd, a), "{y0} -> {y1}");
    // A stone at head height: suffocation by the event.
    // A stone in the block of the eyes: the head is in a wall.
    let eye_y = (1.0 + species().eye_height).ifloor();
    herd.set_block(20, eye_y, 20, "minecraft:stone");
    for _ in 0..15 {
        herd.tick();
    }
    assert!(herd.world.get::<InWall>(c).is_some() && herd.world.get::<Health>(c).unwrap().0 < species().max_health);
    // Lava put into the cell of a resting cow: its hazard module appears and it burns to death.
    herd.set_block(-21, 1, 20, "minecraft:lava");
    herd.tick();
    assert!(herd.world.get::<Hazard>(d).is_some_and(|h| h.0 & HAZ_LAVA != 0));
    for _ in 0..60 {
        herd.tick();
    }
    assert!(herd.world.get::<Health>(d).is_none(), "the cow in lava lives");
    assert!(herd.world.get::<Health>(far).unwrap().0 == species().max_health && !moving(&herd, far));
    // A path is dropped by a change near its middle only.
    let walker = {
        let e = spawn(&mut herd, -30.5, -20.5);
        // A new animal has no known ground until its first sweep: it cannot update a path before.
        herd.tick();
        herd.tick();
        herd.world.get_mut::<Stroll>(e).unwrap().walking = true;
        herd.world.get_mut::<Stroll>(e).unwrap().tx = -10.5;
        herd.world.get_mut::<Stroll>(e).unwrap().tz = -20.5;
        herd.world.entity_mut(e).insert(NeedsPath);
        e
    };
    for _ in 0..4 {
        herd.tick();
    }
    let nodes = |herd: &Herd| herd.world.get::<Nav>(walker).unwrap().path.as_ref().map(|p| p.nodes.len());
    let before = nodes(&herd).expect("a path");
    herd.set_block(45, 1, 45, "minecraft:stone");
    herd.tick();
    assert_eq!(nodes(&herd).map(|n| n + 0), Some(before), "a far change dropped the path");
    let target_before = herd.world.get::<Nav>(walker).unwrap().path.as_ref().unwrap().nodes.clone();
    herd.set_block(-20, 1, -21, "minecraft:stone");
    herd.set_block(-20, 2, -21, "minecraft:stone");
    herd.tick();
    assert!(herd.world.get::<NeedsPath>(walker).is_some() || herd.world.get::<Nav>(walker).unwrap().path.as_ref().is_some_and(|p| p.nodes != target_before));
    for _ in 0..3 {
        herd.tick();
    }
    let new_nodes = herd.world.get::<Nav>(walker).unwrap().path.as_ref().map(|p| p.nodes.clone());
    assert!(new_nodes.clone().is_some_and(|n| n.iter().all(|p| !(p.x == -20 && p.y == 1 && p.z == -21))), "the new path crosses the stone");
    // Less than 20 ticks after that recompute: the next change on the path waits (the old path is kept).
    let nodes_now = new_nodes.unwrap();
    let mid = nodes_now[nodes_now.len() * 2 / 3];
    let (r, remaining) = (herd.world.get::<Nav>(walker).unwrap().last_recompute, herd.world.resource::<Clock>().0);
    assert!(remaining - r < 15, "{r} {remaining}");
    herd.set_block(mid.x, mid.y, mid.z, "minecraft:stone");
    herd.set_block(mid.x, mid.y + 1, mid.z, "minecraft:stone");
    herd.tick();
    herd.tick();
    assert!(herd.world.get::<Nav>(walker).unwrap().delayed, "the recompute is delayed");
    assert_eq!(herd.world.get::<Nav>(walker).unwrap().path.as_ref().map(|p| p.nodes.clone()), Some(nodes_now.clone()));
    for _ in 0..25 {
        herd.tick();
    }
    let after = herd.world.get::<Nav>(walker).unwrap();
    assert!(!after.delayed && after.last_recompute > r + 20, "recomputed after the delay");
}

/// Water placed and removed at any height: `InFluid` and the float, fire put out, and a hazard high above the floor.
#[test]
fn water_placed_floats_extinguishes_and_works_at_height() {
if crate::cow::skip_unless_small_body("water_placed_floats_extinguishes_and_works_at_height") {
    return;
}
if crate::cow::skip_unless_walker("water_placed_floats_extinguishes_and_works_at_height") {
    return;
}
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut herd = Herd::with_hazards(0, 100.0, true, false, 5, false);
    let mut spawn = |herd: &mut Herd, x: f64, z: f64| {
        let e = herd.world.spawn(cow_bundle(x, z, 3 + x as u64)).id();
        herd.world.get_mut::<ActiveUntil>(e).unwrap().0 = 0;
        e
    };
    let (swimmer, burner, high) = (spawn(&mut herd, 10.5, 10.5), spawn(&mut herd, -10.5, 10.5), spawn(&mut herd, 10.5, -10.5));
    for _ in 0..5 {
        herd.tick();
    }
    // Water around and over the swimmer (the floor is at y = 0, the cow stands in y = 1).
    for x in 9..=11 {
        for z in 9..=11 {
            for y in 1..=3 {
                herd.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    for _ in 0..30 {
        herd.tick();
    }
    assert!(herd.world.get::<InFluid>(swimmer).is_some());
    let y = herd.world.get::<Pos>(swimmer).unwrap().0.y;
    // Only a species with the float goal rises (a zombie has none: it stays on the bottom).
    if species().goals.has(&crate::behaviours::float::MODULE) {
        assert!(y > 1.5, "the cow floats up: y = {y}");
    }
    // Fire in the cell of the second cow, then water over it: the burning stops.
    // (The burner stands still for this: a panic that runs from the fire is the goals' business, not the hazards'.)
    herd.world.entity_mut(burner).remove::<crate::goals::Panic>();
    herd.set_block(-11, 1, 10, "minecraft:fire");
    for _ in 0..3 {
        herd.tick();
    }
    assert!(herd.world.get::<Burning>(burner).is_some(), "burning in fire");
    // Water in the column but above the cow does not put it out; the first hit of the fire is on the first tick.
    herd.set_block(-11, 3, 10, "minecraft:water");
    for _ in 0..3 {
        herd.tick();
    }
    assert!(herd.world.get::<Burning>(burner).is_some(), "water above the cow does not extinguish");
    herd.set_block(-11, 1, 10, "minecraft:water");
    herd.set_block(-10, 1, 10, "minecraft:water");
    for _ in 0..3 {
        herd.tick();
    }
    assert!(herd.world.get::<Burning>(burner).is_none(), "water puts the fire out");
    // The water leaves: the swimmer is no longer in a fluid.
    for x in 9..=11 {
        for z in 9..=11 {
            for y in 1..=3 {
                herd.set_block(x, y, z, "minecraft:air");
            }
        }
    }
    for _ in 0..40 {
        herd.tick();
    }
    assert!(herd.world.get::<InFluid>(swimmer).is_none(), "water removed");
    assert!(herd.world.get::<Pos>(swimmer).unwrap().0.y < 1.01, "back on the floor");
    // A hazard 4 blocks up (above the generated floor), then the cow is lifted into it: only then it is hurt.
    herd.set_block(10, 6, -11, "minecraft:lava");
    herd.set_block(10, 6, -10, "minecraft:lava");
    for _ in 0..5 {
        herd.tick();
    }
    assert!(herd.world.get::<Hazard>(high).is_none(), "the lava is out of reach");
    let up = Vec3::new(10.5, 6.0, -10.5);
    herd.world.get_mut::<Pos>(high).unwrap().0 = up;
    herd.world.get_mut::<Bounds>(high).unwrap().0 = box_at(up, species().width, species().height);
    herd.tick();
    assert!(herd.world.get::<Hazard>(high).is_some_and(|h| h.0 & HAZ_LAVA != 0), "lava at height 6");
}

/// A hit from the east pushes the cow west with a hop (knockback 0.4), and wakes its physics.
#[test]
fn a_hit_knocks_a_cow_back() {
if crate::cow::skip_unless_small_body("a_hit_knocks_a_cow_back") {
    return;
}
if crate::cow::skip_unless_walker("a_hit_knocks_a_cow_back") {
    return;
}
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut herd = Herd::new(1, 100.0, true, false, 5);
    let e = herd.world.spawn(cow_bundle(10.5, 10.5, 9)).id();
    herd.world.entity_mut(e).insert(ActiveUntil(0));
    for _ in 0..5 {
        herd.tick();
    }
    assert!(herd.world.get::<Moving>(e).is_none(), "the cow was not at rest");
    herd.hit(e, 1.0, (11.5, 10.5));
    herd.tick();
    // The push is applied at the end of the step (an event); the cow moves in the next one.
    herd.tick();
    let v = herd.world.get::<Vel>(e).unwrap().0;
    assert!(herd.world.get::<Moving>(e).is_some());
    assert!(herd.world.get::<Pos>(e).unwrap().0.x < 10.5 && herd.world.get::<Pos>(e).unwrap().0.y > 1.0, "{v:?}");
}

/// Common sense: the cows stay on the slab and on its ground, walk, fall asleep and wake.
#[test]
fn the_cows_walk_sleep_and_wake_on_the_slab() {
if crate::cow::skip_unless_small_body("the_cows_walk_sleep_and_wake_on_the_slab") {
    return;
}
if crate::cow::skip_unless_farm_goals("the_cows_walk_sleep_and_wake_on_the_slab") {
    return;
}
if crate::cow::skip_unless_walker("the_cows_walk_sleep_and_wake_on_the_slab") {
    return;
}
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut herd = Herd::new(300, 3.0, true, false, 3);
    herd.set_players(vec![(0.0, 0.0)]);
    let half = f64::from(herd.world.resource::<Level>().half);
    let start: Vec<Vec3> = herd.world.query::<&Pos>().iter(&herd.world).map(|p| p.0).collect();
    let (mut max_asleep, mut min_asleep) = (0, usize::MAX);
    for t in 0..800 {
        herd.tick();
        if t >= 400 {
            let (asleep, _, _, _) = herd.counts();
            max_asleep = max_asleep.max(asleep);
            min_asleep = min_asleep.min(asleep);
        }
    }
    let now: Vec<Vec3> = herd.world.query::<&Pos>().iter(&herd.world).map(|p| p.0).collect();
    let travelled = start.iter().zip(&now).filter(|(a, b)| (a.x - b.x).abs() + (a.z - b.z).abs() > 3.0).count();
    // The wall of the slab holds the walkers. An animal that leaps (the goat up to 5 blocks, the frog 2) can go over it, as in the `Mob` (a leap
    // lands on any block with a solid one below it); what it may not do is stay under the world: there it is hurt 4 a tick and dies
    // (`Cause::OutOfWorld`), so no creature is left below it. The other species stay on the slab, on its ground.
    let void_y = f64::from(herd.world.resource::<Level>().mw.0.world.bottom_y() - 64);
    let rows: Vec<(Vec3, bool)> = herd.world.query::<(&Pos, Has<crate::behaviours::long_jump::LongJumpState>)>().iter(&herd.world).map(|(p, l)| (p.0, l)).collect();
    for (p, leaps) in rows {
        if leaps {
            assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite() && p.y >= void_y, "a leaper under the world is still there: {p:?}");
        } else {
            assert!(p.x.abs() < half && p.z.abs() < half && p.y > 0.99 && p.y < 1.2, "{p:?}");
        }
    }
    assert!(travelled > 100, "only {travelled} cows travelled more than 3 blocks");
    assert!(max_asleep > 30 && min_asleep < 300, "{min_asleep}..{max_asleep} asleep of 300");
    // The player leaves: after 100 ticks every cow is idle and asleep, for good.
    herd.set_players(vec![]);
    for _ in 0..400 {
        herd.tick();
    }
    // On the slab (a leaper that went over the wall is where its leap took it, not on the ground of the test: it is left out of both counts).
    let on_slab = |herd: &mut Herd| -> (usize, usize) {
        let rows: Vec<(Vec3, bool, bool)> = herd.world.query::<(&Pos, Has<Moving>, Has<Following>)>().iter(&herd.world).map(|(p, m, f)| (p.0, m, f)).collect();
        let here: Vec<&(Vec3, bool, bool)> = rows.iter().filter(|(p, _, _)| p.x.abs() < half && p.z.abs() < half && p.y > 0.99 && p.y < 1.2).collect();
        (here.iter().filter(|(_, m, f)| !m && !f).count(), here.len())
    };
    let (idle, here) = on_slab(&mut herd);
    assert!(idle * 300 >= here * 295, "{idle} of {here} on the slab asleep after the player left");
    // And the player comes back: the cows around it wake.
    herd.set_players(vec![(0.0, 0.0)]);
    let mut least = 300;
    for _ in 0..300 {
        herd.tick();
        least = least.min(herd.counts().0);
    }
    assert!(least < 250, "{least} idle at the least with a player there");
    eprintln!("travelled {travelled}, asleep between {min_asleep} and {max_asleep} of 300");
}


/// A cow that walks out of the 32 blocks of a player that stays where it is stops being active when it changes
/// cell (the player moved nowhere: only the cell change tells).
#[test]
fn a_cow_leaving_a_standing_player_stops_being_active() {
    if skip_unless_walker("a_cow_leaving_a_standing_player_stops_being_active") {
        return;
    }
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut herd = Herd::new(0, 6.0, true, false, 3);
    let e = herd.world.spawn(cow_bundle(0.5, 0.5, 11)).id();
    herd.set_players(vec![(0.5, 0.5)]);
    herd.tick();
    herd.tick();
    assert_eq!(herd.world.get::<ActiveUntil>(e).unwrap().0, u64::MAX, "a cow next to a player is active");
    herd.teleport(e, Vec3::new(40.5, 1.0, 0.5));
    herd.tick();
    herd.tick();
    let until = herd.world.get::<ActiveUntil>(e).unwrap().0;
    assert!(until != u64::MAX && until <= herd.world.resource::<Clock>().0 + 100, "still active without end: {until}");
}
