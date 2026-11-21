//! The in-game entity tags: add and remove, the refusal past vanilla's 1024 tags per entity, the exact query of a tag, the `Tags` list
//! written and read back, and many distinct names on many entities.
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::{nbt::Saved, nbt_tag as tags};

#[test]
fn add_has_and_remove_one_tag_on_an_entity() {
    let mut w = World::new();
    let e = w.spawn(Saved).id();
    assert!(!tags::has(&w, e, "boss"));
    assert!(tags::add(&mut w, e, "boss"));
    assert!(tags::has(&w, e, "boss"));
    assert!(!tags::add(&mut w, e, "boss"), "a tag the entity has is not added again");
    assert_eq!(tags::names(&w, e), vec!["boss"]);
    assert!(tags::remove(&mut w, e, "boss"));
    assert!(!tags::has(&w, e, "boss"));
    assert!(!tags::remove(&mut w, e, "boss"));
}

#[test]
fn a_query_gives_exactly_the_entities_of_the_tag() {
    let mut w = World::new();
    let a = w.spawn(Saved).id();
    let b = w.spawn(Saved).id();
    let c = w.spawn(Saved).id();
    tags::add(&mut w, a, "raid");
    tags::add(&mut w, c, "raid");
    tags::add(&mut w, b, "other");
    let mut found: Vec<Entity> = tags::query(&mut w, "raid").iter(&w).collect();
    found.sort();
    assert_eq!(found, vec![a, c]);
}

#[test]
fn an_entity_refuses_the_1025th_tag_and_keeps_the_others() {
    let mut w = World::new();
    let e = w.spawn(Saved).id();
    for i in 0..tags::MAX_TAGS {
        assert!(tags::add(&mut w, e, &format!("t{i}")), "tag {i}");
    }
    assert!(!tags::add(&mut w, e, "one_too_many"));
    assert!(!tags::has(&w, e, "one_too_many"));
    assert_eq!(tags::names(&w, e).len(), tags::MAX_TAGS);
    assert!(tags::has(&w, e, "t0"));
}

#[test]
fn the_tags_list_is_written_and_read_back_on_another_entity() {
    let mut w = World::new();
    let a = w.spawn(Saved).id();
    let b = w.spawn(Saved).id();
    tags::add(&mut w, a, "alpha");
    tags::add(&mut w, a, "beta");
    let mut out = Compound::new();
    tags::write(&w, a, &mut out);
    let list = out.get(&JavaString::from("Tags")).expect("a Tags key").clone();
    assert_eq!(list, Tag::List(vec![Tag::String("alpha".into()), Tag::String("beta".into())]));
    assert!(tags::read(&mut w, b, &list));
    assert_eq!(tags::names(&w, b), vec!["alpha", "beta"]);
    assert!(!tags::read(&mut w, b, &Tag::Int(3)), "a tag that is not a list of strings is refused");
    assert_eq!(tags::names(&w, b), vec!["alpha", "beta"], "and changes nothing");
}

#[test]
fn an_entity_without_tags_writes_no_key() {
    let mut w = World::new();
    let e = w.spawn(Saved).id();
    let mut out = Compound::new();
    tags::write(&w, e, &mut out);
    assert!(out.is_empty());
}

#[test]
fn two_thousand_distinct_names_on_two_thousand_entities() {
    let mut w = World::new();
    let made: Vec<Entity> = (0..2000).map(|i| {
        let e = w.spawn(Saved).id();
        assert!(tags::add(&mut w, e, &format!("name_{i}")));
        e
    }).collect();
    for (i, &e) in made.iter().enumerate() {
        assert!(tags::has(&w, e, &format!("name_{i}")));
        assert_eq!(tags::names(&w, e), vec![format!("name_{i}").as_str()]);
    }
    let found: Vec<Entity> = tags::query(&mut w, "name_1999").iter(&w).collect();
    assert_eq!(found, vec![made[1999]]);
}

#[test]
fn a_tag_is_refused_on_an_entity_that_is_not_saved() {
    let mut w = World::new();
    let e = w.spawn_empty().id();
    assert!(!tags::add(&mut w, e, "marked"));
    assert!(!tags::has(&w, e, "marked"));
    w.entity_mut(e).insert(Saved);
    assert!(tags::add(&mut w, e, "marked"));
}
