//! The job site of a villager, in the same world on both sides (`HORIZON_TEST_RESOURCES` set): a composter put down, the villager
//! takes it and gets its profession (on the tick after), the block goes and the profession goes with it, and of two villagers
//! for one job site exactly one takes it. The bed and the bell are not in this file yet: their point type is not in the
//! registry (villager-poi.md, section 3).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::villager;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::poi_claim::{self, ClaimIndex, Params},
    behaviours::profession::Profession,
    brain::*,
    cow::*,
    goals::*,
    poi::PoiRegistry,
};
use horizon_world::poi::PoiType;

const SITE: [i32; 3] = [3, 1, 3];

static JOB_MEM: MemKey = MemKey::new("test:job_site", Shape::Block);
fn accept_job(k: PoiType) -> bool {
    k.is_job()
}
fn on_acquired(w: &mut bevy_ecs::world::World, e: Entity, p: [i32; 3]) {
    horizon_ecs_proto::behaviours::profession::on_acquired(w, e, p);
}
fn on_lost(w: &mut bevy_ecs::world::World, e: Entity) {
    horizon_ecs_proto::behaviours::profession::on_lost(w, e);
}
static PARAMS: Params = Params { memory: &JOB_MEM, accept: accept_job, range: 1, filter: horizon_ecs_proto::behaviours::poi_claim::Filter::Any, on_acquired, on_lost };
fn params() -> &'static Params {
    &PARAMS
}
fn ctx(p: &'static Params) -> Ctx {
    Ctx { id: 0, nth: 0, params: p }
}
/// A villager of the ecs at `(x, 1, z)`, awake in the world: its brain holds the memories.
fn villager_at(h: &mut Herd, x: f64, z: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// The ecs: a villager takes the composter's point (its profession is the composter's job), the block goes, the registry says
/// so, the claim goes, the profession goes.
#[test]
fn ecs_job_site_is_taken_and_lost_with_its_block() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    h.world.insert_resource(ClaimIndex::default());
    h.tick();
    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    h.tick();
    let e = villager_at(&mut h, 3.5, 3.5, 11);
    let p = params();
    assert!((poi_claim::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(p)));
    (poi_claim::BEHAVIOUR.start)(&mut h.world, e, &ctx(p));
    assert_eq!(h.world.get::<Profession>(e), Some(&Profession(PoiType::Farmer)));

    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:air");
    // The registry's events are readable the tick after the one that read the change: two ticks (the Mob loses the job in one).
    h.tick();
    h.tick();
    assert_eq!(h.world.resource::<PoiRegistry>().removed(), &[SITE]);
    poi_claim::lose_claims(&mut h.world);
    assert!(h.world.get::<Profession>(e).is_none());
    assert!(!present(&h.world, e, &JOB_MEM));
}

/// The same scene on the `Mob`: the villager takes the job site within 40 ticks, the profession is the composter's; the block
/// goes, the profession goes within 5 ticks.
#[test]
fn mob_job_site_is_taken_and_lost_with_its_block() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(40, false);
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let i = r.add_kind(Vec3::new(3.5, 1.0, 3.5), mob_kind("minecraft:villager"));
    let farmer = villager::profession_index("farmer");
    for _ in 0..40 {
        r.step(None);
    }
    assert_eq!(Some(r.mobs[i].extra.profession.index()), farmer.map(Some));
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:air");
    for _ in 0..5 {
        r.step(None);
    }
    assert_eq!(r.mobs[i].extra.profession.index(), Some(villager::NONE));
}

/// Two villagers for one job site (the ecs and the `Mob`): exactly one takes it, the other has no profession.
#[test]
fn two_villagers_one_job_site_one_profession() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, true, false);
    h.world.insert_resource(ClaimIndex::default());
    h.tick();
    h.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    h.tick();
    let a = villager_at(&mut h, 3.5, 3.5, 11);
    let b = villager_at(&mut h, 3.6, 3.6, 12);
    let p = params();
    for e in [a, b] {
        if (poi_claim::BEHAVIOUR.can_start)(&mut h.world, e, &ctx(p)) {
            (poi_claim::BEHAVIOUR.start)(&mut h.world, e, &ctx(p));
        }
    }
    let with = [a, b].iter().filter(|&&e| h.world.get::<Profession>(e).is_some()).count();
    assert_eq!(with, 1);

    let mut r = MobRef::new(40, false);
    r.set_block(SITE[0], SITE[1], SITE[2], "minecraft:composter");
    let i = r.add_kind(Vec3::new(3.5, 1.0, 2.5), mob_kind("minecraft:villager"));
    let j = r.add_kind(Vec3::new(3.5, 1.0, 4.5), mob_kind("minecraft:villager"));
    let farmer = villager::profession_index("farmer").map(Some);
    for _ in 0..300 {
        r.step(None);
    }
    let held = [i, j].iter().filter(|&&k| Some(r.mobs[k].extra.profession.index()) == farmer).count();
    assert_eq!(held, 1);
}
