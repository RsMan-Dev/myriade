//! The step view's reads of a held entity come from its components, not from the game host.
use super::mod_goals::EcsCx;
use bevy_ecs::{entity::Entity, world::World};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    bridge::{host_read, ids::bind},
    cow::{Health, Pos, Vel, Yaw},
    glowing::Glowing,
    invulnerable::Invulnerable,
    nbt::Saved,
    nbt_tag,
    raids::Raider,
};
use mod_api::{EffectKey, EntityRef, Host, LeashHolder, LivingRef, MobRef, WorldEpoch};

/// A game host that answers nothing: a read served from the components would reach it and fail.
struct NoGame;

impl Host for NoGame {}

/// The step's view of `e`, which has network id `id` in epoch 1.
fn view<'a>(w: &'a mut World, e: Entity, id: u32, host: &'a dyn Host) -> EcsCx<'a> {
    bind(w, e, id);
    EcsCx::new(w, e, WorldEpoch::new(1), None, id, host)
}

#[test]
fn a_held_entity_reads_its_position_from_its_components() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::new(1.0, 64.0, 2.0)), Vel(Vec3::ZERO), Yaw(90.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    let motion = host_read::motion(cx.w, e).expect("an awake entity with a position");
    assert_eq!(motion.position, [1.0, 64.0, 2.0]);
}

#[test]
fn a_held_living_reads_its_health_from_its_components() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Health(12.5))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert_eq!(host_read::health(cx.w, e), Some(12.5));
}

/// `Entity.isCurrentlyGlowing` reads the `Glowing` marker that `setGlowingTag` sets: absent, the entity does not glow.
#[test]
fn a_held_entity_glows_when_it_carries_the_glowing_marker() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Glowing)).id();
    let plain = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    assert!(w.get::<Glowing>(e).is_some());
    assert!(w.get::<Glowing>(plain).is_none());
}

/// `Entity.isInvulnerableToPiercingWeapon` is true for the `Invulnerable` marker, the same source as `isInvulnerable`.
#[test]
fn a_held_entity_with_the_invulnerable_marker_resists_piercing_weapons() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Invulnerable)).id();
    assert!(w.get::<Invulnerable>(e).is_some());
}

/// `Entity.getTags` lists the command tags an entity carries, sorted, and nothing else.
#[test]
fn a_held_entity_lists_its_command_tags() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Saved)).id();
    assert!(nbt_tag::add(&mut w, e, "test:b"));
    assert!(nbt_tag::add(&mut w, e, "test:a"));
    assert_eq!(nbt_tag::names(&w, e), vec!["test:a", "test:b"]);
}

/// `Entity.getRaid` is the raid the `Raider` component names, and none for an entity outside a raid.
#[test]
fn a_held_raider_reads_its_raid_and_a_non_raider_has_none() {
    let mut w = World::new();
    let raider = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Raider { raid: Some(3), wave: 1, leader: false })).id();
    let outside = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    assert_eq!(w.get::<Raider>(raider).and_then(|r| r.raid), Some(3));
    assert!(w.get::<Raider>(outside).is_none());
}

/// `Entity.isInvulnerableToPiercingWeapon` answers true for the `Invulnerable` marker.
#[test]
fn the_view_answers_piercing_weapons_for_the_invulnerable_marker() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Invulnerable)).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert_eq!(cx.r_entity_invulnerable_to_piercing_weapon(EntityRef::new(WorldEpoch::new(1), 7)), Some(true));
}

/// `Entity.invulnerableTime` is 0 for a held entity with no invulnerability window.
#[test]
fn the_view_answers_no_invulnerable_time_without_a_window() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert_eq!(cx.r_entity_invulnerable_time(EntityRef::new(WorldEpoch::new(1), 7)), Some(0));
}

/// `Entity.isCrouching` is false for a held entity with no pose.
#[test]
fn the_view_answers_not_crouching_without_a_pose() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert_eq!(cx.r_entity_is_crouching(EntityRef::new(WorldEpoch::new(1), 7)), Some(false));
}

/// `Entity.getLeashHolder` is unleashed for a held entity without a `Leash`.
#[test]
fn the_view_answers_unleashed_without_a_leash() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert!(matches!(cx.r_entity_leash_holder(EntityRef::new(WorldEpoch::new(1), 7)), Some(LeashHolder::Unleashed)));
}

/// `TamableAnimal.isTame` is false for a held living entity without the tame flag.
#[test]
fn the_view_answers_not_tamed_without_the_tame_flag() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Health(20.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert_eq!(cx.r_living_is_tamed(LivingRef::new(EntityRef::new(WorldEpoch::new(1), 7))), Some(false));
}

/// `LivingEntity.getItemInHand` is none for an empty main hand: no equipment is held.
#[test]
fn the_view_answers_no_hand_item_for_an_empty_hand() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Health(20.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    let hand = mod_api::client::first_person::Hand::Main;
    assert!(cx.r_living_hand_item(LivingRef::new(EntityRef::new(WorldEpoch::new(1), 7)), hand).is_none());
}

/// `LivingEntity.hasLineOfSight` is a block ray the game answers: the held entity has no answer of its own.
#[test]
fn the_view_asks_the_game_for_line_of_sight() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Health(20.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    let r = EntityRef::new(WorldEpoch::new(1), 7);
    assert!(cx.r_living_has_line_of_sight(LivingRef::new(r), r).is_none());
}

/// `Raider.getCurrentRaid` needs the level the view holds: without it the raid is not named.
#[test]
fn the_view_names_no_raid_without_the_level() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Raider { raid: Some(3), wave: 1, leader: false })).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert!(cx.r_mob_raid(MobRef::new(EntityRef::new(WorldEpoch::new(1), 7))).is_none());
}

/// `Entity.pick` is a block ray the game answers: the held entity has no answer of its own.
#[test]
fn the_view_asks_the_game_for_the_pick() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert!(cx.r_entity_pick(EntityRef::new(WorldEpoch::new(1), 7), 4.0).is_none());
}

/// `Entity.getSoundSource` reads the species of a held entity; with no species the view answers none.
#[test]
fn the_view_answers_no_sound_source_without_a_species() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert!(cx.r_entity_sound_source(EntityRef::new(WorldEpoch::new(1), 7)).is_none());
}

/// `Entity.isFireImmune` reads the prepared mob types: none prepared, no answer.
#[test]
fn the_view_answers_no_fire_immunity_without_the_mob_types() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert!(cx.r_entity_fire_immune(EntityRef::new(WorldEpoch::new(1), 7)).is_none());
}

/// `Entity.portalCooldown` is the game's countdown: the held entity has no answer of its own.
#[test]
fn the_view_asks_the_game_for_the_portal_cooldown() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert!(cx.r_entity_portal_cooldown(EntityRef::new(WorldEpoch::new(1), 7)).is_none());
}

/// `Entity.getParts` of a dragon's parts is the game's: the held entity has no answer of its own.
#[test]
fn the_view_asks_the_game_for_the_parts() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert!(cx.r_entity_parts(EntityRef::new(WorldEpoch::new(1), 7)).is_none());
}

/// `LivingEntity.hasEffect` is false for a held living entity that carries no effect.
#[test]
fn the_view_answers_no_effect_without_effects() {
    let mut w = World::new();
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Health(20.0))).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    let effect = EffectKey::from_slot(0).expect("the first effect slot");
    assert!(!cx.r_living_potion_affected(LivingRef::new(EntityRef::new(WorldEpoch::new(1), 7)), effect));
}

/// The registry's effect ids have no namespace and the components' names have one: the held entity's poison is found by its index.
#[test]
fn the_view_finds_the_effect_a_held_entity_carries() {
    use horizon_ecs_proto::effects::{Effect, Effects, Kind};
    let index = |id: &str| horizon_client_core::potions::EFFECTS.iter().position(|d| d.id == id).expect("a vanilla effect");
    let mut w = World::new();
    let mut carried = Effects::default();
    carried.list.push(Effect { kind: Kind::Poison, amp: 0, ends: u64::MAX, next: u64::MAX, flags: 0 });
    let e = w.spawn((Pos(Vec3::ZERO), Vel(Vec3::ZERO), Yaw(0.0), Health(20.0), carried)).id();
    let host = NoGame;
    let cx = view(&mut w, e, 7, &host);
    assert!(cx.held_has_effect(e, index("poison")));
    assert!(!cx.held_has_effect(e, index("speed")));
}
