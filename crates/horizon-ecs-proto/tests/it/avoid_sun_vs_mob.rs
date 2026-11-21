//! `Nav.avoid_sun` (`RestrictSunGoal`) of the frame against the `Mob` skeleton (`HORIZON_SPECIES=minecraft:skeleton`): a skeleton in the shade of a
//! roof is sent to a spot in the open. By day the path ends at the roof's edge (`trimPath`); by night it goes all the way.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::{restrict_sun, Entry}, cow::*, goals::*};

const ROOF: i32 = 6;
fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:skeleton"])
}
fn roof_cells() -> impl Iterator<Item = (i32, i32)> {
    (-6..=6).flat_map(|x| (-3..=3).map(move |z| (x, z)))
}
fn mob_end(day: bool, avoid: bool) -> f64 {
    let mut r = MobRef::new(24, false);
    for (x, z) in roof_cells() {
        r.set_block(x, ROOF, z, "minecraft:stone");
    }
    // SAFETY: the test thread owns the world while it edits it.
    unsafe {
        mobs::world::relight_test_world(&mut *r.level.blocks.0, &roof_cells().map(|(x, z)| (x, ROOF, z)).collect::<Vec<_>>());
        let w = r.w as *const mobs::MobWorld as *mut mobs::MobWorld;
        (*w).daylight = day;
    }
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:skeleton"));
    r.mobs[i].ai.goals.clear();
    r.mobs[i].ai.targets.clear();
    // The goal `RestrictSun` is on only while it is bright outside.
    r.mobs[i].ai.nav.avoid_sun = avoid && day;
    for _ in 0..3 {
        r.step(None);
    }
    let w = r.w;
    r.mobs[i].nav_move_to(w, [12.5, 1.0, 0.5], 1.0);
    for _ in 0..200 {
        r.step(None);
    }
    r.mobs[i].position.x
}
fn ecs_end(day: bool, avoid: bool) -> f64 {
    let mut h = Herd::with_slab_awake(24, false, false);
    for (x, z) in roof_cells() {
        h.set_block(x, ROOF, z, "minecraft:stone");
    }
    h.set_day(day);
    let mut sp = *species();
    let entries: &'static [Entry] = if avoid { Box::leak(Box::new([Entry::new(&restrict_sun::MODULE, 2, &restrict_sun::Params)])) } else { &[] };
    sp.goals = horizon_ecs_proto::species::Config { entries };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
    h.world.resource_mut::<Entered>().0.push(e);
    for _ in 0..3 {
        h.tick();
    }
    go_to_at(&mut h.world, e, [12.5, 1.0, 0.5], 1.0, 1);
    for _ in 0..200 {
        h.tick();
    }
    h.world.get::<Pos>(e).unwrap().0.x
}

#[test]
fn a_skeleton_by_day_stops_where_the_sky_begins_as_the_original() {
    if skip() {
        return;
    }
    let (me, ee) = (mob_end(true, true), ecs_end(true, true));
    let (mn, en) = (mob_end(false, true), ecs_end(false, true));
    let (mf, ef) = (mob_end(true, false), ecs_end(true, false));
    eprintln!("x at 200 ticks, goal on, day: mob {me:.2} ecs {ee:.2}; night: mob {mn:.2} ecs {en:.2}; day, no goal: mob {mf:.2} ecs {ef:.2}");
    assert!((me - ee).abs() < 1.0 && me < 8.0, "day {me} {ee}");
    assert!(mn > 11.0 && en > 11.0 && mf > 11.0 && ef > 11.0, "{mn} {en} {mf} {ef}");
}
