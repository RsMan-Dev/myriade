//! Lot 10 (`regles-mob-vers-ecs-10.md`): the rules of the old creature and piglin tests that had no ECS test. Values from the vanilla
//! 26.3 jar (`javap` of `Mob.asValidTarget`, `DamageSources.explosion`, `DamageSource`), not from the old Mob.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::world::leaked_test_world_mut;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::bridge::host::{EcsWorld, HostSetup, HurtSource, PlayerFact};
use horizon_ecs_proto::behaviours::hunt_prey::{CannotHunt, Hunt};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::{equipped, interact, Entered, Events, Online, Player, Slot, UnsafePlace};
use horizon_ecs_proto::behaviours::{admire::Admiring, breed::Love, brute_stroll_home::Home, Interaction};
use horizon_ecs_proto::brain::{behaviours::pacify::PACIFIED, set, MemValue};
use horizon_ecs_proto::equipment;
use horizon_ecs_proto::items;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::hostile::{HurtBy, PlayerState};
use horizon_ecs_proto::item::Item;
use horizon_ecs_proto::item_store::ItemStore;
use horizon_ecs_proto::loot;
use horizon_ecs_proto::nbt::{self, Saved};
use horizon_ecs_proto::nbt_fields::creatures;
use horizon_ecs_proto::projectile::{Blast, Projectile};
use horizon_ecs_proto::rules::admire_seen::{step, Step};
use horizon_ecs_proto::targeting::{Target, Who};

fn setup() -> HostSetup {
    HostSetup { seed: 7, dimension: "overworld", simulation_radius: 4 }
}

fn tags() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"))
}

fn player_far() -> Player {
    Player { id: 1, x: 10.5, y: 1.0, z: 0.5, ..Default::default() }
}

fn fact() -> PlayerFact {
    PlayerFact { alive: true, targetable: true, ..Default::default() }
}

/// The damage names of the `Hurt` events of one step.
fn hurt_names(out: &[HostEvent]) -> Vec<&'static str> {
    out.iter().filter_map(|ev| if let HostEvent::Hurt { damage, .. } = ev { Some(*damage) } else { None }).collect()
}

/// A creative or a spectator player is never a target of a monster (`Mob.asValidTarget` refuses both); the survival one is.
#[test]
fn creative_and_spectator_players_are_never_targets() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_player(player_far(), fact());
    h.step(mw);
    assert!(h.world().resource::<PlayerState>().targetable(1), "a survival player is a target");
    h.world_mut().resource_mut::<PlayerState>().set_creative(1, true);
    assert!(!h.world().resource::<PlayerState>().targetable(1), "a creative player is not");
    h.world_mut().resource_mut::<PlayerState>().set_creative(1, false);
    h.world_mut().resource_mut::<PlayerState>().spectator = vec![false, true];
    assert!(!h.world().resource::<PlayerState>().targetable(1), "a spectator is not");
}

/// `DamageSource` of a named type keeps its registry id, with or without an attacker (`/damage`, `HurtSource::Typed`).
#[test]
fn named_damage_keeps_its_registry_id_with_or_without_an_attacker() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    let victim = h.spawn(mw, "minecraft:cow", [0.5, 1.0, 0.5], None).expect("a known species");
    let other = h.spawn(mw, "minecraft:cow", [1.5, 1.0, 0.5], None).expect("a known species");
    for by in [None, Some(Who::Entity(other)), Some(Who::Player(1))] {
        h.hurt(mw, victim, 2.0, HurtSource::Typed { damage: "example:fan_fire", by, from: None });
        let out = h.step(mw);
        assert_eq!(hurt_names(&out), ["example:fan_fire"], "by {by:?}");
    }
}

/// `DamageSources.explosion`: a blast with a causing entity is a `player_explosion` for the creatures it hurts, else `explosion`.
#[test]
fn a_creeper_blast_is_a_player_explosion() {
    have_data();
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    h.set_player(player_far(), fact());
    h.step(mw);
    let source = h.spawn(mw, "minecraft:cow", [1.5, 1.0, 0.5], None).expect("a known species");
    h.spawn(mw, "minecraft:cow", [2.5, 1.0, 0.5], None).expect("a known species");
    let blast = |source| Blast { at: Vec3::new(1.5, 1.0, 0.5), radius: 3.0, fire: false, owner: None, source };
    h.explode(mw, blast(Some(source)));
    let named = hurt_names(&h.step(mw));
    assert!(!named.is_empty() && named.iter().all(|n| *n == "player_explosion"), "a caused blast: {named:?}");
    h.explode(mw, blast(None));
    let named = hurt_names(&h.step(mw));
    assert!(!named.is_empty() && named.iter().all(|n| *n == "explosion"), "an uncaused blast: {named:?}");
}

/// The `Hurt` of a blow through a projectile is not direct (the arrow is the direct entity, the owner the cause, `DamageSource`);
/// a melee blow of a mob is direct.
#[test]
fn a_projectile_blow_is_not_direct_and_a_melee_one_is() {
    have_data();
    let heard = |add: fn(&mut DamageQueue, Entity, Entity)| {
        let mut h = Herd::with_slab_awake(60, false, false);
        let victim = ecs_cow(&mut h, 0.5, 1.0, 0.5, 1);
        let attacker = ecs_cow(&mut h, 3.5, 1.0, 0.5, 2);
        add(&mut h.world.resource_mut::<DamageQueue>(), victim, attacker);
        h.tick();
        h.drain_host_out().into_iter().find_map(|ev| if let HostEvent::Hurt { direct, .. } = ev { Some(direct) } else { None })
    };
    assert_eq!(heard(|q, victim, attacker| q.add_via(victim, 2.0, (3.5, 0.5), Who::Entity(attacker), "minecraft:arrow")), Some(false));
    assert_eq!(heard(|q, victim, attacker| q.add_by_entity(victim, 2.0, (3.5, 0.5), attacker)), Some(true));
}

/// `AdmireItem` on the ground (`admire_seen`): it goes on while the item is closer than 9 blocks (81 squared), ends once the time is
/// over or the item is gone, and gives up with a walk-off wait when the animal is tired of trying.
#[test]
fn a_loved_item_in_sight_is_admired_until_too_far_or_too_long() {
    assert_eq!(step(10, 1000, 0, 0, 200, Some(25.0), 81.0), Step::Go, "in sight: admired");
    assert_eq!(step(10, 1000, 0, 0, 200, Some(81.0), 81.0), Step::End, "nine blocks away: over");
    assert_eq!(step(10, 1000, 0, 0, 200, None, 81.0), Step::End, "gone: over");
    assert_eq!(step(1000, 1000, 0, 0, 200, Some(25.0), 81.0), Step::End, "time over");
    assert_eq!(step(10, 1000, 5, 4, 200, Some(25.0), 81.0), Step::GiveUp { walk_off_until: 210 }, "tired of trying");
}

fn spawn_at(h: &mut Herd, name: &str, x: f64, seed: u64) -> Entity {
    let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, 0.5, seed);
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

fn skip_piglin() -> bool {
    !have_data() || wrong_species(&["minecraft:piglin"])
}

/// `HoglinAi.broadcastAttackTarget`: a hoglin hurt by a player calls the hoglin near it onto that player.
#[test]
fn a_hoglin_hurt_by_a_nearer_attacker_takes_it_as_target() {
    if skip_piglin() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let near = spawn_at(&mut h, "minecraft:hoglin", 3.5, 9);
    let hurt = spawn_at(&mut h, "minecraft:hoglin", 0.5, 7);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 6.5, y: 1.0, z: 0.5, ..Default::default() });
    h.world.resource_mut::<HurtBy>().0.push((hurt, Who::Player(1)));
    h.tick();
    h.tick();
    assert_eq!(h.world.get::<Target>(near).map(|t| t.who), Some(Who::Player(1)), "the hoglin nearby takes the attacker");
}

/// `PiglinBruteAi.wasHurtBy` is `PiglinAi.maybeRetaliate`: a brute hit by a player calls the adult piglins around it.
#[test]
fn a_brute_hit_by_a_player_sets_the_target_of_the_piglins_nearby() {
    if skip_piglin() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let piglin = spawn_at(&mut h, "minecraft:piglin", 3.5, 9);
    let brute = spawn_at(&mut h, "minecraft:piglin_brute", 0.5, 7);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 6.5, y: 1.0, z: 0.5, ..Default::default() });
    h.world.resource_mut::<HurtBy>().0.push((brute, Who::Player(1)));
    h.tick();
    h.tick();
    assert_eq!(h.world.get::<Target>(piglin).map(|t| t.who), Some(Who::Player(1)), "the piglin takes the player");
}

/// A piglin killed without a player drops the gold ingot it holds (`AbstractPiglin`, the admired item in the off hand).
#[test]
fn an_admired_piglin_drops_the_item_it_holds() {
    if skip_piglin() {
        return;
    }
    let (mw, types, _) = leaked_test_world_mut(true, |_, _| {}).expect("a prepared world");
    let mut h = EcsWorld::new(&setup(), mw, types, &tags());
    let piglin = h.spawn(mw, "minecraft:piglin", [0.5, 1.0, 0.5], None).expect("a known species");
    h.world_mut().get_mut::<ItemStore>(piglin).expect("an inventory").items[8] = Some(ItemStack::new("minecraft:gold_ingot", 1));
    loot::on_death(h.world_mut(), piglin);
    let world = h.world_mut();
    let mut q = world.query::<&Item>();
    let items: Vec<(String, u32)> = q.iter(world).map(|i| (i.stack.item.to_string(), i.stack.count as u32)).collect();
    assert!(items.contains(&("minecraft:gold_ingot".to_owned(), 1)), "{items:?}");
}

/// `CannotHunt` is written as a byte 1 and read back: a saved piglin that may not hunt stays so after a reload.
#[test]
fn a_saved_cannot_hunt_is_read_and_written_back() {
    let mut w = World::new();
    nbt::register(&mut w, creatures::FIELDS);
    let piglin = w.spawn((Saved, CannotHunt)).id();
    let saved = nbt::save(&w, piglin).unwrap();
    assert_eq!(saved.get(&JavaString::from("CannotHunt")), Some(&Tag::Byte(1)), "saved as a byte 1");
    let mut w2 = World::new();
    nbt::register(&mut w2, creatures::FIELDS);
    let loaded = w2.spawn(Saved).id();
    nbt::load(&mut w2, loaded, &saved);
    assert!(w2.get::<CannotHunt>(loaded).is_some(), "loaded back as unable to hunt");
}

/// The stacks of the `Item` entities on the ground: name and count.
fn dropped(w: &mut World) -> Vec<(String, u32)> {
    let mut q = w.query::<&Item>();
    q.iter(w).map(|i| (i.stack.item.to_string(), i.stack.count as u32)).collect()
}

/// `PiglinAi.initFightActivity`: a crossbow in the main hand makes the fight shoot an arrow at the attacker (`CrossbowAttack`); a golden sword does not.
#[test]
fn a_piglin_holding_a_crossbow_shoots_instead_of_meleeing() {
    if skip_piglin() {
        return;
    }
    for (item, shoots) in [("minecraft:crossbow", true), ("minecraft:golden_sword", false)] {
        let mut h = Herd::with_slab_awake(60, true, false);
        let p = spawn_at(&mut h, "minecraft:piglin", 0.5, 9);
        equipment::set_slot(&mut h.world, p, Slot::Main, items::item_id(item));
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 5.5, y: 1.0, z: 0.5, ..Default::default() });
        h.world.resource_mut::<HurtBy>().0.push((p, Who::Player(1)));
        let mut arrows = 0;
        for _ in 0..200 {
            h.tick();
            arrows = arrows.max(h.world.query::<&Projectile>().iter(&h.world).count());
        }
        assert_eq!(arrows > 0, shoots, "{item}: {arrows} arrows");
    }
}

/// `Piglin.finishConversion`: the inventory falls at the feet before the zombified piglin is made, and none of it goes with the new mob.
#[test]
fn a_converting_piglin_drops_its_inventory_before_it_converts() {
    if skip_piglin() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.world.insert_resource(UnsafePlace(|_, _| true));
    let p = spawn_at(&mut h, "minecraft:piglin", 0.5, 9);
    {
        let mut store = h.world.get_mut::<ItemStore>(p).expect("an inventory");
        store.items[0] = Some(ItemStack::new("minecraft:gold_nugget", 7));
        store.items[1] = Some(ItemStack::new("minecraft:gold_ingot", 3));
    }
    for _ in 0..400 {
        h.tick();
    }
    assert!(h.world.get::<Pos>(p).is_none(), "the piglin converted");
    let items = dropped(&mut h.world);
    assert!(items.contains(&("minecraft:gold_nugget".to_owned(), 7)), "{items:?}");
    assert!(items.contains(&("minecraft:gold_ingot".to_owned(), 3)), "{items:?}");
}

/// `PiglinAi.cancelAdmiring` (from `Piglin.finishConversion`): the admired item of the off hand falls at the feet, and the zombified piglin carries nothing.
#[test]
fn a_converted_piglin_drops_the_item_it_was_admiring() {
    if skip_piglin() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.world.insert_resource(UnsafePlace(|_, _| true));
    let p = spawn_at(&mut h, "minecraft:piglin", 0.5, 9);
    equipment::set_slot(&mut h.world, p, Slot::Off, items::item_id("minecraft:gold_ingot"));
    let until = now_of(&h.world, p) + 10_000;
    h.world.entity_mut(p).insert(Admiring { until });
    for _ in 0..400 {
        h.tick();
    }
    assert!(h.world.get::<Pos>(p).is_none(), "the piglin converted");
    let items = dropped(&mut h.world);
    assert!(items.contains(&("minecraft:gold_ingot".to_owned(), 1)), "{items:?}");
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let zombified: Vec<Entity> = h.world.query::<(Entity, &SpeciesId)>().iter(&h.world).filter(|(_, s)| table[s.0 as usize].name == "minecraft:zombified_piglin").map(|(e, _)| e).collect();
    assert_eq!(zombified.len(), 1, "one zombified piglin");
    assert_eq!(equipped(&h.world, zombified[0], Slot::Off), 0, "the new mob carries nothing");
}

/// `StrollToPoi(HOME, 0.6, 2, 100)`: a brute 30 blocks from home tries the way home, and the next try is set 80 ticks after it (`nextOkStartTime`).
#[test]
fn a_brute_tries_the_way_home_once_every_80_ticks() {
    if !have_data() || wrong_species(&["minecraft:piglin_brute"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let b = spawn_at(&mut h, "minecraft:piglin_brute", 0.5, 7);
    h.teleport(b, Vec3::new(30.5, 1.0, 0.5));
    let mut tries = Vec::new();
    let mut last = h.world.get::<Home>(b).unwrap().next_ok;
    for _ in 0..400 {
        let before = now_of(&h.world, b);
        h.tick();
        let next = h.world.get::<Home>(b).unwrap().next_ok;
        if next != last {
            let at = next - 80;
            assert!(at >= before && at <= now_of(&h.world, b), "the try is set on its own tick");
            tries.push(at);
            last = next;
        }
    }
    assert!(!tries.is_empty(), "the brute tries the way home");
}

/// `PiglinBruteAi.wasHurtBy` erases no home: a blow by a player leaves the `Home` of the brute as it was.
#[test]
fn a_hit_leaves_the_brutes_home_alone() {
    if !have_data() || wrong_species(&["minecraft:piglin_brute"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let b = spawn_at(&mut h, "minecraft:piglin_brute", 0.5, 7);
    let before = *h.world.get::<Home>(b).unwrap();
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 6.5, y: 1.0, z: 0.5, ..Default::default() });
    h.world.resource_mut::<HurtBy>().0.push((b, Who::Player(1)));
    h.tick();
    h.tick();
    let after = *h.world.get::<Home>(b).unwrap();
    assert_eq!((after.block, after.next_ok), (before.block, before.next_ok), "the home is kept");
}

/// `PACIFIED` alone refuses an adult its crimson fungus (`Hoglin.canFallInLove`), and a pacified baby still takes it to grow (a tenth of its time).
#[test]
fn the_pacified_memory_alone_refuses_an_adult_and_lets_a_baby_grow() {
    if !have_data() || wrong_species(&["minecraft:hoglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let adult = spawn_at(&mut h, "minecraft:hoglin", 0.5, 7);
    let baby = spawn_at(&mut h, "minecraft:hoglin", 3.5, 9);
    let fungus = items::item_id("minecraft:crimson_fungus");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: fungus, off: 0, ..Default::default() });
    set(&mut h.world, adult, &PACIFIED, MemValue::Bool(true), Some(200));
    set(&mut h.world, baby, &PACIFIED, MemValue::Bool(true), Some(200));
    h.world.entity_mut(baby).insert(GrowAt(now_of(&h.world, baby) + 24_000));
    let now = now_of(&h.world, baby);
    assert!(!interact(&mut h.world, adult, &Interaction { player: 1, item: fungus }), "a pacified adult refuses its food");
    assert!(h.world.get::<Love>(adult).is_none(), "and does not fall in love");
    assert!(interact(&mut h.world, baby, &Interaction { player: 1, item: fungus }), "a pacified baby takes its food");
    assert_eq!(h.world.get::<GrowAt>(baby).unwrap().0, now + 24_000 - 2_400, "a tenth of its time left");
}

/// `BecomePassiveIfMemoryPresent(NEAREST_REPELLENT)`: a warped fungus in reach makes the adult pacified and it refuses its food, while a
/// baby near it still grows; an adult out of reach of the fungus takes its food (the control).
#[test]
fn a_pacified_hoglin_refuses_breeding_food_and_a_baby_still_grows() {
    if !have_data() || wrong_species(&["minecraft:hoglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_block(3, 1, 0, "minecraft:warped_fungus");
    let adult = spawn_at(&mut h, "minecraft:hoglin", 0.5, 7);
    let baby = spawn_at(&mut h, "minecraft:hoglin", -2.5, 9);
    let control = spawn_at(&mut h, "minecraft:hoglin", -20.5, 11);
    let fungus = items::item_id("minecraft:crimson_fungus");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: fungus, off: 0, ..Default::default() });
    for _ in 0..20 {
        h.tick();
    }
    assert!(!interact(&mut h.world, adult, &Interaction { player: 1, item: fungus }), "the pacified adult refuses its food");
    assert!(h.world.get::<Love>(adult).is_none(), "and does not fall in love");
    assert!(interact(&mut h.world, control, &Interaction { player: 1, item: fungus }), "the control takes its food");
    h.world.entity_mut(baby).insert(GrowAt(now_of(&h.world, baby) + 24_000));
    let now = now_of(&h.world, baby);
    assert!(interact(&mut h.world, baby, &Interaction { player: 1, item: fungus }), "the pacified baby takes its food");
    assert_eq!(h.world.get::<GrowAt>(baby).unwrap().0, now + 24_000 - 2_400, "a tenth of its time left");
}

/// `BecomePassiveIfMemoryPresent.create(NEAREST_REPELLENT, 200)`: `PACIFIED` outlives the repellent by its 200 ticks. Away from the fungus the
/// adult still refuses its food, then it breeds again, 181 to 201 ticks after it left.
#[test]
fn pacification_outlives_the_repellent_by_200_ticks() {
    if !have_data() || wrong_species(&["minecraft:hoglin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    h.set_block(3, 1, 0, "minecraft:warped_fungus");
    let adult = spawn_at(&mut h, "minecraft:hoglin", 0.5, 7);
    let fungus = items::item_id("minecraft:crimson_fungus");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: fungus, off: 0, ..Default::default() });
    for _ in 0..20 {
        h.tick();
    }
    assert!(!interact(&mut h.world, adult, &Interaction { player: 1, item: fungus }), "pacified by the fungus");
    h.teleport(adult, Vec3::new(40.5, 1.0, 0.5));
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 40.5, y: 1.0, z: 2.5, main: fungus, off: 0, ..Default::default() });
    let mut accepted = None;
    for t in 1..=260 {
        h.tick();
        if accepted.is_none() && interact(&mut h.world, adult, &Interaction { player: 1, item: fungus }) {
            accepted = Some(t);
        }
    }
    let t = accepted.expect("the adult breeds again");
    assert!((181..=201).contains(&t), "{t} ticks after leaving the fungus");
}

/// The brute growls (`entity.piglin_brute.angry`) of the host events of one tick.
fn growls(h: &mut Herd) -> usize {
    h.world
        .resource_mut::<Events>()
        .host
        .drain()
        .filter(|ev| matches!(ev, HostEvent::Sound { name, .. } if *name == "minecraft:entity.piglin_brute.angry"))
        .count()
}

/// `PiglinBruteAi.updateActivity`: a brute hit by a player takes the fight, its active non-core activity.
#[test]
fn a_brute_with_a_target_selects_the_fight_activity() {
    if !have_data() || wrong_species(&["minecraft:piglin_brute"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let b = spawn_at(&mut h, "minecraft:piglin_brute", 0.5, 7);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 6.5, y: 1.0, z: 0.5, ..Default::default() });
    h.world.resource_mut::<HurtBy>().0.push((b, Who::Player(1)));
    h.tick();
    assert!(horizon_ecs_proto::brain::is_active(&h.world, b, "fight"), "the brute fights");
}

/// `PiglinBruteAi.playActivitySound`: in its fight the brute growls `entity.piglin_brute.angry`, drawn at 0.0125 a tick (2 000 ticks: many draws).
#[test]
fn a_brute_names_the_brute_growl() {
    if !have_data() || wrong_species(&["minecraft:piglin_brute"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let b = spawn_at(&mut h, "minecraft:piglin_brute", 0.5, 7);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 6.5, y: 1.0, z: 0.5, ..Default::default() });
    h.world.resource_mut::<HurtBy>().0.push((b, Who::Player(1)));
    let mut n = 0;
    for _ in 0..2_000 {
        h.tick();
        n += growls(&mut h);
    }
    assert!(n > 0, "the fighting brute growls");
}

/// `PiglinBruteAi.maybePlayActivitySound`: the roll sounds only in a fight; a brute that is not fighting stays silent over the same 2 000 ticks.
#[test]
fn the_growl_roll_draws_every_tick_and_only_sounds_in_a_fight() {
    if !have_data() || wrong_species(&["minecraft:piglin_brute"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    spawn_at(&mut h, "minecraft:piglin_brute", 0.5, 7);
    let mut n = 0;
    for _ in 0..2_000 {
        h.tick();
        n += growls(&mut h);
    }
    assert_eq!(n, 0, "no fight, no growl");
}

/// `PiglinAi.pickUpItem` food branch: a piglin that walks onto food eats one piece (`ATE_RECENTLY` for 200 ticks) and takes no other food
/// meanwhile; its hunt timer and its shield are left as they were.
#[test]
fn eating_does_not_reset_the_hunt_timer_nor_drop_the_shield() {
    if skip_piglin() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let p = spawn_at(&mut h, "minecraft:piglin", 0.5, 9);
    equipment::set_slot(&mut h.world, p, Slot::Off, items::item_id("minecraft:shield"));
    h.world.entity_mut(p).insert(Hunt { p: &horizon_ecs_proto::behaviours::hunt_prey::Params::DEFAULT, until: 5_000 });
    let mut rng = horizon_core::random::LegacyRandom::new(1);
    for _ in 0..2 {
        horizon_ecs_proto::item::spawn_at_location(&mut h.world, ItemStack::new("minecraft:porkchop", 1), Vec3::new(5.5, 1.0, 0.5), &mut rng);
    }
    let porkchops = |w: &mut World| dropped(w).iter().filter(|(n, _)| n == "minecraft:porkchop").map(|(_, c)| c).sum::<u32>();
    h.teleport(p, Vec3::new(5.5, 1.0, 0.5));
    h.tick();
    assert_eq!(porkchops(&mut h.world), 1, "one piece is eaten");
    assert!(h.world.get::<horizon_ecs_proto::behaviours::piglin_eat::Ate>(p).is_some(), "the piglin ate");
    assert_eq!(h.world.get::<Hunt>(p).map(|x| x.until), Some(5_000), "the hunt timer is kept");
    assert_eq!(equipped(&h.world, p, Slot::Off), items::item_id("minecraft:shield"), "the shield is kept");
    h.teleport(p, Vec3::new(0.5, 1.0, 0.5));
    h.teleport(p, Vec3::new(5.5, 1.0, 0.5));
    h.tick();
    assert_eq!(porkchops(&mut h.world), 1, "no second piece within 200 ticks");
    for _ in 0..200 {
        h.tick();
    }
    h.teleport(p, Vec3::new(0.5, 1.0, 0.5));
    h.teleport(p, Vec3::new(5.5, 1.0, 0.5));
    h.tick();
    assert_eq!(porkchops(&mut h.world), 0, "the second piece is eaten after 200 ticks");
}
