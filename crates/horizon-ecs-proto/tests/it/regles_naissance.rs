//! `finalizeSpawnChildFromBreeding`: a birth with the `mob_drops` rule on leaves 1 to 7 experience at the parents' place; with the rule off
//! the pair still breeds and no orb comes. The Mob's test uses llamas; the ECS one uses cows, whose breeding gives experience too.
use crate::common::*;
use horizon_ecs_proto::behaviours::breed::{self, feed, Params};
use horizon_ecs_proto::behaviours::Entry;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;

/// Two cows in love side by side under the `mob_drops` rule `drops`: whether a baby is born within 600 ticks, and the experience of
/// the orbs left by then.
fn births(drops: bool) -> (bool, u32) {
    let mut h = Herd::with_slab_awake(60, true, false);
    let mut level = *h.world.resource::<Level>();
    let rule: &'static horizon_client_lib::mobs::MobWorld<'static> = Box::leak(Box::new(horizon_client_lib::mobs::MobWorld { mob_drops: drops, ..level.mw.0.clone() }));
    level.mw = Mw(rule);
    *h.world.resource_mut::<Level>() = level;
    let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&breed::MODULE, 2, Box::leak(Box::new(Params::DEFAULT)))]));
    sp.goals = Config { entries };
    let a = spawn_armed(&mut h.world, &sp, 0.5, 0.5, 71);
    let b = spawn_armed(&mut h.world, &sp, 2.5, 0.5, 72);
    for e in [a, b] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    }
    h.tick();
    assert!(feed(&mut h.world, a) && feed(&mut h.world, b));
    let mut born = false;
    for _ in 0..600 {
        h.tick();
        born |= h.world.query::<&GrowAt>().iter(&h.world).count() > 0;
    }
    let xp = h.world.query::<&horizon_ecs_proto::orb::Orb>().iter(&h.world).map(|o| o.value.max(0) as u32 * o.count).sum();
    (born, xp)
}

#[test]
fn a_birth_gives_one_to_seven_experience_with_the_rule_and_none_without() {
    if !have_data() {
        return;
    }
    let (born, xp) = births(true);
    assert!(born && (1..=7).contains(&xp), "rule on: born {born}, {xp} experience");
    let (born, xp) = births(false);
    assert!(born, "témoin : le couple naît sans la règle");
    assert_eq!(xp, 0, "rule off: no experience");
}
