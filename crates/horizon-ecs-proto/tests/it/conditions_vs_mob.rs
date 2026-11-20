//! The conditions of the goals and targets against a real `Mob` in the same scene (`HORIZON_TEST_RESOURCES` set): the spider that gives
//! its target up in light (`quit_in_light`), the drowned whose attack stops when its `okTarget` fails (`keep_while`), the polar bear that
//! takes the player who hits its cub (`alert_if_baby`), the witch's nearest-player target outside a raid.
use crate::common;
use common::*;
use horizon_client_lib::mobs::{self, Who as MWho};
use bevy_ecs::entity::Entity;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::*,
    goals::*,
    hostile,
    targeting::{set_target, Target, Who},
};

fn species_of(h: &Herd, name: &str) -> Species {
    *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap()
}
/// The same daylight on the `Mob` side (the world is shared by reference: the spider test does the same).
fn mob_day(r: &mut MobRef, day: bool) {
    // SAFETY: the test thread owns the world while it edits it.
    unsafe {
        let w = r.w as *const mobs::MobWorld as *mut mobs::MobWorld;
        (*w).daylight = day;
    }
}
/// An ECS creature of the table at `x` (z 0.5), kept alive and stepped.
fn ecs_mob(h: &mut Herd, name: &str, x: f64, seed: u64) -> Entity {
    let sp = species_of(h, name);
    let e = spawn_species(&mut h.world, &sp, x, 0.5, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
fn online(h: &mut Herd, at: Vec3) {
    // The `Mob` scene has a player that takes no hit; the ECS one must not die of the fight either.
    h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
}

/// A spider with a player 8 blocks away as its target, `ticks` ticks by day or by night: does each side still have it at the end.
fn spider_keeps(day: bool, ticks: usize) -> (bool, bool) {
    let at = Vec3::new(8.5, 1.0, 0.5);
    let pv = player_view(at, None);
    let mut r = MobRef::new(40, false);
    mob_day(&mut r, day);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:spider"));
    r.mobs[i].ai.target = Some(MWho::Player);
    let mut h = Herd::with_slab(40, true, false);
    h.set_daylight(day);
    h.set_day(day);
    let e = ecs_mob(&mut h, "minecraft:spider", 0.5, 5);
    online(&mut h, at);
    set_target(&mut h.world, e, Target::player(1, 60));
    for _ in 0..ticks {
        r.step(Some(&pv));
        h.tick();
    }
    (r.mobs[i].ai.target.is_some(), h.world.get::<Target>(e).is_some())
}

/// `SpiderAttackGoal.canContinue`: in light the spider drops its target (one chance in 100 per tick); by night it keeps it.
#[test]
fn a_spider_gives_its_target_up_in_light_as_the_mob_does() {
    if !have_data() {
        return;
    }
    let night = spider_keeps(false, 300);
    eprintln!("spider target by night after 300 ticks: mob/ecs {night:?}");
    assert_eq!(night, (true, true), "witness: by night both keep the target");
    let day = spider_keeps(true, 1000);
    eprintln!("spider target by day after 1000 ticks: mob/ecs {day:?}");
    assert_eq!(day, (false, false), "by day both have given the target up");
}

/// A drowned with a player target, 30 ticks by night (its attack runs), then daylight: the first tick, after the change, at which its
/// attack goal is no longer running on each side (`DrownedAttackGoal`, `okTarget`: the target is in water or it is night).
fn drowned_stops() -> ((bool, bool), Option<usize>, Option<usize>) {
    let at = Vec3::new(2.5, 1.0, 0.5);
    let pv = player_view(at, None);
    let mut r = MobRef::new(40, false);
    mob_day(&mut r, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:drowned"));
    r.mobs[i].ai.target = Some(MWho::Player);
    let mut h = Herd::with_slab(40, true, false);
    h.set_daylight(false);
    h.set_day(false);
    let e = ecs_mob(&mut h, "minecraft:drowned", 0.5, 6);
    online(&mut h, at);
    set_target(&mut h.world, e, Target::player(1, 60));
    // The `Mob` starts its attack on tick 20 and the blow that ends it comes on tick 36: tick 30 is inside the run.
    for _ in 0..30 {
        r.step(Some(&pv));
        h.tick();
    }
    let night = (r.mobs[i].ai.goals.is_running("DrownedAttackGoal"), h.world.get::<hostile::Meleeing>(e).is_some());
    mob_day(&mut r, true);
    h.set_daylight(true);
    h.set_day(true);
    let (mut mob_end, mut ecs_end) = (None, None);
    for t in 0..40 {
        r.step(Some(&pv));
        h.tick();
        if mob_end.is_none() && !r.mobs[i].ai.goals.is_running("DrownedAttackGoal") {
            mob_end = Some(t);
        }
        if ecs_end.is_none() && h.world.get::<hostile::Meleeing>(e).is_none() {
            ecs_end = Some(t);
        }
    }
    (night, mob_end, ecs_end)
}

/// `keep_while`: the drowned's attack ends on the tick its `okTarget` turns false, the same tick on both sides.
#[test]
fn a_drowned_stops_its_attack_when_its_target_is_no_longer_ok_as_the_mob_does() {
    if !have_data() {
        return;
    }
    let (night, mob_end, ecs_end) = drowned_stops();
    eprintln!("drowned attack by night mob/ecs {night:?}; stops after daylight: mob {mob_end:?}, ecs {ecs_end:?}");
    assert_eq!(night, (true, true), "witness: by night both attack");
    assert!(mob_end.is_some() && ecs_end.is_some(), "by day both stop: mob {mob_end:?}, ecs {ecs_end:?}");
    assert_eq!(mob_end, ecs_end, "the same tick");
}

/// `hurt_by` with `alert_if_baby` and `alert_when` adults only: a player hits a cub, the adult bear 4 blocks away takes the player.
#[test]
fn an_adult_polar_bear_takes_the_player_who_hits_its_cub_as_the_mob_does() {
    if !have_data() {
        return;
    }
    let at = Vec3::new(-1.5, 1.0, 0.5);
    let pv = player_view(at, None);
    let mut r = MobRef::new(40, false);
    let cub = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:polar_bear"));
    r.mobs[cub].extra.age = -24000;
    let adult = r.add_kind(Vec3::new(4.5, 1.0, 0.5), mob_kind("minecraft:polar_bear"));
    let mut h = Herd::with_slab(40, true, false);
    let cub_e = ecs_mob(&mut h, "minecraft:polar_bear", 0.5, 7);
    h.make_baby(cub_e, 24000);
    let adult_e = ecs_mob(&mut h, "minecraft:polar_bear", 4.5, 8);
    online(&mut h, at);
    for _ in 0..3 {
        r.step(Some(&pv));
        h.tick();
    }
    r.hit(cub, 1.0, [at.x, at.y, at.z]);
    h.hit_by(cub_e, 1.0, 1);
    for _ in 0..10 {
        r.step(Some(&pv));
        h.tick();
    }
    let (m, x) = (r.mobs[adult].ai.target, h.world.get::<Target>(adult_e).map(|t| t.who));
    eprintln!("adult bear target after a hit on the cub: mob {m:?}, ecs {x:?}");
    assert!(matches!(m, Some(MWho::Player)), "witness: the Mob adult takes the player, got {m:?}");
    assert_eq!(x, Some(Who::Player(1)), "the ecs adult takes the player");
}

/// `Witch`'s nearest-player target outside a raid (`NearestAttackableWitchTargetGoal`): a still player 8 blocks away, both take it.
#[test]
fn a_witch_outside_a_raid_takes_the_nearest_player_as_the_mob_does() {
    if !have_data() {
        return;
    }
    let at = Vec3::new(8.5, 1.0, 0.5);
    let pv = player_view(at, None);
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:witch"));
    let mut h = Herd::with_slab(40, true, false);
    let e = ecs_mob(&mut h, "minecraft:witch", 0.5, 9);
    online(&mut h, at);
    for _ in 0..60 {
        r.step(Some(&pv));
        h.tick();
    }
    let (m, x) = (r.mobs[i].ai.target, h.world.get::<Target>(e).map(|t| t.who));
    eprintln!("witch target outside a raid after 60 ticks: mob {m:?}, ecs {x:?}");
    assert!(matches!(m, Some(MWho::Player)), "witness: the Mob witch takes the player, got {m:?}");
    assert_eq!(x, Some(Who::Player(1)), "the ecs witch takes the player");
}
