//! The suspicious stew of a brown mooshroom (`MushroomCow.mobInteract`): an adult eats one flower (`STEW_FLOWER`) and keeps its effect
//! (`Stew`, a sparse state); the next bowl (`STEW_BOWL`, before `held_swap`'s plain stew) comes back as a suspicious stew carrying it.
//! A second flower while one is kept only puffs smoke. The variant is read, no species is named. Saved as the jar's `stew_effects`.
use crate::behaviours::held_swap::swap_in_hand;
use crate::behaviours::shear::adult;
use crate::behaviours::variant::Variant;
use crate::cow::{Body, Pos};
use crate::goals::Events;
use crate::nbt::{owner, Field};
use crate::rules::{no_attrs, Rule};
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::nbt::{Compound, JavaString, Tag};
use std::borrow::Cow;
use std::sync::Mutex;

const BROWN: u8 = 1;

/// The effect the mooshroom keeps from a flower: its id and its duration in ticks (`SuspiciousEffectHolder`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct Stew(pub &'static str, pub i32);

/// `stew_effects` (`SuspiciousStewEffects.CODEC`): a list of `{id, duration}`, here the one entry the creature keeps; its presence poses `Stew`
/// (`nbt::register_creators`).
pub const CREATORS: &[Field] = &[Field { key: Cow::Borrowed("stew_effects"), write: write_stew, read: read_stew, accepts: is_entries, owner: Some(owner::<Stew>) }];

fn is_entries(t: &Tag) -> bool {
    matches!(t, Tag::List(l) if l.iter().all(|c| matches!(c, Tag::Compound(_))))
}

fn write_stew(w: &World, e: Entity, c: &mut Compound) {
    let Some(&Stew(effect, ticks)) = w.get::<Stew>(e) else { return };
    let mut entry = Compound::new();
    entry.insert("id".into(), Tag::String(JavaString::from(effect)));
    entry.insert("duration".into(), Tag::Int(ticks));
    c.insert("stew_effects".into(), Tag::List(vec![Tag::Compound(entry)]));
}

/// The first entry with an id and a duration; an empty list keeps nothing. The id is interned (`Stew` holds a static name).
fn read_stew(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::List(list) = t else { return false };
    for tag in list {
        let Tag::Compound(entry) = tag else { return false };
        let (Some(Tag::String(id)), Some(Tag::Int(ticks))) = (entry.get(&JavaString::from("id")), entry.get(&JavaString::from("duration"))) else { continue };
        w.entity_mut(e).insert(Stew(intern(&id.to_string_lossy()), *ticks));
        break;
    }
    true
}

fn intern(id: &str) -> &'static str {
    static KNOWN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let known = &mut *KNOWN.lock().unwrap_or_else(|p| p.into_inner());
    let found = horizon_items::flowers::FLOWER_STEW_EFFECTS.iter().map(|f| f.1).chain(known.iter().copied()).find(|k| *k == id);
    found.unwrap_or_else(|| {
        let leaked: &'static str = Box::leak(id.to_owned().into_boxed_str());
        known.push(leaked);
        leaked
    })
}

pub static STEW_FLOWER: Rule = Rule { name: "stew_flower", equip: |_, _, _| {}, attrs: no_attrs, interact: flower };
pub static STEW_BOWL: Rule = Rule { name: "stew_bowl", equip: |_, _, _| {}, attrs: no_attrs, interact: bowl };

fn brown_adult(w: &World, e: Entity) -> bool {
    adult(w, e) && w.get::<Variant>(e).is_some_and(|v| v.0 == BROWN)
}

fn middle(w: &World, e: Entity) -> Option<[f64; 3]> {
    let at = w.get::<Pos>(e)?.0;
    let half = w.get::<Body>(e).map_or(0.7, |b| f64::from(b.height) * 0.5);
    Some([at.x, at.y + half, at.z])
}

fn flower(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    if i.item == 0 || !brown_adult(w, e) {
        return false;
    }
    let Some((effect, ticks)) = crate::items::item_name(i.item).and_then(horizon_items::flowers::stew_effect) else { return false };
    let Some(at) = middle(w, e) else { return false };
    if w.get::<Stew>(e).is_some() {
        w.resource_mut::<Events>().host.particles("minecraft:smoke", at, [0.5; 3], 0.2, 2);
        return true;
    }
    crate::give_item::take_from_hand(w, i);
    w.entity_mut(e).insert(Stew(effect, ticks));
    w.resource_mut::<Events>().host.particles("minecraft:effect", at, [0.5; 3], 0.2, 4);
    crate::behaviours::act_phase::sound(w, e, "minecraft:entity.mooshroom.eat", 1.0, 1.0);
    true
}

fn bowl(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    if i.item == 0 || !crate::items::is("minecraft:bowl", i.item) || !adult(w, e) {
        return false;
    }
    let Some(Stew(effect, ticks)) = w.get::<Stew>(e).copied() else { return false };
    let mut stew = ItemStack::new("minecraft:suspicious_stew", 1);
    stew.data_mut().stew_effects = vec![(effect.into(), ticks)];
    if !swap_in_hand(w, i, stew) {
        return false;
    }
    w.entity_mut(e).remove::<Stew>();
    crate::behaviours::act_phase::sound(w, e, "minecraft:entity.mooshroom.milk_suspiciously", 1.0, 1.0);
    true
}
