//! The NBT socle: the view composed from the components and the free keys kept as loaded, the declarations of the registry (several
//! fields per key, one per kind of entity, in declaration order), the refusal of a wrong type, the atomic merge, the change event of the
//! free keys only, the presence of `NbtData` (the free keys, present only while there is one) and `Kind`.
use bevy_ecs::prelude::*;
use horizon_core::{
    geometry::Vec3,
    nbt::{Compound, Tag},
};
use horizon_ecs_proto::{
    cow::{spawn_species, species_table, Herd, Pos},
    passive,
    nbt::{self, accept, NbtData, Field, Kind, NbtChanged, Saved},
    nbt_tag,
    passive::{DisplayBox, Interact},
};
use std::borrow::Cow;

#[derive(Component)]
struct Score(i32);

#[derive(Component)]
struct ModMeter(i32);

#[derive(Resource, Default)]
struct Seen(Vec<String>);

#[derive(Resource, Default)]
struct Order(Vec<&'static str>);

const SCORE: &[Field] = &[Field {
    key: Cow::Borrowed("Score"),
    write: write_score,
    read: read_score,
    accepts: accept::int,
    owner: Some(nbt::owner::<Score>),
}];

fn write_score(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<Score>(e) {
        c.insert("Score".into(), Tag::Int(s.0));
    }
}

fn read_score(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    w.entity_mut(e).insert(Score(*v));
    true
}

fn comp(entries: impl IntoIterator<Item = (&'static str, Tag)>) -> Compound {
    match Tag::compound(entries) {
        Tag::Compound(c) => c,
        _ => unreachable!("Tag::compound makes a compound"),
    }
}

fn has_custom(w: &World, e: Entity) -> bool {
    w.get::<NbtData>(e).is_some()
}

#[test]
fn an_entity_without_nbt_has_none_and_a_load_keeps_every_unknown_key() {
    let mut w = World::new();
    let bare = w.spawn_empty().id();
    assert!(nbt::save(&w, bare).is_none());
    assert!(nbt::load(&mut w, bare, &comp([("x", Tag::Int(1))])).is_none());
    assert!(!nbt::merge(&mut w, bare, &comp([("x", Tag::Int(1))])));
    assert!(!has_custom(&w, bare), "no NBT, no free key");

    let src = comp([
        ("data", Tag::compound([("deep", Tag::compound([("k", Tag::List(vec![Tag::Int(1)]))]))])),
        ("horizon:attachments", Tag::compound([("a", Tag::String("b".into()))])),
        ("mod_key", Tag::Long(9)),
    ]);
    let e = w.spawn((Kind("test".into()), Saved)).id();
    assert_eq!(nbt::load(&mut w, e, &src), Some(0));
    assert_eq!(nbt::save(&w, e), Some(src));
}

#[test]
fn a_declared_key_goes_to_its_component_and_never_to_the_free_keys() {
    let mut w = World::new();
    nbt::register(&mut w, SCORE);
    let e = w.spawn((Kind("test".into()), Saved)).id();
    assert_eq!(nbt::load(&mut w, e, &comp([("Score", Tag::Int(7)), ("data", Tag::Int(1))])), Some(0));
    assert_eq!(w.get::<Score>(e).map(|s| s.0), Some(7));
    let free = w.get::<NbtData>(e).expect("the free key");
    assert_eq!(free.0.len(), 1, "only `data` is free");
    assert_eq!(nbt::save(&w, e), Some(comp([("Score", Tag::Int(7)), ("data", Tag::Int(1))])));
}

#[test]
fn a_refused_declared_key_is_ignored_counted_and_never_kept_free() {
    let mut w = World::new();
    nbt::register(&mut w, SCORE);
    let e = w.spawn((Kind("test".into()), Saved)).id();
    assert_eq!(nbt::load(&mut w, e, &comp([("Score", Tag::String("x".into()))])), Some(1));
    assert!(w.get::<Score>(e).is_none());
    assert!(!has_custom(&w, e), "a refused declared key is not free");
}

#[test]
fn a_merge_with_one_refused_tag_changes_nothing() {
    let mut w = World::new();
    nbt::register(&mut w, SCORE);
    let e = w.spawn((Kind("test".into()), Saved, Score(1))).id();
    nbt::load(&mut w, e, &comp([("data", Tag::Int(1))]));
    let before = nbt::save(&w, e);
    assert!(!nbt::merge(&mut w, e, &comp([("Score", Tag::String("x".into())), ("other", Tag::Int(1))])));
    assert_eq!(nbt::save(&w, e), before);
    assert_eq!(w.get::<Score>(e).map(|s| s.0), Some(1));
}

#[test]
fn a_merge_is_recursive_and_a_remove_takes_one_free_key() {
    let mut w = World::new();
    let e = w.spawn((Kind("test".into()), Saved)).id();
    nbt::load(&mut w, e, &comp([("data", Tag::compound([("a", Tag::Int(1))]))]));
    assert!(nbt::merge(&mut w, e, &comp([("data", Tag::compound([("b", Tag::Int(3)), ("c", Tag::Int(4))]))])));
    assert_eq!(nbt::get(&w, e, "data"), Some(Tag::compound([("a", Tag::Int(1)), ("b", Tag::Int(3)), ("c", Tag::Int(4))])));
    assert!(nbt::remove(&mut w, e, "data"));
    assert!(!nbt::remove(&mut w, e, "data"));
    assert!(!has_custom(&w, e), "no free key, no NbtData");
}

#[test]
fn a_remove_of_a_declared_key_is_refused() {
    let mut w = World::new();
    nbt::register(&mut w, SCORE);
    let e = w.spawn((Kind("test".into()), Saved, Score(3))).id();
    assert!(!nbt::remove(&mut w, e, "Score"));
    assert_eq!(w.get::<Score>(e).map(|s| s.0), Some(3));
}

#[test]
fn declared_keys_are_distributed_in_declaration_order() {
    fn read_zeta(w: &mut World, _: Entity, _: &Tag) -> bool {
        w.resource_mut::<Order>().0.push("zeta");
        true
    }
    fn read_alpha(w: &mut World, _: Entity, _: &Tag) -> bool {
        w.resource_mut::<Order>().0.push("alpha");
        true
    }
    fn none(_: &World, _: Entity, _: &mut Compound) {}
    let mut w = World::new();
    w.init_resource::<Order>();
    let zeta = Field { key: Cow::Borrowed("zeta"), write: none, read: read_zeta, accepts: accept::any, owner: None };
    let alpha = Field { key: Cow::Borrowed("alpha"), write: none, read: read_alpha, accepts: accept::any, owner: None };
    nbt::register(&mut w, &[zeta, alpha]);
    let e = w.spawn((Kind("test".into()), Saved)).id();
    nbt::load(&mut w, e, &comp([("alpha", Tag::Int(1)), ("zeta", Tag::Int(2))]));
    assert_eq!(w.resource::<Order>().0, ["zeta", "alpha"], "the declaration order, not the key order");
}

#[test]
fn one_key_has_a_field_per_component_and_the_applicable_one_is_used() {
    let mut w = World::new();
    let meter = Field {
        key: Cow::Borrowed("value"),
        write: |w, e, c| {
            if let Some(m) = w.get::<ModMeter>(e) {
                c.insert("value".into(), Tag::Int(m.0));
            }
        },
        read: |w, e, t| match t {
            Tag::Int(v) => {
                w.entity_mut(e).insert(ModMeter(*v));
                true
            }
            _ => false,
        },
        accepts: accept::int,
        owner: Some(nbt::owner::<ModMeter>),
    };
    nbt::register(&mut w, SCORE);
    nbt::register_owned(&mut w, meter);
    let e = w.spawn((Kind("test".into()), Saved)).id();
    let scored = w.spawn((Kind("test".into()), Saved, Score(0))).id();
    nbt::load(&mut w, e, &comp([("value", Tag::Int(4))]));
    assert_eq!(w.get::<ModMeter>(e).map(|m| m.0), Some(4));
    assert_eq!(nbt::get(&w, e, "value"), Some(Tag::Int(4)));
    assert!(!has_custom(&w, e), "a declared key is never free");
    assert_eq!(nbt::load(&mut w, scored, &comp([("value", Tag::Int(4))])), Some(0));
    assert!(w.get::<Score>(scored).is_some() && w.get::<ModMeter>(scored).is_none(), "no field applies: `value` stays free");
}

#[test]
#[should_panic(expected = "two fields")]
fn two_applicable_fields_of_one_key_on_one_entity_are_an_error() {
    let mut w = World::new();
    nbt::register(&mut w, SCORE);
    nbt::register(&mut w, SCORE);
    let e = w.spawn((Kind("test".into()), Saved, Score(1))).id();
    nbt::save(&w, e);
}

#[test]
fn a_width_belongs_to_the_interaction_or_the_display_that_has_it() {
    let mut w = World::new();
    nbt::register(&mut w, horizon_ecs_proto::passive::FIELDS);
    let display = w.spawn((Kind("test".into()), Saved, DisplayBox::default())).id();
    let interaction = w.spawn((Kind("test".into()), Saved, Interact::default())).id();
    nbt::load(&mut w, display, &comp([("width", Tag::Float(2.5f32.to_bits()))]));
    nbt::load(&mut w, interaction, &comp([("width", Tag::Float(0.5f32.to_bits()))]));
    assert_eq!(w.get::<DisplayBox>(display).map(|b| b.width), Some(2.5));
    assert_eq!(w.get::<Interact>(interaction).map(|i| i.width), Some(0.5));
    assert!(!has_custom(&w, display) && !has_custom(&w, interaction));
}

#[test]
fn removing_saved_takes_the_free_keys_and_the_tags_away() {
    let mut w = World::new();
    let e = w.spawn((Kind("test".into()), Saved)).id();
    nbt::load(&mut w, e, &comp([("data", Tag::Int(1))]));
    assert!(nbt_tag::add(&mut w, e, "marked"));
    assert!(has_custom(&w, e));
    w.entity_mut(e).remove::<Saved>();
    w.flush();
    assert!(!has_custom(&w, e), "the free keys go with Saved");
    assert!(!nbt_tag::has(&w, e, "marked"), "the tags go with Saved");
    assert!(nbt::save(&w, e).is_none());
    assert_eq!(nbt::get(&w, e, "data"), None);
    assert!(!nbt::merge(&mut w, e, &comp([("data", Tag::Int(2))])));
}

#[test]
fn kind_is_kept_only_by_an_entity_without_a_species() {
    let mut w = World::new();
    nbt::register(&mut w, nbt::COMMON);
    let passive = w.spawn((Kind("test".into()), Saved)).id();
    nbt::load(&mut w, passive, &comp([("id", Tag::String("minecraft:item".into()))]));
    assert_eq!(w.get::<Kind>(passive).map(|k| k.0.as_str()), Some("minecraft:item"));
    assert_eq!(nbt::get(&w, passive, "id"), Some(Tag::String("minecraft:item".into())));
    assert!(!has_custom(&w, passive), "`id` is declared, so not free");
}

#[test]
fn a_changed_free_key_sends_one_event_and_a_declared_key_none() {
    let mut w = World::new();
    w.init_resource::<Seen>();
    w.add_observer(|on: On<NbtChanged>, mut seen: ResMut<Seen>| seen.0.push(on.key.clone()));
    nbt::register(&mut w, SCORE);
    let e = w.spawn((Kind("test".into()), Saved)).id();
    nbt::load(&mut w, e, &comp([("data", Tag::Int(1)), ("Score", Tag::Int(2))]));
    assert_eq!(w.resource::<Seen>().0, ["data"]);
    nbt::load(&mut w, e, &comp([("data", Tag::Int(1))]));
    assert_eq!(w.resource::<Seen>().0, ["data"], "an unchanged free key sends nothing");
}

#[test]
fn a_field_declared_at_run_time_is_read_and_written() {
    let mut w = World::new();
    let meter = Field {
        key: Cow::Owned("mod:meter".to_string()),
        write: |w, e, c| {
            if let Some(m) = w.get::<ModMeter>(e) {
                c.insert("mod:meter".into(), Tag::Int(m.0));
            }
        },
        read: |w, e, t| match t {
            Tag::Int(v) => {
                w.entity_mut(e).insert(ModMeter(*v));
                true
            }
            _ => false,
        },
        accepts: accept::int,
        owner: Some(nbt::owner::<ModMeter>),
    };
    nbt::register_owned(&mut w, meter);
    let e = w.spawn((Kind("test".into()), Saved, ModMeter(0))).id();
    nbt::load(&mut w, e, &comp([("mod:meter", Tag::Int(4))]));
    assert_eq!(w.get::<ModMeter>(e).map(|m| m.0), Some(4));
    assert_eq!(nbt::get(&w, e, "mod:meter"), Some(Tag::Int(4)));
}

#[test]
fn the_common_position_round_trips() {
    let mut w = World::new();
    nbt::register(&mut w, nbt::COMMON);
    let e = w.spawn((Pos(Vec3::new(1.0, 2.0, 3.0)), Kind("test".into()), Saved)).id();
    let saved = nbt::save(&w, e).expect("an NBT");
    let f = w.spawn((Kind("test".into()), Saved)).id();
    nbt::load(&mut w, f, &saved);
    assert_eq!((w.get::<Pos>(f).unwrap().0.x, w.get::<Pos>(f).unwrap().0.y, w.get::<Pos>(f).unwrap().0.z), (1.0, 2.0, 3.0));
}

#[test]
fn the_unsaved_species_are_declared_so_and_the_others_saved() {
    let registry = horizon_ecs_proto::species::registry();
    assert!(!registry.by_name("minecraft:leash_knot").unwrap().saved);
    assert!(!registry.by_name("minecraft:fishing_bobber").unwrap().saved);
    assert!(registry.by_name("minecraft:cow").unwrap().saved);
}

#[test]
fn save_all_then_restore_in_a_new_world_gives_the_same_nbt() {
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow_species = species_table().0.into_iter().find(|s| s.name == "minecraft:cow").expect("the cow");
    let cow = spawn_species(&mut a.world, &cow_species, 3.0, 4.0, 1);
    nbt_tag::add(&mut a.world, cow, "marked");
    nbt::load(&mut a.world, cow, &comp([("data", Tag::Int(4))]));
    let display = passive::spawn(&mut a.world, "minecraft:block_display", Vec3::new(1.0, 2.0, 3.0), Tag::Compound(Compound::new()));
    nbt::load(&mut a.world, display, &comp([("data", Tag::Int(5))]));
    let mut saved = Vec::new();
    nbt::save_all(&mut a.world, |e, c| saved.push((e, c)));
    assert_eq!(saved.len(), 2, "the cow and the display, both saved");

    let mut b = Herd::new(0, 1.0, true, false, 1);
    let mut restored = Vec::new();
    for (e, c) in &saved {
        let r = nbt::restore(&mut b.world, c).expect("a known, saved type");
        assert_eq!(nbt::save(&b.world, r).as_ref(), Some(c), "the same NBT as {e:?}");
        restored.push(r);
    }
    let restored_cow = restored.into_iter().find(|&r| nbt_tag::has(&b.world, r, "marked")).expect("the cow back, with its tag");
    assert_eq!(nbt::get(&b.world, restored_cow, "data"), Some(Tag::Int(4)));
}

#[test]
fn restore_of_an_unknown_type_is_none_and_spawns_nothing() {
    let mut b = Herd::new(0, 1.0, true, false, 1);
    let before = b.world.query::<Entity>().iter(&b.world).count();
    let c = comp([("id", Tag::String("minecraft:not_a_mob".into())), ("Pos", Tag::List(vec![Tag::Double(0.0f64.to_bits()), Tag::Double(0.0f64.to_bits()), Tag::Double(0.0f64.to_bits())]))]);
    assert!(nbt::restore(&mut b.world, &c).is_none());
    assert_eq!(b.world.query::<Entity>().iter(&b.world).count(), before);
}

static WRITES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn count_write(_: &World, _: Entity, _: &mut Compound) {
    WRITES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

fn refuse(_: &mut World, _: Entity, _: &Tag) -> bool {
    false
}

#[test]
fn a_field_whose_component_is_absent_is_never_called() {
    let mut w = World::new();
    let field = Field { key: Cow::Borrowed("counted"), owner: Some(nbt::owner::<Score>), accepts: accept::any, write: count_write, read: refuse };
    nbt::register(&mut w, &[field]);
    let bare = w.spawn(Saved).id();
    assert!(nbt::save(&w, bare).is_some());
    assert_eq!(WRITES.load(std::sync::atomic::Ordering::SeqCst), 0, "no Score, no call");
    let scored = w.spawn((Saved, Score(1))).id();
    assert!(nbt::save(&w, scored).is_some());
    assert_eq!(WRITES.load(std::sync::atomic::Ordering::SeqCst), 1, "the one entity with a Score");
}

#[derive(Resource, Default)]
struct Sent(Vec<Entity>);

#[test]
fn save_all_sends_saving_on_each_saved_entity_before_composing() {
    let mut w = World::new();
    w.init_resource::<Sent>();
    w.add_observer(|on: On<nbt::Saving>, mut sent: ResMut<Sent>| sent.0.push(on.entity));
    let a = w.spawn(Saved).id();
    let b = w.spawn((Saved, Score(2))).id();
    w.spawn(Score(3));
    nbt::save_all(&mut w, |_, _| {});
    let mut sent = w.resource::<Sent>().0.clone();
    sent.sort();
    assert_eq!(sent, [a, b], "only the Saved entities");
}
