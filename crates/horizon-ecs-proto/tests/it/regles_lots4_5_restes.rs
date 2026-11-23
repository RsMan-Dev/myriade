//! Restes des lots 4 et 5 des règles du Mob : la marche d'un chat (`Cat.registerGoals`), l'assise sur un coffre, la neige poudreuse
//! (debout pour un loup-bottes et un renard, enfoncement d'une vache, atterrissage d'un cochon), le gémissement d'un loup apprivoisé
//! sous 20 de vie, le compte de bail d'un gardien tenu en laisse, et l'écrasement d'une culture par classe. Les valeurs attendues
//! viennent du jar 26.3 (`javap`) ou des tests d'origine.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_ecs_proto::behaviours::act_phase::ActPhase;
use horizon_ecs_proto::behaviours::breed::{self, Love};
use horizon_ecs_proto::behaviours::sit_when_ordered::{self, SitPose};
use horizon_ecs_proto::behaviours::{cat_sit_on_block, same, stroll, trample, Entry};
use horizon_ecs_proto::cow::{now_of, spawn_species, ActiveUntil, Health, Herd, Pos, SpeciesTable};
use horizon_ecs_proto::equip_stack;
use horizon_ecs_proto::goals::{is_occupied, set_lazy_look, slot_of, Entered, Events, GoalSet, Look, Req, Requests, OCC_SITTING};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::leash::{self, Holder};
use horizon_ecs_proto::level::block_has_class;
use horizon_ecs_proto::species::Config;
use horizon_ecs_proto::tame::{self, Owner};
use horizon_ecs_proto::targeting::Who;

/// The species of the table by name, spawned at (x, 0.5, z) with a seed.
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    e
}

/// The water-avoiding chance of the stroll goal of a creature (the cat's `Cat.registerGoals` value, 1/100000).
fn stroll_water_avoiding(h: &Herd, e: Entity) -> Option<f32> {
    let goals = h.world.get::<GoalSet>(e).unwrap();
    goals
        .entries()
        .iter()
        .filter(|en| same(en.module, &stroll::MODULE))
        .find_map(|en| en.params.downcast_ref::<stroll::Params>())
        .map(|p| p.water_avoiding)
}

#[test]
fn a_cats_stroll_is_one_tick_in_a_hundred_thousand() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let cat = spawn(&mut h, "minecraft:cat", 0.5, 0.5);
    assert_eq!(stroll_water_avoiding(&h, cat), Some(1.000_000_1e-5), "Cat.registerGoals : stroll.water_avoiding = 1/100000");
}

/// A tame animal with the sit-on-block goal alone, awake, at (0.5, 0.5) on the slab (the module's own test world).
fn sitting_world() -> (Herd, Entity) {
    let mut h = Herd::with_slab_awake(200, false, false);
    set_lazy_look(&mut h.world, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == "minecraft:wolf").unwrap();
    let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&cat_sit_on_block::MODULE, 7, &cat_sit_on_block::Params::DEFAULT)]));
    sp.goals = Config { entries };
    let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
    h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(c);
    tame::set_owner(&mut h.world, c, Who::Player(1));
    h.tick();
    (h, c)
}

/// A chest is a place to sit as a lit furnace is: the tame animal walks to the chest five blocks away (the walk ends one block short).
#[test]
fn a_chest_is_a_place_to_sit_on_like_a_lit_furnace() {
    if !have_data() {
        return;
    }
    let (mut h, c) = sitting_world();
    h.set_block(5, 0, 0, "minecraft:chest");
    for _ in 0..800 {
        h.tick();
    }
    let x = h.world.get::<Pos>(c).unwrap().0.x;
    assert!(x > 3.5, "the animal walks to the chest (x {x})");
}

/// Powder snow: a fox and a zombie in leather boots stand on the column's top (6.0), a cow sinks into it. The feet slot is 2.
#[test]
fn a_fox_and_a_booted_zombie_stand_on_powder_snow_and_a_cow_sinks() {
    if !have_data() {
        return;
    }
    fn y_on_snow(name: &str, boots: bool) -> f64 {
        let mut h = Herd::with_slab_awake(8, true, false);
        for y in 1..=5 {
            h.set_block(0, y, 0, "minecraft:powder_snow");
        }
        let e = spawn(&mut h, name, 0.5, 0.5);
        h.world.get_mut::<Pos>(e).unwrap().0.y = 7.0;
        if boots {
            assert!(equip_stack::set(&mut h.world, e, 2, Some(ItemStack::new("minecraft:leather_boots", 1))), "the boots are worn");
        }
        for _ in 0..60 {
            h.tick();
        }
        h.world.get::<Pos>(e).unwrap().0.y
    }
    let fox = y_on_snow("minecraft:fox", false);
    let booted = y_on_snow("minecraft:zombie", true);
    let cow = y_on_snow("minecraft:cow", false);
    assert!(fox > 5.5, "a fox stands on powder snow (y = {fox})");
    assert!(booted > 5.5, "a zombie in leather boots stands on powder snow (y = {booted})");
    assert!(cow < 5.5, "a cow sinks into powder snow (y = {cow})");
}

/// A pig falling onto a powder snow block over stone rests 0.9 above the block's bottom (the original's 64 + 0.9 with the snow at 64).
#[test]
fn a_pig_landing_on_powder_snow_rests_at_its_0_9_top() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(0, 0, 0, "minecraft:stone");
    h.set_block(0, 1, 0, "minecraft:powder_snow");
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    h.world.get_mut::<Pos>(pig).unwrap().0.y = 6.0;
    for _ in 0..80 {
        h.tick();
    }
    let y = h.world.get::<Pos>(pig).unwrap().0.y;
    assert!((y - (1.0 + f64::from(0.9_f32))).abs() < 1e-6, "the pig rests at 1 + 0.9 (y = {y})");
}

/// The sounds a wolf made in the tick, from the host events.
fn wolf_sounds(h: &mut Herd) -> Vec<String> {
    h.world
        .resource_mut::<Events>()
        .host
        .drain()
        .filter_map(|e| match e {
            HostEvent::Sound { name, .. } if name.starts_with("entity.wolf.") => Some(name.to_string()),
            _ => None,
        })
        .collect()
}

/// The sounds of a tame wolf at this health over 2400 ticks.
fn tame_wolf_sounds(health: f32) -> Vec<String> {
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = spawn(&mut h, "minecraft:wolf", 0.5, 0.5);
    h.world.entity_mut(e).insert((Owner(Some(Who::Player(1))), Health(health)));
    let mut heard = Vec::new();
    for _ in 0..2400 {
        h.tick();
        heard.extend(wolf_sounds(&mut h));
    }
    heard
}

/// `Wolf.getAmbientSound`: under 20 health a tame wolf whines (never pants); at 20 it never whines.
#[test]
fn a_tame_wolf_whines_under_twenty_health_whatever_its_max() {
    if !have_data() {
        return;
    }
    let hurt = tame_wolf_sounds(19.0);
    assert!(hurt.iter().any(|s| s == "entity.wolf.whine"), "a tame wolf under 20 health whines: {hurt:?}");
    assert!(hurt.iter().all(|s| s != "entity.wolf.pant"), "a tame wolf under 20 health never pants: {hurt:?}");
    let full = tame_wolf_sounds(20.0);
    assert!(full.iter().all(|s| s != "entity.wolf.whine"), "a tame wolf at 20 health never whines: {full:?}");
}

/// A warden on a leash keeps its dig cooldown at 1200 ticks, as a persistent one does (`Warden.tick`, `WardenAi.setDigCooldown`);
/// a free warden keeps none.
#[test]
fn a_leashed_warden_keeps_its_dig_cooldown_at_1200_ticks() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let free = spawn(&mut h, "minecraft:warden", 0.5, 0.5);
    let leashed = spawn(&mut h, "minecraft:warden", 4.5, 0.5);
    assert!(leash::attach_from_save(&mut h.world, leashed, Holder::Knot([10, 0, 0])), "the warden is leashed");
    h.tick();
    let now = now_of(&h.world, leashed);
    assert_eq!(h.world.get::<ActPhase>(leashed).unwrap().dig_cooldown_until, now + 1200, "a leashed warden restarts its cooldown");
    assert_eq!(h.world.get::<ActPhase>(free).unwrap().dig_cooldown_until, 0, "a free warden keeps no cooldown");
}

/// A ravager walking onto a crop breaks it by its class (`CropBlock`, here beetroots, whose name is not a crop's), as `Ravager` does.
#[test]
fn a_ravager_crushes_a_crop_by_its_class() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(1, 1, 0, "minecraft:beetroots");
    let ravager = spawn(&mut h, "minecraft:ravager", 0.5, 0.5);
    h.world.entity_mut(ravager).insert(trample::Trample(&trample::Params::DEFAULT));
    let p = [1, 1, 0];
    assert!(block_has_class(&h.world, p, &["CropBlock"]), "témoin : la betterave est une CropBlock");
    trample::on_enter_cell(&mut h.world, ravager, p);
    assert!(!block_has_class(&h.world, p, &["CropBlock"]), "the ravager breaks the crop it walks onto");
}

/// A tame wolf ordered to sit, on land or in water (a water cell under its feet), ticked a few times.
fn ordered_wolf(water: bool) -> (Herd, Entity) {
    let mut h = Herd::with_slab_awake(8, true, false);
    if water {
        h.set_block(0, 1, 0, "minecraft:water");
    }
    let e = spawn(&mut h, "minecraft:wolf", 0.5, 0.5);
    tame::set_owner(&mut h.world, e, Who::Player(1));
    tame::order_sit(&mut h.world, e, true);
    for _ in 0..3 {
        h.tick();
    }
    (h, e)
}

/// `TamableAnimal.isOrderedToSit` stays set in water, but the sitting pose (`isInSittingPose`) is not taken there.
#[test]
fn ordering_a_pet_to_sit_in_water_keeps_the_order_and_no_pose() {
    if !have_data() {
        return;
    }
    let (h, e) = ordered_wolf(true);
    assert!(is_occupied(&h.world, e, OCC_SITTING), "the order to sit stays in water");
    assert!(h.world.get::<SitPose>(e).is_none(), "no sitting pose in water");
}

/// On land the ordered pet takes the sitting pose, and it leaves the pose when the sit goal stops (the order stays).
#[test]
fn an_ordered_pet_takes_the_sitting_pose_on_land_and_drops_it_when_the_goal_stops() {
    if !have_data() {
        return;
    }
    let (mut h, e) = ordered_wolf(false);
    assert!(h.world.get::<SitPose>(e).is_some(), "the sitting pose is taken on land");
    let slot = slot_of(&h.world, e, &sit_when_ordered::MODULE).expect("the sit goal is in the wolf's configuration");
    h.world.resource_mut::<Requests>().0.push(Req::End(e, slot));
    h.tick();
    assert!(h.world.get::<SitPose>(e).is_none(), "the pose is dropped when the goal stops");
    assert!(is_occupied(&h.world, e, OCC_SITTING), "the order to sit is still there");
}

/// `Wolf.getMaxHeadXRot`: a sitting wolf's head tilts 20 degrees at most (the look's default limit), 40 once it gets up.
#[test]
fn a_sitting_wolf_limits_its_head_pitch_to_twenty() {
    if !have_data() {
        return;
    }
    let (mut h, e) = ordered_wolf(false);
    assert_eq!(h.world.get::<Look>(e).unwrap().x_default, 20.0, "the sitting wolf's head limit");
    let slot = slot_of(&h.world, e, &sit_when_ordered::MODULE).expect("the sit goal is in the wolf's configuration");
    h.world.resource_mut::<Requests>().0.push(Req::End(e, slot));
    h.tick();
    assert_eq!(h.world.get::<Look>(e).unwrap().x_default, 40.0, "the head limit is back at 40 once it stops sitting");
}

/// `Wolf.canMate`: a sitting wolf refuses a mate; a sitting cat does not (`Cat` keeps `TamableAnimal.canMate`).
#[test]
fn a_sitting_wolf_refuses_a_mate_and_a_sitting_cat_does_not() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let (a, b) = (spawn(&mut h, "minecraft:wolf", 0.5, 0.5), spawn(&mut h, "minecraft:wolf", 2.5, 0.5));
    for e in [a, b] {
        h.world.entity_mut(e).insert(Love(u64::MAX));
    }
    assert!(breed::can_mate(&h.world, a, b), "two wolves in love mate");
    tame::set_owner(&mut h.world, a, Who::Player(1));
    tame::order_sit(&mut h.world, a, true);
    for _ in 0..3 {
        h.tick();
    }
    assert!(!breed::can_mate(&h.world, a, b), "a sitting wolf refuses a mate");
    let (c, d) = (spawn(&mut h, "minecraft:cat", 4.5, 0.5), spawn(&mut h, "minecraft:cat", 6.5, 0.5));
    for e in [c, d] {
        h.world.entity_mut(e).insert(Love(u64::MAX));
    }
    tame::set_owner(&mut h.world, c, Who::Player(1));
    tame::order_sit(&mut h.world, c, true);
    for _ in 0..3 {
        h.tick();
    }
    assert!(breed::can_mate(&h.world, c, d), "a sitting cat still mates");
}
