//! A player's item in hand traded for another on a creature: a bucket for milk on a cow, a bowl for stew on a mooshroom, a water bucket for a bucket of fish
//! on a fish (the creature goes: `takes_creature`). A RULE of the species (`HELD_SWAP`), `Mob.mobInteract` of the adult. What the pair is, is a parameter;
//! the item leaves the hand and the other takes its place (`Online`: the bench's player has the one hand); in creative the item stays. The host loses the item and gets the other by `HostEvent::HeldSwap`.
//! A creature taken into its bucket carries `Bucketable.saveDefaultDataToBucketTag` (`bucket_entity_data`) and its custom name, then the
//! species' own data (`SwapParams::extra`: the fish's variant, the baby's age). Not ported: the baby's refusal
//! beyond `adult_only`, the sheep's dye (`behaviours::shear` does the shears).
use crate::age_lock::AgeLocked;
use crate::behaviours::spawn_draw::Wide;
use crate::behaviours::variant::Variant;
use crate::brain::sensors::attackables::HAS_HUNTING_COOLDOWN;
use crate::cow::*;
use crate::goals::*;
use crate::rules::variant::Table;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use myriade_engine_lib::items::ItemStack;

#[derive(Clone, Copy, Debug)]
pub struct SwapParams {
    pub from: &'static str,
    pub to: &'static str,
    pub adult_only: bool,
    /// The creature is taken into the item (a fish in its bucket).
    pub takes_creature: bool,
    /// The sound of the swap, played where the creature is.
    pub sound: &'static str,
    /// Another sound when the test holds on the creature (the screaming goat's milk).
    pub alt: Option<(fn(&World, Entity) -> bool, &'static str)>,
    /// The species' own data of the item, written after `bucket_entity_data` (`no_data` when there is none).
    pub extra: fn(&World, Entity, &mut ItemStack),
}
/// The item of `i` leaves the player's hand and `to` takes its place: in the mirror (`Online`, by id) and on the host (`HostEvent::HeldSwap`,
/// which carries the stack's data). False when the player is not online or `to` is no item.
pub fn swap_in_hand(w: &mut World, i: &crate::behaviours::Interaction, to: myriade_engine_lib::items::ItemStack) -> bool {
    let id = crate::items::item_id(&to.item);
    let Some(pl) = w.resource::<Online>().get(i.player) else { return false };
    if id == 0 {
        return false;
    }
    // Creative (`createFilledResult`): the item stays in the hand, the host adds the result when the inventory lacks it.
    if !w.get_resource::<crate::used_stack::UsedStack>().is_some_and(|u| u.creative) {
        let hand = if pl.main == i.item { Player { main: id, ..pl } } else { Player { off: id, ..pl } };
        w.resource_mut::<Online>().set(hand);
    }
    w.resource_mut::<Events>().host.held_swap(i.player, i.item, to);
    true
}
/// `Bucketable.saveDefaultDataToBucketTag`: `Health` always, each flag only when it is set, read from its component.
fn bucket_data(w: &World, e: Entity) -> Compound {
    let mut c = Compound::new();
    if let Some(h) = w.get::<Health>(e) {
        c.insert("Health".into(), Tag::Float(h.0.to_bits()));
    }
    let flags = [
        ("NoAI", w.get::<crate::dragon::flight::NoAi>(e).is_some()),
        ("Silent", w.get::<crate::behaviours::elder_curse::Silent>(e).is_some()),
        ("NoGravity", w.get::<crate::no_gravity::NoGravity>(e).is_some()),
        ("Glowing", w.get::<crate::glowing::Glowing>(e).is_some()),
        ("Invulnerable", w.get::<crate::invulnerable::Invulnerable>(e).is_some()),
        ("PersistenceRequired", w.get::<crate::despawn::Persistent>(e).is_some()),
    ];
    for (k, on) in flags {
        if on {
            c.insert(k.into(), Tag::Byte(1));
        }
    }
    c
}
/// The data of a bucket that has none of its own.
pub fn no_data(_: &World, _: Entity, _: &mut ItemStack) {}
/// The packed `TropicalFish` patterns (`Pattern.getPackedId`: the size's id, `Base` small 0 and large 1, in the index << 8), by ordinal.
const PATTERNS: [(&str, u32); 12] = [
    ("kob", 0x000), ("sunstreak", 0x100), ("snooper", 0x200), ("dasher", 0x300), ("brinely", 0x400), ("spotty", 0x500),
    ("flopper", 0x001), ("stripey", 0x101), ("glitter", 0x201), ("blockfish", 0x301), ("betty", 0x401), ("clayfish", 0x501),
];
/// The name of the variant `id` of `t` as an item's component: `component_names`, else the saved `names`.
fn variant_name(t: &Table, id: u8) -> Option<&'static str> {
    let i = t.ids.iter().position(|x| *x == id)?;
    t.component_names.get(i).or_else(|| t.names.get(i)).copied()
}
/// Writes the variant of `e` as the item's component `key` (`copyFrom` of `SALMON_SIZE` or `AXOLOTL_VARIANT`).
fn variant_component(w: &World, e: Entity, to: &mut ItemStack, key: &str) {
    let Some(t) = crate::behaviours::variant::table_of(w, e) else { return };
    let v = w.get::<Variant>(e).map_or_else(|| crate::behaviours::variant::default_id(t), |v| v.0);
    if let Some(name) = variant_name(t, v) {
        to.data_mut().set_other(key, Some(Tag::String(name.into())));
    }
}
/// Adds `k` to the `bucket_entity_data` of `to` (`CustomData.update`): the saved data of the creature in its bucket.
fn put_entity_data(to: &mut ItemStack, k: &str, v: Tag) {
    let mut c = match to.data().and_then(|d| d.other("minecraft:bucket_entity_data")) {
        Some(Tag::Compound(c)) => c.clone(),
        _ => Compound::new(),
    };
    c.insert(k.into(), v);
    to.data_mut().set_other("minecraft:bucket_entity_data", Some(Tag::Compound(c)));
}
/// `Age` (negative: the ticks left as a baby, zero for a grown-up) and `AgeLocked`, as `AgeableMob.addAdditionalSaveData` writes them.
fn age_data(w: &World, e: Entity, to: &mut ItemStack) {
    let age = match w.get::<GrowAt>(e) {
        Some(g) => -i32::try_from(w.get::<AgeLocked>(e).map_or_else(|| g.0.saturating_sub(now_of(w, e)), |l| l.0)).unwrap_or(i32::MAX),
        None => 0,
    };
    put_entity_data(to, "Age", Tag::Int(age));
    put_entity_data(to, "AgeLocked", Tag::Byte(i8::from(w.get::<AgeLocked>(e).is_some())));
}
/// `Salmon.saveToBucketTag`: the size component.
pub fn salmon_data(w: &World, e: Entity, to: &mut ItemStack) {
    variant_component(w, e, to, "minecraft:salmon/size");
}
/// `TropicalFish.saveToBucketTag`: the pattern and the two colours, read from the packed id that `Wide` holds whole (bits 0-15 the pattern,
/// 16-23 the base colour, 24-31 the pattern's colour; the colours are `DyeColor`'s order).
pub fn tropical_data(w: &World, e: Entity, to: &mut ItemStack) {
    // `TropicalFish.DEFAULT_VARIANT` (kob, white, white: packed 0) for a fish never drawn.
    let p = w.get::<Wide>(e).map_or(0, |p| p.0);
    let Some(&(pattern, _)) = PATTERNS.iter().find(|(_, id)| *id == p & 0xFFFF) else { return };
    let (Some(base), Some(dye)) = (crate::species::cushion::DYES.get(((p >> 16) & 0xFF) as usize), crate::species::cushion::DYES.get((p >> 24) as usize)) else { return };
    to.data_mut().set_other("minecraft:tropical_fish/pattern", Some(Tag::String(pattern.into())));
    to.data_mut().set_other("minecraft:tropical_fish/base_color", Some(Tag::String((*base).into())));
    to.data_mut().set_other("minecraft:tropical_fish/pattern_color", Some(Tag::String((*dye).into())));
}
/// `Axolotl.saveToBucketTag`: the variant component, then `Age`, `AgeLocked` and `HuntingCooldown` (only while the hunting memory is set).
pub fn axolotl_data(w: &World, e: Entity, to: &mut ItemStack) {
    variant_component(w, e, to, "minecraft:axolotl/variant");
    age_data(w, e, to);
    if crate::brain::present(w, e, &HAS_HUNTING_COOLDOWN) {
        let left = crate::brain::ttl_left(w, e, &HAS_HUNTING_COOLDOWN).unwrap_or(0);
        put_entity_data(to, "HuntingCooldown", Tag::Long(left as i64));
    }
}
/// `Tadpole.saveToBucketTag`: `Age` and `AgeLocked`.
pub fn tadpole_data(w: &World, e: Entity, to: &mut ItemStack) {
    age_data(w, e, to);
}
pub static HELD_SWAP: Rule = Rule { name: "held_swap", equip: |_, _, _| {}, attrs: no_attrs, interact };
fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<SwapParams>(p);
    if i.item == 0 || !crate::items::is(p.from, i.item) || (p.adult_only && w.get::<GrowAt>(e).is_some()) {
        return false;
    }
    let mut to = myriade_engine_lib::items::ItemStack::new(p.to, 1);
    if p.takes_creature {
        to.data_mut().set_other("minecraft:bucket_entity_data", Some(Tag::Compound(bucket_data(w, e))));
        if let Some(name) = crate::custom_name::tag(w, e) {
            to.data_mut().set_other("minecraft:custom_name", Some(name));
        }
        (p.extra)(w, e, &mut to);
    }
    if !swap_in_hand(w, i, to) {
        return false;
    }
    let sound = p.alt.filter(|(test, _)| test(w, e)).map_or(p.sound, |(_, s)| s);
    crate::behaviours::act_phase::sound(w, e, sound, 1.0, 1.0);
    if p.takes_creature {
        w.entity_mut(e).despawn();
    }
    true
}
