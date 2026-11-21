//! The passive entities against their originals. A display keeps the data it was given and one change emits one update (the
//! `Display` data of `addAdditionalSaveData`). An interaction records its last click and attack as `Interaction` does (the jar's
//! `skipAttackInteraction` and `interact`). A kind the ECS does not simulate comes back with its data. An area effect cloud grows
//! and ends tick by tick, on the same tick as the client's `tick_potion_cloud` (`items/potions.rs`), transcribed below.
use bevy_ecs::prelude::*;
use horizon_core::{geometry::Vec3, nbt::Tag};
use horizon_ecs_proto::{
    cow::Pos,
    display::{self, BlockState, Shown, Transform},
    effect_cloud::{self, EffectCloud},
    effects::PotionId,
    goals::Events,
    host_out::HostEvent,
    interaction,
    passive::{self, Act, Interact},
};
use horizon_world::registry::BlockStateId;

fn world() -> World {
    let mut w = World::new();
    w.init_resource::<Events>();
    w.init_resource::<horizon_ecs_proto::goals::Online>();
    w.init_resource::<horizon_ecs_proto::hostile::PlayerState>();
    w.init_resource::<horizon_ecs_proto::cow::Index>();
    horizon_ecs_proto::nbt::register(&mut w, passive::FIELDS);
    w
}
fn at() -> Vec3 {
    Vec3::new(10.0, 64.0, -3.0)
}
/// The display updates written since the last call.
fn updates(w: &mut World) -> usize {
    w.resource_mut::<Events>().host.drain().filter(|e| matches!(e, HostEvent::DisplayUpdate { .. })).count()
}
/// The last attack and the last use of an interaction.
fn acts(w: &World, e: Entity) -> (Option<Act>, Option<Act>) {
    let i = w.get::<Interact>(e).unwrap();
    (i.attack.as_deref().copied(), i.interaction.as_deref().copied())
}

#[test]
fn a_display_reads_back_its_data_and_one_change_emits_one_update() {
    let mut w = world();
    let t = Transform { translation: [0.5, -1.0, 2.0], left_rotation: [0.0, 0.7071, 0.0, 0.7071], right_rotation: [0.0, 0.0, 0.0, 1.0], scale: [2.0; 3] };
    let s = Shown { interpolation_delay: 3, interpolation_duration: 5, billboard: 3, brightness: 0x00F0_00F0, view_range: 0.5, ..Shown::default() };
    let e = display::spawn(&mut w, at(), t, s, BlockState(BlockStateId(7)));
    assert_eq!(*w.get::<Transform>(e).unwrap(), t);
    assert_eq!(*w.get::<Shown>(e).unwrap(), s);
    assert_eq!(w.get::<BlockState>(e).unwrap().0, BlockStateId(7));
    assert_eq!(updates(&mut w), 0, "the summon emits nothing");

    let moved = Transform { translation: [1.0, 0.0, 0.0], ..t };
    assert!(display::set(&mut w, e, moved));
    assert_eq!(*w.get::<Transform>(e).unwrap(), moved);
    assert_eq!(updates(&mut w), 1, "one change, one update");
    assert!(!display::set(&mut w, e, moved), "the same value changes nothing");
    assert_eq!(updates(&mut w), 0);
}

#[test]
fn an_interaction_records_its_last_click_and_attack_as_the_jar_does() {
    let mut w = world();
    let e = interaction::spawn(&mut w, at(), 1.5, 2.0, false);
    let i = w.get::<Interact>(e).unwrap();
    assert_eq!((i.width, i.height, i.response), (1.5, 2.0, false));

    let p = [1, -2, 3, -4];
    assert!(interaction::attack(&mut w, e, p, 100), "no response: the blow is skipped");
    interaction::click(&mut w, e, [9, 9, 9, 9], 105);
    assert_eq!(acts(&w, e), (Some(Act { player: p, timestamp: 100 }), Some(Act { player: [9, 9, 9, 9], timestamp: 105 })));

    let r = interaction::spawn(&mut w, at(), 1.0, 1.0, true);
    assert!(!interaction::attack(&mut w, r, p, 7), "a responding interaction lets the blow through");
    assert_eq!(acts(&w, r), (Some(Act { player: p, timestamp: 7 }), None));

    interaction::attack(&mut w, e, [5, 5, 5, 5], 110);
    assert_eq!(acts(&w, e).0, Some(Act { player: [5, 5, 5, 5], timestamp: 110 }), "a later attack replaces the record");
}

#[test]
fn an_unsimulated_entity_comes_back_with_its_data() {
    let mut w = world();
    let data = Tag::compound([("id", Tag::String("custom:thing".into())), ("custom", Tag::String("kept".into()))]);
    let e = passive::spawn(&mut w, "custom:thing", at(), data.clone());
    assert_eq!(passive::save(&w, e), data);
    assert_eq!(w.get::<Pos>(e).unwrap().0.y, 64.0);
    assert_eq!(passive::save(&w, e).get("custom"), Some(&Tag::String("kept".into())));
}

/// `AreaEffectCloud` as the client ticks it (`items/potions.rs`, `tick_potion_cloud` and the radius part of it).
#[derive(Clone, Copy)]
struct Origin {
    age: i32,
    radius: f32,
    per_tick: f32,
    wait: i32,
    duration: i32,
}
/// One tick: false once the cloud is gone.
fn origin_tick(o: &mut Origin) -> bool {
    o.age = o.age.wrapping_add(1);
    if o.duration != -1 && o.age.wrapping_sub(o.wait) >= o.duration {
        return false;
    }
    if o.age < o.wait {
        return true;
    }
    if o.per_tick != 0.0 {
        o.radius += o.per_tick;
        if o.radius < 0.5 {
            return false;
        }
        o.radius = o.radius.clamp(0.0, 32.0);
    }
    true
}

/// Ticks a cloud of `spec` in the ECS and `origin` side by side, and returns the tick both are gone on. The radius is compared each tick.
fn both_end(spec: EffectCloud, mut origin: Origin) -> i32 {
    let mut w = world();
    let e = effect_cloud::spawn(&mut w, [10.0, 64.0, -3.0], PotionId("minecraft:poison"), spec);
    assert!(matches!(w.resource_mut::<Events>().host.drain().next(), Some(HostEvent::Cloud { duration, .. }) if duration == spec.duration));
    for tick in 1..=1000 {
        let origin_alive = origin_tick(&mut origin);
        effect_cloud::tick(&mut w);
        let ours = w.get::<EffectCloud>(e).copied();
        assert_eq!(ours.is_some(), origin_alive, "tick {tick}");
        if let Some(c) = ours {
            assert_eq!(c.radius, origin.radius, "tick {tick}");
        }
        if !origin_alive {
            return tick;
        }
    }
    panic!("the cloud did not end within 1000 ticks");
}

#[test]
fn an_effect_cloud_waits_then_shrinks_and_ends_tick_by_tick_as_the_origin() {
    let spec = EffectCloud { radius: 3.0, radius_per_tick: -3.0 / 600.0, radius_on_use: 0.0, duration_on_use: 0, age: 0, wait: 10, duration: 600, reapplication_delay: 20, duration_scale: 1.0, hidden: &[], ambient: &[], owner: None };
    let end = both_end(spec, Origin { age: 0, radius: 3.0, per_tick: -3.0 / 600.0, wait: 10, duration: 600 });
    assert!(end > 10, "the wait comes first, then the shrink, tick {end}");

    let spec = EffectCloud { radius: 5.0, radius_per_tick: 0.0, wait: 20, duration: 200, ..spec };
    assert_eq!(both_end(spec, Origin { age: 0, radius: 5.0, per_tick: 0.0, wait: 20, duration: 200 }), 220, "the duration counts after the wait: 20 + 200");
}
