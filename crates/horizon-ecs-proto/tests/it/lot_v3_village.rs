//! Le villageois du lot V3 (village, cachette, ciel, raid, cloche) : l'ECS contre le `Mob` sur le résultat observable, troupeau éveillé.
//! Les comportements que le villageois ne lance pas par son `BEntry` (les `Kid` tirés au hasard) sont appelés directement côté ECS.
//! Écrit sans lancer : à vérifier au premier passage.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{villager, villager_brain, Mob};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{bell_heard::HEARD_BELL, hide, raid, sky_spot},
    brain::{
        behaviours::{village_brownian_walk, village_go_to_village, village_home_walk, village_stroll},
        *,
    },
    cow::*,
    goals::*,
    species::villager::{HOME, JOB_SITE},
};

const SITE: [i32; 3] = [3, 1, 3];

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:villager"])
}
fn ctx<T: std::any::Any + Sync>(p: &'static T) -> Ctx {
    Ctx { id: 0, nth: 0, params: p }
}
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
/// The activity of the ECS brain: `describe` gives `activity <name> running [...] memories [...]`.
fn ecs_act(w: &World, e: Entity) -> String {
    horizon_ecs_proto::brain::describe(w, e).and_then(|d| d.split_whitespace().nth(1).map(str::to_string)).unwrap_or_default()
}
/// Whole-token match on the `Mob`'s describe: `raid` does not match `pre_raid`, `walk_target` matches `minecraft:walk_target=...`.
fn mob_token(m: &Mob, tok: &str) -> bool {
    villager_brain::describe(m).is_some_and(|(a, b)| {
        a.split(|c: char| !c.is_alphanumeric() && c != '_').chain(b.split(|c: char| !c.is_alphanumeric() && c != '_')).any(|t| t == tok)
    })
}
fn ecs_xz(h: &Herd, a: Entity, b: Entity) -> f64 {
    xz(h.world.get::<Pos>(a).unwrap().0, h.world.get::<Pos>(b).unwrap().0)
}
/// A stone roof over the 5x5 around the origin, the `Mob` side.
fn roof_mob(r: &mut MobRef) {
    for x in -2..=2 {
        for z in -2..=2 {
            r.set_block(x, 3, z, "minecraft:stone");
        }
    }
}
/// The same roof on the ECS side, then two ticks for the light to settle.
fn roof_ecs(h: &mut Herd) {
    for x in -2..=2 {
        for z in -2..=2 {
            h.set_block(x, 3, z, "minecraft:stone");
        }
    }
    h.tick();
    h.tick();
}

static HOME_WALK: village_home_walk::Params = village_home_walk::Params { range: 48, retry: 20, home: &HOME };
static BROWN: village_brownian_walk::Params = village_brownian_walk::Params { speed: 0.5 };
static VILLAGE: village_go_to_village::Params = village_go_to_village::Params { speed: 0.5, h: 15, v: 7, close: 4 };
static STROLL: village_stroll::Params = village_stroll::Params { speed: 0.75, xz: 2, y: 2 };
static HIDE: hide::Params = hide::Params { homes: village_home_walk::homes, radius: 32, speed: 0.625, close: 2, home: &HOME };
static SKY: sky_spot::Params = sky_spot::Params { tries: 10, speed: 0.5 };

/// `ClosestHomeAsWalkTarget` (`villager_brain.rs:2225`): a bed 8 blocks away becomes the walk target, on the Mob too.
#[test]
fn home_walk_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    h.set_block(8, 1, 0, "minecraft:white_bed");
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 3);
    h.tick();
    h.tick();
    let ecs = (village_home_walk::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&HOME_WALK));
    let mut r = MobRef::new(40, false);
    r.set_block(8, 1, 0, "minecraft:white_bed");
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    let mut mob = false;
    for _ in 0..20 {
        r.step(None);
        mob |= mob_token(&r.mobs[i], "walk_target");
    }
    assert!(ecs && present(&h.world, e, &WALK_TARGET), "ECS : pas de cible vers le lit");
    assert!(mob, "Mob : pas de cible vers le lit");
}

/// `InsideBrownianWalk` (`villager_brain.rs:2278`): under a roof, a random walk target is set, on the Mob too.
#[test]
fn brownian_walk_under_a_roof_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    roof_ecs(&mut h);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 4);
    h.tick();
    let ecs = (village_brownian_walk::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&BROWN));
    let mut r = MobRef::new(40, false);
    roof_mob(&mut r);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    let mut mob = false;
    for _ in 0..40 {
        r.step(None);
        mob |= mob_token(&r.mobs[i], "walk_target");
    }
    assert!(ecs && present(&h.world, e, &WALK_TARGET), "ECS : pas de cible sous le toit");
    assert!(mob, "Mob : pas de cible sous le toit");
}

/// `GoToClosestVillage` (`villager_brain.rs:2309`): a villager 30 blocks from its job site walks toward the village.
#[test]
fn go_to_village_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    h.tick();
    h.tick();
    let e = spawn(&mut h, "minecraft:villager", -30.5, 0.5, 5);
    set(&mut h.world, e, &JOB_SITE, MemValue::Block(SITE), None);
    h.tick();
    let ecs = (village_go_to_village::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&VILLAGE));
    let mut r = MobRef::new(40, false);
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let i = r.add_kind(Vec3::new(-30.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    let goal = Vec3::new(SITE[0] as f64, 1.0, SITE[2] as f64);
    let start = xz(r.mobs[i].position, goal);
    for _ in 0..60 {
        r.step(None);
    }
    let end = xz(r.mobs[i].position, goal);
    assert!(ecs && present(&h.world, e, &WALK_TARGET), "ECS : pas de cible vers le village");
    assert!(end < start - 5.0, "Mob : le village n'attire pas ({start} -> {end})");
}

/// `VillageBoundRandomStroll` (`villager_brain.rs:2006`): a bounded stroll sets a walk target, on the Mob too after a hit.
#[test]
fn village_stroll_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 6);
    h.tick();
    let ecs = (village_stroll::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&STROLL));
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    r.hit(i, 1.0, [-4.5, 1.0, 0.5]);
    let mut mob = false;
    for _ in 0..30 {
        r.step(None);
        mob |= mob_token(&r.mobs[i], "walk_target");
    }
    assert!(ecs && present(&h.world, e, &WALK_TARGET), "ECS : pas de promenade");
    assert!(mob, "Mob : pas de promenade après un coup");
}

/// `WalkAwayFrom` (`villager_brain.rs:2407`): a zombie 3 blocks away, the villager walks away from it, the Mob too.
#[test]
fn flee_hostile_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    let v = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 7);
    let z = spawn(&mut h, "minecraft:zombie", 3.5, 0.5, 8);
    let before = ecs_xz(&h, v, z);
    for _ in 0..20 {
        h.tick();
    }
    let ecs = ecs_xz(&h, v, z) - before;
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    let k = r.add_kind(Vec3::new(3.5, 1.0, 0.5), mob_kind("minecraft:zombie"));
    let mob_before = xz(r.mobs[i].position, r.mobs[k].position);
    for _ in 0..20 {
        r.step(None);
    }
    let mob = xz(r.mobs[i].position, r.mobs[k].position) - mob_before;
    assert!(ecs >= 1.0, "ECS : le villageois ne s'éloigne pas ({ecs})");
    assert!(mob >= 1.0, "Mob : le villageois ne s'éloigne pas ({mob})");
}

/// `LocateHidingPlace` (`villager_brain.rs:2530`): after a bell, the villager picks the bed within its radius as its hiding place.
#[test]
fn hiding_place_after_a_bell_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    h.set_block(8, 1, 0, "minecraft:white_bed");
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 9);
    h.tick();
    h.tick();
    let now = now_of(&h.world, e) as i64;
    set(&mut h.world, e, &HEARD_BELL, MemValue::Int(now), None);
    let ecs = (hide::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&HIDE));
    let mut r = MobRef::new(40, false);
    r.set_block(8, 1, 0, "minecraft:white_bed");
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    villager_brain::heard_bell(&mut r.mobs[i], r.t);
    for _ in 0..20 {
        r.step(None);
    }
    assert!(ecs && present(&h.world, e, &hide::HIDING_PLACE), "ECS : pas de cachette");
    assert!(mob_token(&r.mobs[i], "hiding_place"), "Mob : pas de cachette");
}

/// `SetHiddenState` (`villager_brain.rs:2570`): the hiding ends 300 ticks after the bell, the place is forgotten on both sides.
#[test]
fn hidden_state_ends_300_ticks_after_the_bell_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    h.set_block(8, 1, 0, "minecraft:white_bed");
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 10);
    h.tick();
    h.tick();
    let now = now_of(&h.world, e) as i64;
    set(&mut h.world, e, &HEARD_BELL, MemValue::Int(now), None);
    (hide::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&HIDE));
    let mut r = MobRef::new(40, false);
    r.set_block(8, 1, 0, "minecraft:white_bed");
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    villager_brain::heard_bell(&mut r.mobs[i], r.t);
    for _ in 0..20 {
        r.step(None);
    }
    for _ in 0..310 {
        h.tick();
        (hide::HIDDEN_STATE.can_start)(&mut h.world, e, &ctx(&()));
        r.step(None);
    }
    assert!(!present(&h.world, e, &hide::HIDING_PLACE), "ECS : la cachette dure encore");
    assert!(!mob_token(&r.mobs[i], "hiding_place"), "Mob : la cachette dure encore");
}

/// `MoveToSkySeeingSpot` (`villager_brain.rs:2497`): from under a roof, a spot that sees the sky becomes the walk target, on the Mob too.
#[test]
fn sky_spot_under_a_roof_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    roof_ecs(&mut h);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 11);
    h.tick();
    let ecs = (sky_spot::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(&SKY));
    (sky_spot::BEHAVIOUR.start)(&mut h.world, e, &ctx(&SKY));
    let mut r = MobRef::new(40, false);
    roof_mob(&mut r);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    let mut mob = false;
    for _ in 0..60 {
        r.step(None);
        mob |= mob_token(&r.mobs[i], "walk_target");
    }
    assert!(ecs && present(&h.world, e, &WALK_TARGET), "ECS : pas de case vers le ciel");
    assert!(mob, "Mob : pas de case vers le ciel");
}

/// `SetRaidStatus` and `ResetRaidStatus` (`villager_brain.rs:1800`, `:2464`): an active raid poses the `raid` activity, its end removes it.
#[test]
fn raid_status_is_set_and_reset_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 12);
    let at = [0.5, 1.0, 0.5];
    raid::set_status(&mut h.world, at, 64.0, raid::RAID_ACTIVE);
    let mut ecs_set = false;
    for _ in 0..200 {
        h.tick();
        ecs_set |= ecs_act(&h.world, e) == "raid";
    }
    raid::set_status(&mut h.world, at, 64.0, raid::RAID_NONE);
    for _ in 0..200 {
        h.tick();
    }
    let ecs_reset = ecs_act(&h.world, e) != "raid";
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    villager::ext_mut(&mut r.mobs[i]).raid = villager::RAID;
    villager::ext_mut(&mut r.mobs[i]).raid_wave = true;
    let mut mob_set = false;
    for _ in 0..200 {
        r.step(None);
        mob_set |= mob_token(&r.mobs[i], "raid");
    }
    villager::ext_mut(&mut r.mobs[i]).raid = villager::RAID_NONE;
    for _ in 0..200 {
        r.step(None);
    }
    let mob_reset = !mob_token(&r.mobs[i], "raid");
    assert!(ecs_set && ecs_reset, "ECS : pose {ecs_set}, retrait {ecs_reset}");
    assert!(mob_set && mob_reset, "Mob : pose {mob_set}, retrait {mob_reset}");
}

/// `SetRaidStatus` before a raid (`villager_brain.rs:1800`): the `pre_raid` activity is posed, as on the Mob.
#[test]
fn pre_raid_meeting_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 13);
    raid::set_status(&mut h.world, [0.5, 1.0, 0.5], 64.0, raid::RAID_BEFORE);
    let mut ecs = false;
    for _ in 0..200 {
        h.tick();
        ecs |= ecs_act(&h.world, e) == "pre_raid";
    }
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    villager::ext_mut(&mut r.mobs[i]).raid = villager::PRE_RAID;
    let mut mob = false;
    for _ in 0..200 {
        r.step(None);
        mob |= mob_token(&r.mobs[i], "pre_raid");
    }
    assert!(ecs && mob, "pre_raid : ECS {ecs}, Mob {mob}");
}

/// A won raid (`villager_brain.rs:1800`): the victory poses the `raid` activity while its wave is on, on both brains.
#[test]
fn raid_victory_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 14);
    raid::set_status(&mut h.world, [0.5, 1.0, 0.5], 64.0, raid::RAID_VICTORY);
    let mut ecs = false;
    for _ in 0..200 {
        h.tick();
        ecs |= ecs_act(&h.world, e) == "raid";
    }
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    villager::ext_mut(&mut r.mobs[i]).raid = villager::RAID_VICTORY;
    villager::ext_mut(&mut r.mobs[i]).raid_wave = true;
    let mut mob = false;
    for _ in 0..200 {
        r.step(None);
        mob |= mob_token(&r.mobs[i], "raid");
    }
    assert!(ecs && mob, "victoire : ECS {ecs}, Mob {mob}");
}

/// `ReactToBell` (`villager_brain.rs:1788`): a bell with no raid poses `hide`; with a raid on (any status but none) it does not.
#[test]
fn react_to_bell_awake() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    let quiet = spawn(&mut h, "minecraft:villager", 0.5, 0.5, 15);
    h.tick();
    let now = now_of(&h.world, quiet) as i64;
    set(&mut h.world, quiet, &HEARD_BELL, MemValue::Int(now), None);
    (hide::REACT_TO_BELL.can_start)(&mut h.world, quiet, &ctx(&()));
    let ecs_quiet = ecs_act(&h.world, quiet) == "hide";
    raid::set_status(&mut h.world, [0.5, 1.0, 0.5], 64.0, raid::RAID_BEFORE);
    let busy = spawn(&mut h, "minecraft:villager", 1.5, 0.5, 16);
    h.tick();
    set(&mut h.world, busy, &HEARD_BELL, MemValue::Int(now), None);
    (hide::REACT_TO_BELL.can_start)(&mut h.world, busy, &ctx(&()));
    let ecs_raid = ecs_act(&h.world, busy) == "hide";
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    let j = r.add_kind(Vec3::new(1.5, 1.0, 0.5), mob_kind("minecraft:villager"));
    villager::ext_mut(&mut r.mobs[j]).raid = villager::PRE_RAID;
    villager_brain::heard_bell(&mut r.mobs[i], r.t);
    villager_brain::heard_bell(&mut r.mobs[j], r.t);
    for _ in 0..5 {
        r.step(None);
    }
    let mob_quiet = mob_token(&r.mobs[i], "hide");
    let mob_raid = !mob_token(&r.mobs[j], "hide");
    assert!(ecs_quiet && !ecs_raid, "ECS : sans raid {ecs_quiet}, avec raid hide {ecs_raid}");
    assert!(mob_quiet && mob_raid, "Mob : sans raid {mob_quiet}, avec raid {mob_raid}");
}
