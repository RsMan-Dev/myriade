//! Restes du lot 8 (`villager_brain.rs`) : les témoins d'un meurtre gardent le rayon de 16 blocs autour de la victime.
use horizon_ecs_proto::behaviours::gossip::{witness_murder, Gossips};
use horizon_ecs_proto::cow::{spawn_species, Herd, SpeciesTable};

#[test]
fn witnesses_use_sensor_candidates_radius_invisibility_and_cached_results() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(24, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let villager = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let dead = spawn_species(&mut h.world, &villager, 0.5, 0.5, 5);
    let near = spawn_species(&mut h.world, &villager, 15.5, 0.5, 5);
    let far = spawn_species(&mut h.world, &villager, 17.5, 0.5, 5);
    witness_murder(&mut h.world, dead, 1);
    assert!(h.world.get::<Gossips>(near).is_some(), "a villager 15 blocks from the victim sees the murder");
    assert!(h.world.get::<Gossips>(far).is_none(), "a villager 17 blocks from the victim is out of the 16 blocks");
}

use crate::common::*;
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::villager;
use horizon_ecs_proto::{
    behaviours::{
        poi_claim::{self, ClaimIndex},
        profession::Profession,
        trade_offers::{Merchant, TradeData},
    },
    brain::behaviours::poi_assign,
    brain::*,
    cow::*,
    goals::Entered,
    species::villager::{JOB_SITE, POTENTIAL_JOB_SITE},
};
use horizon_world::poi::PoiType;

/// A job point the claim accepts, and the reset's parameters, as in `lot_v1_poi.rs`.
fn accept_job(k: PoiType) -> bool {
    k.is_job()
}
fn no_acquired(_: &mut World, _: Entity, _: [i32; 3]) {}
fn no_lost(_: &mut World, _: Entity) {}
static OWN_JOB: poi_claim::Params = poi_claim::Params { memory: &POTENTIAL_JOB_SITE, accept: accept_job, range: 48, filter: poi_claim::Filter::OwnJob, on_acquired: no_acquired, on_lost: no_lost };
static RESET_SITE: poi_assign::Params = poi_assign::Params { job: &JOB_SITE, potential: &POTENTIAL_JOB_SITE, professions: &[] };

/// `Villager.canUse` of a profession id the table does not hold (`villager_brain.rs:4440`): the villager claims no job point and gives
/// no profession back, whatever its experience. Index 200 is past the table; the farmer of the same scene does both (the control).
#[test]
fn an_unresolved_profession_claims_no_job_and_resets_none() {
    if !have_data() {
        return;
    }
    let farmer = villager::profession_index("farmer").unwrap();
    let mut h = Herd::with_slab_awake(40, true, false);
    h.world.insert_resource(ClaimIndex::default());
    let (items, enchantments, trades) = crate::common::registries();
    h.world.insert_resource(TradeData { trades, items, enchantments, seed: 0x5EED, sequences: Default::default() });
    h.tick();
    h.set_block(3, 1, 3, "minecraft:composter");
    h.tick();
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let spawn = |h: &mut Herd, profession: u8| {
        let e = spawn_species(&mut h.world, &sp, 3.5, 3.5, 11);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
        h.world.entity_mut(e).insert(Merchant::new(false, profession, 0));
        e
    };
    let unresolved = spawn(&mut h, 200);
    let known = spawn(&mut h, farmer);
    let claim = Ctx { id: 0, nth: 0, params: &OWN_JOB };
    let reset = Ctx { id: 0, nth: 0, params: &RESET_SITE };
    assert!((poi_claim::BEHAVIOUR.can_start)(&mut h.world, known, &claim), "the control farmer claims the composter");
    assert!(!(poi_claim::BEHAVIOUR.can_start)(&mut h.world, unresolved, &claim), "an unresolved profession claims no job point");
    h.world.entity_mut(unresolved).insert(Profession(PoiType::Farmer));
    h.world.entity_mut(known).insert(Profession(PoiType::Farmer));
    assert!((poi_assign::RESET.can_start)(&mut h.world, known, &reset), "the control farmer without a site and no experience resets");
    assert!(!(poi_assign::RESET.can_start)(&mut h.world, unresolved, &reset), "an unresolved profession resets to nothing");
}

/// `poi_matches` of the job filters (`villager_brain.rs:4463`): a farmer holds the farmer's point and not a butcher's, an unemployed
/// villager may acquire a farmer's point but holds none, and a nitwit holds no point.
#[test]
fn job_points_held_and_acquirable_follow_the_profession_table() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    use myriade_engine_lib::mobs::villager;
    use horizon_world::poi::{PoiIdentity, PoiType};
    let (_, _, trades) = crate::common::registries();
    let table = trades.professions.clone();
    let farmer = usize::from(villager::profession_index("farmer").unwrap());
    let nitwit = usize::from(villager::profession_index("nitwit").unwrap());
    let farm = PoiIdentity::Vanilla(PoiType::Farmer);
    assert!(table.accepts(farmer, farm, true), "a farmer holds the farmer's point");
    assert!(table.accepts(farmer, farm, false), "a farmer may acquire the farmer's point");
    assert!(!table.accepts(farmer, PoiIdentity::Vanilla(PoiType::Butcher), true), "a farmer does not hold a butcher's point");
    assert!(!table.accepts(nitwit, farm, true), "a nitwit holds no point");
    assert!(table.accepts(0, farm, false), "an unemployed villager may acquire a farmer's point");
    assert!(!table.accepts(0, farm, true), "an unemployed villager holds none");
}

/// The meeting point's reach and tickets (`PoiType.validRange` 6, `maxTickets` 32 in `PoiTypes`), the job points' one ticket.
#[test]
fn a_meeting_point_is_valid_within_six_blocks_and_gives_32_tickets() {
    use horizon_world::poi::PoiType;
    assert_eq!(PoiType::Meeting.valid_range(), 6);
    assert_eq!(PoiType::Meeting.max_tickets(), 32);
    assert_eq!(PoiType::Farmer.max_tickets(), 1);
}
