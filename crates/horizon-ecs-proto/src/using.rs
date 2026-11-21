//! The item an entity uses (`LivingEntity.useItem`): its hand, and the ticks left as the deadline of the use on the timer wheel, never a
//! counter ticked per tick. Sparse: present only during a use. The totem's pop goes to `UsingOut`; a lethal hit on a holder of a totem is `lethal`. Nothing here names a species.
use crate::absorption::{self, Absorption};
use crate::bridge::host_events_more::resolve_death;
use crate::bridge::ids::id_of;
use crate::cow::{Clock, Dying, Health, Immune, after, now_of, set_on_fire};
use crate::effects::{self, IMMUNE_FIRE, Kind};
use crate::equip_stack;
use crate::host_out_using::{UsingEvent, UsingOut};
use crate::rules::stroll::after_hurt_seed;
use bevy_ecs::prelude::*;
use horizon_core::random::{LegacyRandom, RandomSource};

/// The main hand and the off hand, as the slots of `equip_stack`.
pub const MAIN: usize = 0;
pub const OFF: usize = 1;
/// The item whose `DEATH_PROTECTION` is the totem's (`ItemIds.TOTEM_OF_UNDYING`).
const TOTEM: &str = "minecraft:totem_of_undying";
/// `DeathProtection.TOTEM_OF_UNDYING`: regeneration 900 ticks at amplifier 1, fire resistance 800 ticks at amplifier 0, as (kind, amplifier, ticks).
const TOTEM_EFFECTS: &[(Kind, i32, i32)] = &[(Kind::Regeneration, 1, 900), (Kind::FireResistance, 0, 800)];
/// The totem's absorption: the vanilla absorption effect at amplifier 1 sets `max(current, 4 * (amplifier + 1))`.
const TOTEM_ABSORPTION: f32 = 8.0;

/// The item in use: the hand, the ticks left as the host set them, and the instant the use ends (`until`, on the entity's own clock).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct UsingItem {
    pub off_hand: bool,
    pub remaining: i32,
    pub until: u64,
}

/// The shield's roar, present until its deadline.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Roaring;

/// The use of `e` as (off hand, ticks left); None when it uses nothing.
pub fn state(w: &World, e: Entity) -> Option<(bool, i32)> {
    w.get::<UsingItem>(e).map(|u| (u.off_hand, u.remaining))
}

/// Starts a use with the stack of the chosen hand. False when `e` is gone, already uses an item or the hand is empty. The ticks start at 0:
/// the host sets them with `set_remaining`.
pub fn start(w: &mut World, e: Entity, off_hand: bool) -> bool {
    let slot = if off_hand { OFF } else { MAIN };
    if w.get_entity(e).is_err() || w.get::<UsingItem>(e).is_some() || equip_stack::get(w, e, slot).is_none() {
        return false;
    }
    let until = now_of(w, e);
    w.entity_mut(e).insert(UsingItem { off_hand, remaining: 0, until });
    true
}

/// Sets the ticks left of the use and arms its deadline `counter` ticks on: then `tick_down` ends it. A later call moves the end, so the
/// older deadline finds `until` moved and does nothing. Nothing when `e` uses nothing.
pub fn set_remaining(w: &mut World, e: Entity, counter: i32) {
    let Some(u) = w.get::<UsingItem>(e).copied() else {
        return;
    };
    let until = now_of(w, e) + counter.max(0) as u64;
    w.entity_mut(e).insert(UsingItem { remaining: counter, until, ..u });
    if counter > 0 {
        after(w, e, counter, tick_down);
    }
}

/// The deadline of the use (`Due::Call`): the use ends only when the entity's clock is at its `until`; an older deadline does nothing.
pub fn tick_down(w: &mut World, e: Entity) {
    if w.get::<UsingItem>(e).is_some_and(|u| u.until == now_of(w, e)) {
        stop(w, e);
    }
}

/// Ends the use of `e` and removes its component.
pub fn stop(w: &mut World, e: Entity) {
    if w.get::<UsingItem>(e).is_some() {
        w.entity_mut(e).remove::<UsingItem>();
    }
}

/// `releaseUsingItem`: the use ends (the host's call read the ticks left with `state` before it asked).
pub fn release(w: &mut World, e: Entity) {
    stop(w, e);
}

/// The totem of `e` as (slot, stack): the main hand first, else the off hand.
fn held_totem(w: &World, e: Entity) -> Option<(usize, horizon_client_lib::items::ItemStack)> {
    [MAIN, OFF].into_iter().find_map(|s| equip_stack::get(w, e, s).filter(|st| &*st.item == TOTEM).map(|st| (s, st)))
}

/// `checkTotemDeathProtection`: a totem in the main hand, else in the off hand, is used up. The entity is at 1 health, loses its effects,
/// then gets the totem's. False when no hand holds a totem.
pub fn try_totem(w: &mut World, e: Entity) -> bool {
    if w.get_entity(e).is_err() {
        return false;
    }
    let Some((slot, mut stack)) = held_totem(w, e) else {
        return false;
    };
    stack.count = stack.count.saturating_sub(1);
    equip_stack::set(w, e, slot, Some(stack));
    if w.get::<Dying>(e).is_some() {
        resolve_death(w, e, Some(1.0));
    } else if let Some(mut h) = w.get_mut::<Health>(e) {
        h.0 = 1.0;
    }
    let kinds: Vec<Kind> = w.get::<effects::Effects>(e).map_or_else(Vec::new, |f| f.list.iter().map(|x| x.kind).collect());
    for kind in kinds {
        effects::remove(w, e, kind);
    }
    for &(kind, amp, ticks) in TOTEM_EFFECTS {
        effects::add(w, e, kind, amp, ticks);
    }
    let absorbed = w.get::<Absorption>(e).map_or(0.0, |a| a.0).max(TOTEM_ABSORPTION);
    absorption::set(w, e, absorbed);
    push(w, UsingEvent::TotemPopped { entity: e, off_hand: slot == OFF });
    true
}

/// Set by the host while a mod hears `living_use_totem`: a lethal hit on a holder of a totem then waits for the host (`lethal`).
#[derive(Resource, Default)]
pub struct TotemWatched(pub bool);

/// A lethal hit that waits for the host's answer on the totem (`TotemAsk`): what `die` needs once the answer is no.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct TotemAsked {
    pub damage: &'static str,
    pub by: Option<crate::targeting::Who>,
    pub at: [f64; 3],
    pub health: f32,
}

/// The end of a lethal hit that `apply_damage` queued: a totem in a hand saves the entity (the host is asked first while it listens,
/// `TotemWatched`), except from the void (`#bypasses_invulnerability`); else it dies.
pub(crate) fn lethal(w: &mut World, e: Entity, damage: &'static str, by: Option<crate::targeting::Who>, at: [f64; 3], health: f32) {
    if w.get_entity(e).is_err() || w.get::<TotemAsked>(e).is_some() || crate::tear_down::is_dead(w, e) {
        return;
    }
    if damage != "out_of_world"
        && let Some((slot, _)) = held_totem(w, e)
    {
        if w.get_resource::<TotemWatched>().is_some_and(|t| t.0) {
            w.entity_mut(e).insert(TotemAsked { damage, by, at, health });
            w.resource_mut::<crate::goals::Events>().host.totem_ask(e, slot == OFF, damage, by);
        } else {
            try_totem(w, e);
        }
        return;
    }
    die(w, e, damage, by, at, health);
}

/// The host's answer to a `TotemAsk`: `cancel` leaves the totem unused and the entity dies; else the totem is used up. True when the
/// entity was saved; false too when `e` was not asked.
pub fn resolve_totem(w: &mut World, e: Entity, cancel: bool) -> bool {
    let Some(a) = w.get_entity_mut(e).ok().and_then(|mut m| m.take::<TotemAsked>()) else {
        return false;
    };
    if !cancel && try_totem(w, e) {
        return true;
    }
    die(w, e, a.damage, a.by, a.at, a.health);
    false
}

/// `LivingEntity.die`: the death is said, the entity leaves the index; under `DeathPolicy.vetoable` it stays `Dying` for the host's
/// hook, else its loot and its drops leave and it goes.
fn die(w: &mut World, e: Entity, damage: &'static str, by: Option<crate::targeting::Who>, at: [f64; 3], health: f32) {
    // The death happens once: a body already dying (lingering or held) is not killed again.
    if w.get_entity(e).is_err() || crate::tear_down::is_dead(w, e) {
        return;
    }
    w.resource_mut::<crate::goals::Events>().host.death(e, damage, by, at);
    if let Some(id) = crate::bridge::ids::id_of(w, e) {
        if !crate::remove_killed::silent(w, e) {
            w.resource_mut::<crate::goals::Events>().host.death_animation(id, at);
        }
        if let Some(r) = w.get::<crate::raids::Raider>(e).copied()
            && let Some(raid) = r.raid
        {
            w.resource_mut::<crate::goals::Events>().host.raider_died(raid, id, by, r.wave, r.leader, health);
        }
    }
    if let Some(c) = w.get::<crate::cow::Cell>(e).map(|c| c.0)
        && c != u32::MAX
    {
        w.resource_mut::<crate::cow::Index>().remove(e, c);
    }
    let species = w.get::<crate::cow::SpeciesId>(e).map_or(u8::MAX, |s| s.0);
    w.resource_mut::<crate::goals::Events>().killed.push(crate::behaviours::celebrate::Kill { victim: e, killer: by, species, at });
    if w.get_resource::<crate::cow::DeathPolicy>().is_some_and(|p| p.vetoable) {
        w.entity_mut(e).insert(Dying { health });
        return;
    }
    crate::loot::on_death(w, e);
    crate::item::spill_destroyed(w, e);
    crate::spawn_trial::forget(w, e);
    if crate::remove_killed::silent(w, e) {
        if let Ok(m) = w.get_entity_mut(e) {
            m.despawn();
        }
    } else {
        crate::tear_down::linger(w, e);
    }
}

/// The generator of `e` for this tick: vanilla's `LegacyRandomSource` (the entity's `random`), seeded from its id (or its entity bits, unbound)
/// and the world's tick, since vanilla's seed is unique per entity. Nothing is kept between draws.
fn draw(w: &World, e: Entity) -> Option<LegacyRandom> {
    w.get_entity(e).ok()?;
    let now = w.get_resource::<Clock>().map_or(0, |c| c.0);
    Some(LegacyRandom::new(after_hurt_seed(id_of(w, e).map_or(e.to_bits(), u64::from), now) as i64))
}

/// A float in `[0, 1)` drawn for `e` this tick; None when `e` is gone.
pub fn next_float(w: &World, e: Entity) -> Option<f32> {
    Some(draw(w, e)?.next_float())
}

/// An int in `0..bound` drawn for `e` this tick; None when `e` is gone or `bound` is not positive.
pub fn next_int_bound(w: &World, e: Entity, bound: i32) -> Option<i32> {
    draw(w, e)?.next_int_bound(bound).ok()
}

/// `Entity.igniteForTicks`: `e` burns for `ticks` more.
pub fn ignite(w: &mut World, e: Entity, ticks: i32) {
    set_on_fire(w, e, ticks.max(0) as u64);
}

/// Whether `e` takes no fire: the fire resistance effect sets `IMMUNE_FIRE` on its `Immune` bits.
pub fn fire_immune(w: &World, e: Entity) -> bool {
    w.get::<Immune>(e).is_some_and(|i| i.has(IMMUNE_FIRE))
}

/// The shield's roar: `Roaring` is set now and removed at its deadline `ticks` on.
pub fn roar(w: &mut World, e: Entity, ticks: i32) {
    if w.get_entity(e).is_err() {
        return;
    }
    w.entity_mut(e).insert(Roaring);
    after(w, e, ticks, end_roar);
}

fn end_roar(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Roaring>();
}

pub fn is_roaring(w: &World, e: Entity) -> bool {
    w.get::<Roaring>(e).is_some()
}

/// The events queued since the last call, in order.
pub fn take_events(w: &mut World) -> Vec<UsingEvent> {
    std::mem::take(&mut w.get_resource_or_insert_with(UsingOut::default).0)
}

fn push(w: &mut World, event: UsingEvent) {
    w.get_resource_or_insert_with(UsingOut::default).0.push(event);
}
