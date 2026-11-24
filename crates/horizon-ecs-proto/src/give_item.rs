//! The rule `GIVE`: a player gives the entity an item it accepts; it heals, cures, or both. One rule for "an item that heals" (iron golem
//! ingot) and "an item that cures under an effect" (golden apple on a zombie villager under Weakness). Nothing here is about one species.
use crate::cow::*;
use crate::effects::{self, Kind};
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;

/// A deferred conversion started by the item: the entity becomes `into` after `base + nextInt(spread)` ticks.
#[derive(Clone, Copy, Debug)]
pub struct Cure {
    pub into: &'static str,
    pub base: i32,
    pub spread: i32,
    /// The effect granted for the duration of the cure (level 0), and the entity event sent at the start.
    pub grants: Option<Kind>,
    pub event: u8,
}
#[derive(Clone, Copy, Debug)]
pub struct GiveParams {
    /// Accepted items: names or `#tags` (`items::is`).
    pub items: &'static [&'static str],
    /// The entity must carry this effect; it is removed by the gift.
    pub needs: Option<Kind>,
    /// When the item matches but `needs` is not met: the click is handled without consuming (vanilla `CONSUME`) instead of ignored.
    pub swallow: bool,
    /// Health given, and whether the gift is refused at full health.
    pub heal: f32,
    pub only_hurt: bool,
    pub cure: Option<Cure>,
    pub sound: Option<&'static str>,
    /// The sound's pitch is `1 + (a - b) * spread` with a, b in [0, 1).
    pub pitch_spread: f32,
}
impl GiveParams {
    pub const NONE: Self = Self { items: &[], needs: None, swallow: false, heal: 0.0, only_hurt: false, cure: None, sound: None, pitch_spread: 0.0 };
    /// `IronGolem.mobInteract`: an iron ingot heals 25 of an injured golem.
    pub const IRON_REPAIR: Self = Self { items: &["minecraft:iron_ingot"], heal: 25.0, only_hurt: true, sound: Some("minecraft:entity.iron_golem.repair"), pitch_spread: 0.2, ..Self::NONE };
    /// `ZombieVillager.mobInteract`: a golden apple under Weakness starts the cure (3600 + nextInt(2401) ticks, Strength meanwhile).
    pub const VILLAGER_CURE: Self = Self {
        items: &["minecraft:golden_apple"],
        needs: Some(Kind::Weakness),
        swallow: true,
        cure: Some(Cure { into: crate::species::lists::VILLAGER, base: 3600, spread: 2401, grants: Some(Kind::Strength), event: 16 }),
        ..Self::NONE
    };
}
/// The cure under way (rare: only while it lasts).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Curing {
    pub into: &'static str,
}
pub static GIVE: Rule = Rule { name: "give_item", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<GiveParams>(p);
    if i.item == 0 || !p.items.iter().any(|n| crate::items::is(n, i.item)) || w.get::<Curing>(e).is_some() {
        return false;
    }
    if p.needs.is_some_and(|k| !effects::has(w, e, k)) {
        return p.swallow;
    }
    if p.only_hurt {
        let (Some(h), m) = (w.get::<Health>(e).map(|h| h.0), max_health(w, e)) else { return false };
        if h >= m {
            return false;
        }
    }
    if p.heal > 0.0 {
        let max = max_health(w, e);
        if let Some(mut h) = w.get_mut::<Health>(e) {
            h.0 = crate::rules::regen::healed(h.0, p.heal, max);
        }
        crate::bridge::view::mark(w, e);
    }
    if let Some(k) = p.needs {
        effects::remove(w, e, k);
    }
    if let Some(c) = p.cure {
        let ticks = c.base + with_rng(w, e, |r| r.next_int(c.spread)) as i32;
        if let Some(k) = c.grants {
            effects::add(w, e, k, 0, ticks);
        }
        w.entity_mut(e).insert(Curing { into: c.into });
        after(w, e, ticks, finish_cure);
        w.resource_mut::<Events>().host.entity_event(e, c.event);
    }
    if let Some(s) = p.sound {
        let (a, b) = with_rng(w, e, |r| (r.next_int(1000), r.next_int(1000)));
        let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
        w.resource_mut::<Events>().host.sound(s, at, 1.0, 1.0 + (a - b) as f32 / 1000.0 * p.pitch_spread);
    }
    take_from_hand(w, i);
    true
}
/// The deadline of the cure: the entity becomes what the gift made it, keeping what it wears.
fn finish_cure(w: &mut World, e: Entity) {
    let Some(c) = w.get::<Curing>(e).copied() else { return };
    crate::conversion::convert(w, e, c.into, crate::conversion::ConvertKeep::ALL);
}
/// One item leaves the player's hand: in the mirror (`Online`), and on the host by `HostEvent::HeldUsed`.
pub fn take_from_hand(w: &mut World, i: &crate::behaviours::Interaction) {
    if let Some(pl) = w.resource::<Online>().get(i.player) {
        let main = pl.main == i.item;
        let hand = Player { main: if main { 0 } else { pl.main }, off: if !main && pl.off == i.item { 0 } else { pl.off }, ..pl };
        w.resource_mut::<Online>().set(hand);
    }
    w.resource_mut::<Events>().host.held_used(i.player, i.item);
}
