//! The cadence of a melee attack: a blow arms `ATTACK_COOLING_DOWN` for `cooldown` ticks, and the memory erases it one tick after it reaches
//! zero, so two blows come `cooldown + 1` ticks apart. The cooldowns are the Mob's: 40 (adult zoglin), 15 (baby zoglin), 20 (piglin brute and piglin).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::Cap;
use horizon_ecs_proto::brain::behaviours::melee_attack::{self, Params};
use horizon_ecs_proto::brain::{bundle, brain_step, set, wake, ActivityDef, BEntry, BrainDef, MemValue, Need, Needs, Plan, ATTACK_COOLING_DOWN, ATTACK_TARGET, VISIBLE_MOBS};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::targeting::Who;

const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
static NEEDS: Needs = &[(&ATTACK_TARGET, Need::Present), (&ATTACK_COOLING_DOWN, Need::Absent)];

fn always(_: &World, _: Entity) -> bool {
    true
}

/// A cow with a melee attack of `cooldown` strikes a cow in reach that it sees, at every tick of 100: the ticks of its blows.
fn blows(cooldown: u64) -> Vec<usize> {
    let params: &'static Params = Box::leak(Box::new(Params { cooldown, can_move: always }));
    let entries: &'static [BEntry] = Box::leak(Box::new([BEntry { behaviour: &melee_attack::BEHAVIOUR, activity: "core", priority: 1, params, needs: NEEDS, duration: None, kids: &[] }]));
    let def = BrainDef {
        sensors: &[],
        activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
        pick: &[],
        initial: &[],
        behaviours: entries,
    };
    let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
    let mut h = Herd::with_slab_awake(40, true, false);
    let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
    sp.goals = Config { entries: &[] };
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 91);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.entity_mut(e).insert(bundle(plan));
    horizon_ecs_proto::brain::start(&mut h.world, e);
    let prey = spawn_species(&mut h.world, &sp, 1.5, 0.5, 92);
    h.world.get_mut::<ActiveUntil>(prey).unwrap().0 = u64::MAX;
    let mut at = Vec::new();
    for t in 0..100 {
        h.tick();
        set(&mut h.world, e, &ATTACK_TARGET, MemValue::Who(Who::Entity(prey)), None);
        set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(vec![Who::Entity(prey)]), None);
        wake(&mut h.world, e);
        brain_step(&mut h.world);
        let mut queue = h.world.resource_mut::<DamageQueue>();
        if queue.0.iter().any(|x| x.target == prey) {
            at.push(t);
        }
        queue.0.clear();
    }
    at
}

#[test]
fn the_blows_come_cooldown_plus_one_ticks_apart() {
    if !have_data() {
        return;
    }
    for cooldown in [40_u64, 15, 20] {
        let at = blows(cooldown);
        assert!(at.len() >= 2, "témoin : au moins deux coups pour {cooldown} : {at:?}");
        for w in at.windows(2) {
            assert_eq!(w[1] - w[0], cooldown as usize + 1, "cooldown {cooldown}: blows at {at:?}");
        }
    }
}
