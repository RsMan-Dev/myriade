//! The goat's ram, ECS against a real `Mob` goat in the same flat scene (`HORIZON_SPECIES=minecraft:goat`). The ECS draws its first
//! cooldown at random (`ram::arm`) and the `Mob` starts at 0, so the timeline is compared from the head going down, not from the start.
//! Not observable on the `Mob` (private `goat::State`): the phases; the start is read from the entity event 58 and the cooldown from
//! `goat::ram_cooldown`.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::{goat, MobEvent};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::ram::{Ramming, RamCooldown},
    cow::*,
    goals::*,
    hostile::PlayerState,
    host_out::HostEvent,
};

fn goat_species() -> bool {
    have_data() && process_species_name() == "minecraft:goat"
}
fn mob_at(rf: &mut MobRef, name: &str, x: f64) -> usize {
    rf.add_kind(Vec3::new(x, 1.0, 0.5), mob_kind(name))
}
fn ecs_at(h: &mut Herd, name: &str, x: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, 0.5, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}
fn hits_player(e: &MobEvent) -> bool {
    match e {
        MobEvent::DamagePlayer { .. } => true,
        MobEvent::ForPlayer { event, .. } => hits_player(event),
        _ => false,
    }
}
/// The `Mob` goat `id` lowers its head this tick (entity event 58).
fn mob_head_down(events: &[MobEvent], id: u32) -> bool {
    events.iter().any(|e| matches!(e, MobEvent::EntityEvent { mob, event: 58, .. } if *mob == id))
}
/// The charge comes a fixed time after the head goes down (the start spot is reached, then `RAM_PREPARE_TIME`): the gap is compared.
/// The charge is a move of more than 0.4 blocks a tick, as in `goat_ram_vs_mob`.
#[test]
fn a_goat_starts_its_charge_on_the_same_tick_as_the_mob() {
    if !goat_species() {
        return;
    }
    let mut rf = MobRef::new(60, false);
    let g = mob_at(&mut rf, "minecraft:goat", 0.5);
    mob_at(&mut rf, "minecraft:pig", 9.5);
    let id = rf.mobs[g].id;
    let (mut head, mut charge, mut last) = (None, None, rf.mobs[g].position.x);
    for t in 1..=7000i64 {
        let from = rf.events.len();
        rf.step(None);
        if mob_head_down(&rf.events[from..], id) {
            head.get_or_insert(t);
        }
        let x = rf.mobs[g].position.x;
        if (x - last).abs() > 0.4 && head.is_some() {
            charge.get_or_insert(t);
        }
        last = x;
    }
    let mob_gap = charge.expect("the Mob goat charges") - head.expect("the Mob head goes down");

    let mut h = Herd::with_slab_awake(60, false, false);
    let eg = ecs_at(&mut h, "minecraft:goat", 0.5, 1000);
    ecs_at(&mut h, "minecraft:pig", 9.5, 1001);
    let (mut head, mut charge, mut last) = (None, None, 0.5);
    for t in 1..=7000i64 {
        h.tick();
        let down = h.world.resource_mut::<Events>().host.drain().any(|ev| matches!(ev, HostEvent::EntityEvent { entity, code: 58 } if entity == eg));
        if down {
            head.get_or_insert(t);
        }
        let x = h.world.get::<Pos>(eg).map_or(last, |p| p.0.x);
        if (x - last).abs() > 0.4 && head.is_some() {
            charge.get_or_insert(t);
        }
        last = x;
    }
    let ecs_gap = charge.expect("the ECS goat charges") - head.expect("the ECS head goes down");
    eprintln!("charge after the head: mob {mob_gap} ticks, ecs {ecs_gap} ticks");
    assert!((mob_gap - ecs_gap).abs() <= 1, "ticks from the head down to the charge: mob {mob_gap}, ecs {ecs_gap}");
}

/// The creature the goat rams first in a flat scene: 'p' the player at `px`, 'c' the cow at `cx`, None if neither is hit.
fn mob_first(px: f64, creative: bool, cx: f64) -> Option<char> {
    let mut rf = MobRef::new(60, false);
    mob_at(&mut rf, "minecraft:goat", 0.5);
    let c = mob_at(&mut rf, "minecraft:cow", cx);
    let cid = rf.mobs[c].id;
    let mut p = player_view(Vec3::new(px, 1.0, 0.5), None);
    p.creative = creative;
    for _ in 0..7000 {
        let from = rf.events.len();
        rf.step(Some(&p));
        let ev = &rf.events[from..];
        if ev.iter().any(hits_player) {
            return Some('p');
        }
        if ev.iter().any(|e| matches!(e, MobEvent::AttackMob { target, .. } if *target == cid)) {
            return Some('c');
        }
    }
    None
}
fn ecs_first(px: f64, creative: bool, cx: f64) -> Option<char> {
    let mut h = Herd::with_slab_awake(60, false, false);
    ecs_at(&mut h, "minecraft:goat", 0.5, 1000);
    let c = ecs_at(&mut h, "minecraft:cow", cx, 1001);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: px, y: 1.0, z: 0.5, ..Default::default() });
    h.world.resource_mut::<PlayerState>().set_creative(1, creative);
    h.world.resource_mut::<PlayerState>().immortal = true;
    let full = h.world.get::<Health>(c).unwrap().0;
    for _ in 0..7000 {
        h.tick();
        if h.world.resource::<PlayerState>().hits.iter().any(|x| x.player == 1) {
            return Some('p');
        }
        if h.world.get::<Health>(c).is_some_and(|x| x.0 < full) {
            return Some('c');
        }
    }
    None
}

/// A player and a cow at reach: the `Mob`'s `nearest_target` takes the player first and the cow only when strictly nearer; a
/// creative player is no target.
#[test]
fn a_goat_picks_the_player_before_a_creature_like_the_mob() {
    if !goat_species() {
        return;
    }
    // Cows wander on both sides, so the cases keep the cow out of reach (40) or the player ignored: the player is the target
    // when it is the only candidate, and a creative player is no target (the cow 9.5 is rammed instead).
    let (m, e) = (mob_first(5.5, false, 40.0), ecs_first(5.5, false, 40.0));
    assert_eq!((m, e), (Some('p'), Some('p')), "the player alone (mob, ecs)");
    let (m, e) = (mob_first(5.5, true, 9.5), ecs_first(5.5, true, 9.5));
    assert_eq!((m, e), (Some('c'), Some('c')), "a creative player is no target (mob, ecs)");
}

/// The charge hits a cow: the cow's velocity a tick after the hit (the knockback, away from the goat) and the end of the charge
/// (`finishRam`: no ram left, the cooldown set in the 600 to 6000 range).
#[test]
fn a_goat_ram_recoil_matches_the_mob() {
    if !goat_species() {
        return;
    }
    let mut rf = MobRef::new(60, false);
    let g = mob_at(&mut rf, "minecraft:goat", 0.5);
    let c = mob_at(&mut rf, "minecraft:cow", 9.5);
    let full = rf.mobs[c].health;
    let far = player_view(Vec3::new(40.0, 1.0, 40.0), None);
    let (mut hit, mut mob_power) = (false, 0.0f64);
    for _ in 0..7000 {
        let from = rf.events.len();
        rf.step(Some(&far));
        if let Some(MobEvent::Knockback { power, .. }) = rf.events[from..].iter().find(|e| matches!(e, MobEvent::Knockback { .. })) {
            mob_power = *power;
        }
        if rf.events[from..].iter().any(|e| matches!(e, MobEvent::AttackMob { .. })) || rf.mobs[c].health < full {
            hit = true;
            break;
        }
    }
    assert!(hit, "the Mob charge hits the cow");
    rf.step(Some(&far));
    // The Mob hands the push to the host as a `Knockback` event (power 2.475 = 0.6 * 1.65 * 2.5, the jar's `getSpeed() * 1.65 * 2.5`).
    let mob_left = goat::ram_cooldown(&rf.mobs[g]);

    let mut h = Herd::with_slab_awake(60, false, false);
    let eg = ecs_at(&mut h, "minecraft:goat", 0.5, 1000);
    let ec = ecs_at(&mut h, "minecraft:cow", 9.5, 1001);
    let full = h.world.get::<Health>(ec).unwrap().0;
    let mut hit = false;
    for _ in 0..7000 {
        h.tick();
        if h.world.get::<Health>(ec).is_some_and(|x| x.0 < full) {
            hit = true;
            break;
        }
    }
    assert!(hit, "the ECS charge hits the cow");
    h.tick();
    let ecs_vx = h.world.get::<Vel>(ec).unwrap().0.x;
    let ecs_left = h.world.get::<RamCooldown>(eg).unwrap().0 as i64 - now_of(&h.world, eg) as i64;
    let ended = h.world.get::<Ramming>(eg).is_none();
    eprintln!("recoil: mob power {mob_power:.3} cooldown {mob_left}; ecs vx a tick later {ecs_vx:.3} cooldown {ecs_left}");
    assert!((mob_power - 2.475).abs() < 0.01, "the Mob's knockback power: {mob_power}");
    // One tick after the hit the ECS cow has the push less the tick's friction: positive (away from the goat) and below the power.
    assert!(ecs_vx > 0.5 * mob_power * 0.5 && ecs_vx <= mob_power, "the recoil: mob power {mob_power}, ecs vx {ecs_vx}");
    assert!((599..=6000).contains(&mob_left) && (599..=6000).contains(&ecs_left), "the cooldown after the charge: mob {mob_left}, ecs {ecs_left}");
    assert!(ended, "the ECS charge is over");
}

/// A prepare with no candidate (no creature, no player in reach) refuses at once: `PrepareRamNearestTarget.stop` sets the minimum
/// cooldown (`rams(m).0`, 600), on the same tick on both sides. Not covered: a target that dies or leaves during Prepare.
#[test]
fn a_goat_cooldown_after_a_refused_prepare_matches_the_mob() {
    if !goat_species() {
        return;
    }
    let mut rf = MobRef::new(60, false);
    mob_at(&mut rf, "minecraft:goat", 0.5);
    let g = rf.mobs.len() - 1;
    let mut mob_left = None;
    for _ in 0..7000 {
        let before = goat::ram_cooldown(&rf.mobs[g]);
        rf.step(None);
        let after = goat::ram_cooldown(&rf.mobs[g]);
        if before == 0 && after > 0 {
            mob_left = Some(after);
            break;
        }
    }
    let mob_left = mob_left.expect("the Mob refuses its prepare and sets a cooldown");

    let mut h = Herd::with_slab_awake(60, false, false);
    let eg = ecs_at(&mut h, "minecraft:goat", 0.5, 1000);
    // The first change is `finalize`'s draw at the spawn (the Mob goat here starts with none); the refusal is the second.
    let (mut ecs_left, mut changes) = (None, 0);
    for _ in 0..14000 {
        let (date, now) = (h.world.get::<RamCooldown>(eg).unwrap().0, now_of(&h.world, eg));
        h.tick();
        if h.world.get::<RamCooldown>(eg).unwrap().0 != date {
            changes += 1;
            if changes == 2 {
                ecs_left = Some(h.world.get::<RamCooldown>(eg).unwrap().0 as i64 - now as i64);
                break;
            }
        }
    }
    let ecs_left = ecs_left.expect("the ECS refuses its prepare and sets a cooldown");
    eprintln!("refused prepare: mob cooldown {mob_left}, ecs cooldown {ecs_left}");
    // Jar: `GoatAi.lambda$initRamActivity$4` gives `TIME_BETWEEN_RAMS.minInclusive()` (600) as the cooldown on failure; the Mob reads it a tick later.
    assert!((599..=600).contains(&mob_left), "the Mob minimum of RAMS: {mob_left}");
    assert!((599..=601).contains(&ecs_left), "the ECS minimum of RAMS: {ecs_left}");
}
