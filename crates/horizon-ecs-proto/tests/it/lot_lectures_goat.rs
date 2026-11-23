//! The goat's ram against the `GoatAi` rules the ECS left out: a goat in love beside an adult in love does not ram
//! (`PrepareRamNearestTarget.canUse`, `AnimalMakeLove`), and a screaming goat cools down for `RAMS_SCREAMER` (100 to 300
//! ticks) after its charge, where the others take the countdown of their parameters (50 here). Written without a run.
use crate::common::*;
use horizon_ecs_proto::{
    behaviours::{breed::Love, ram, Entry},
    cow::*,
    goals::{arm_timers, set_lazy_look, GoalSet},
    species::{goat, goat::Screaming, Config},
};

static ENTRIES: [Entry; 1] = [Entry::new(&ram::MODULE, 2, &ram::Params { cooldown: (50, 50), marked: Some(goat::SCREAMING_RAM), ..ram::Params::DEFAULT })];

/// A goat (a cow with the ram alone) at x = 0.5, a prey pig at `px` as its target, and an optional goat partner at x = 2.5 in love.
fn scene(h: &mut Herd, px: f64, partner: bool) -> (bevy_ecs::entity::Entity, bevy_ecs::entity::Entity) {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let mut goat = cow;
    goat.name = "minecraft:goat";
    goat.goals = Config { entries: &ENTRIES };
    let mut prey_sp = *table.iter().find(|s| s.name == "minecraft:pig").unwrap();
    prey_sp.goals = Config { entries: &[] };
    let g = spawn_species(&mut h.world, &goat, 0.5, 0.5, 51);
    let p = spawn_species(&mut h.world, &prey_sp, px, 0.5, 52);
    let mut all = vec![g, p];
    if partner {
        let q = spawn_species(&mut h.world, &goat, 2.5, 0.5, 53);
        all.push(q);
        h.world.entity_mut(q).insert(Love(u64::MAX));
        h.world.entity_mut(g).insert(Love(u64::MAX));
    }
    h.world.flush();
    for &e in &all {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    }
    h.world.get_mut::<AttackDamage>(g).unwrap().0 = 2.0;
    h.world.get_mut::<Health>(p).unwrap().0 = 10.0;
    arm_timers(&mut h.world, g);
    (g, p)
}

fn running(h: &Herd, e: bevy_ecs::entity::Entity) -> bool {
    h.world.get::<GoalSet>(e).unwrap().is_running(&ram::MODULE)
}

/// Two goats in love and an adult of their kind beside one: the cooldown is over, yet no ram starts for 120 ticks. Its control, the same
/// goat alone, starts its ram within 60 ticks (the cooldown is 50).
#[test]
fn a_goat_in_love_beside_an_adult_in_love_starts_no_ram() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(30, false, false);
    set_lazy_look(&mut h.world, false);
    let (g, _p) = scene(&mut h, 6.5, true);
    let mut started = false;
    for _ in 0..120 {
        h.tick();
        started |= running(&h, g);
    }
    assert!(!started, "a goat being bred starts no ram in 120 ticks");

    let mut h = Herd::with_slab_awake(30, false, false);
    set_lazy_look(&mut h.world, false);
    let (g, _p) = scene(&mut h, 6.5, false);
    let started = (0..60).any(|_| {
        h.tick();
        running(&h, g)
    });
    assert!(started, "alone, the goat rams once its 50-tick cooldown is over");
}

/// A screaming goat's charge sets a cooldown of 100 to 300 ticks; a goat that does not scream sets one of 50 (the parameter's).
#[test]
fn a_screaming_goat_cools_down_for_100_to_300_ticks_after_its_charge() {
    if !have_data() {
        return;
    }
    let cooldown_after_a_hit = |screaming: bool| {
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        let (g, p) = scene(&mut h, 6.5, false);
        if screaming {
            h.world.entity_mut(g).insert(Screaming);
        }
        let mut now = 0;
        for _ in 0..400 {
            now = now_of(&h.world, g);
            h.tick();
            if h.world.get::<Health>(p).unwrap().0 < 10.0 {
                break;
            }
        }
        assert!(h.world.get::<Health>(p).unwrap().0 < 10.0, "the charge hits the prey");
        let date = h.world.get::<ram::RamCooldown>(g).unwrap().0;
        date - now
    };
    let plain = cooldown_after_a_hit(false);
    let screamer = cooldown_after_a_hit(true);
    assert_eq!(plain, 50, "a goat that does not scream waits the parameter's 50 ticks");
    assert!((100..=300).contains(&screamer), "a screaming goat waits 100 to 300 ticks: {screamer}");
}

/// The Mob's horns are lost one goat in ten at birth (`goat_spawn`, `LOST_HORN_CHANCE` 0.1 here): 5000 adults drawn, the share of
/// those with a snapped horn (a `Shed` part) must stay near 0.1. A grown-up kid (`GrowAt`) keeps both, so only adults are counted.
#[test]
fn about_one_goat_in_ten_is_born_with_a_single_horn() {
    if !have_data() || wrong_species(&["minecraft:goat"]) {
        return;
    }
    let mut h = Herd::new_awake(0, 6.0, true, false, 3);
    let sp = *species();
    const BORN: u64 = 5000;
    let single = (0..BORN).filter(|&seed| {
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 19000 + seed);
        h.world.get::<ram::Shed>(e).is_some()
    });
    let share = single.count() as f64 / BORN as f64;
    eprintln!("goats born with one horn: {share:.4} of {BORN}");
    // The chance is 0.1: 5000 draws give 500 on average, a binomial deviation of about 21.
    assert!((share - 0.1).abs() < 0.03, "{share}");
}

/// `RamTarget.tick` (jar): a hit on a creature knocks it back and ends the ram; `Goat.dropHorn` is reached only through `hasRammedHornBreakingBlock`.
/// So a ram on prey drops no horn and snaps no side.
#[test]
fn a_ram_that_hits_a_creature_drops_no_horn() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(30, false, false);
    set_lazy_look(&mut h.world, false);
    let (g, p) = scene(&mut h, 6.5, false);
    h.world.entity_mut(g).insert(ram::Shed(0));
    let items = |h: &mut Herd| h.world.query::<&horizon_ecs_proto::item::Item>().iter(&h.world).count();
    let before = items(&mut h);
    for _ in 0..400 {
        h.tick();
        if h.world.get::<Health>(p).unwrap().0 < 10.0 {
            break;
        }
    }
    assert!(h.world.get::<Health>(p).unwrap().0 < 10.0, "the charge hits the prey");
    assert_eq!(items(&mut h), before, "no horn drops on a creature hit");
    assert_eq!(h.world.get::<ram::Shed>(g).map_or(0, |s| s.0), 0, "no side is snapped");
}
