//! The variants (spider, llama) of the frame against the `Mob` in the same scene (`HORIZON_TEST_RESOURCES` set): the spider hunts a player
//! only in the dark (`SpiderTargetGoal`); a llama hit by a player takes it as its target (`LlamaHurtByTargetGoal`).
use crate::common;
use common::*;
use horizon_client_lib::mobs::{self, Who as MWho};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*, targeting::Target};

fn species_of(h: &Herd, name: &str) -> horizon_ecs_proto::cow::Species {
    *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap()
}

/// A spider and a still player 8 blocks away, 150 ticks: does the spider have a target at the end, by day and by night, on both sides.
fn spider_has_target(day: bool) -> (bool, bool) {
    let mut r = MobRef::new(40, false);
    // SAFETY: the test thread owns the world while it edits it.
    unsafe {
        let w = r.w as *const mobs::MobWorld as *mut mobs::MobWorld;
        (*w).daylight = day;
    }
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), horizon_ecs_proto::cow::mob_kind("minecraft:spider"));
    let mut h = Herd::with_slab(40, true, false);
    h.set_daylight(day);
    let sp = species_of(&h, "minecraft:spider");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    let at = Vec3::new(8.5, 1.0, 0.5);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
    let pv = player_view(at, None);
    for _ in 0..150 {
        r.step(Some(&pv));
        h.tick();
    }
    (r.mobs[i].ai.target.is_some(), h.world.get::<Target>(e).is_some())
}

#[test]
fn a_spider_hunts_a_player_only_in_the_dark_as_the_mob_does() {
    if !have_data() {
        return;
    }
    let (night, day) = (spider_has_target(false), spider_has_target(true));
    eprintln!("target at 150 ticks: night mob/ecs {night:?}, day mob/ecs {day:?}");
    assert!(night.0 && night.1, "witness: both hunt by night (mob {}, ecs {})", night.0, night.1);
    assert_eq!(day.0, day.1, "by day the two sides agree (mob {}, ecs {})", day.0, day.1);
}

/// A llama hit once by a player: both take the player as target, within 10 ticks.
#[test]
fn a_hit_llama_takes_the_player_as_the_mob_does() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), horizon_ecs_proto::cow::mob_kind("minecraft:llama"));
    let mut h = Herd::with_slab(40, true, false);
    let sp = species_of(&h, "minecraft:llama");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    let at = Vec3::new(3.5, 1.0, 0.5);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, main: 0, off: 0, ..Default::default() });
    let pv = player_view(at, None);
    for _ in 0..3 {
        r.step(Some(&pv));
        h.tick();
    }
    r.hit(i, 1.0, [at.x, at.y, at.z]);
    h.hit_by(e, 1.0, 1);
    for _ in 0..10 {
        r.step(Some(&pv));
        h.tick();
    }
    let (m, x) = (r.mobs[i].ai.target, h.world.get::<Target>(e).map(|t| t.who));
    eprintln!("llama target after a hit: mob {m:?}, ecs {x:?}");
    assert!(matches!(m, Some(MWho::Player)), "witness: the Mob llama targets the player, got {m:?}");
    assert_eq!(x, Some(horizon_ecs_proto::targeting::Who::Player(1)), "the ecs llama targets the player");
}
