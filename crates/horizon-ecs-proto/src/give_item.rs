//! The rule `GIVE`: a player gives the entity an item it accepts; it heals, cures, or both. One rule for "an item that heals" (iron golem
//! ingot) and "an item that cures under an effect" (golden apple on a zombie villager under Weakness). Nothing here is about one species.
use crate::behaviours::convert_timer::{self, Converting};
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
/// The cure under way (rare: only while it lasts), with its count in `Converting`; `starter` is the player's uuid (`ConversionPlayer`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Curing {
    pub into: &'static str,
    pub starter: Option<u128>,
}
pub static GIVE: Rule = Rule { name: "give_item", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<GiveParams>(p);
    if i.item == 0 || !p.items.iter().any(|n| crate::items::is(n, i.item)) {
        return false;
    }
    if w.get::<Curing>(e).is_some() {
        take_from_hand(w, i);
        return true;
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
        let starter = crate::player_uuids::uuid_of(w, i.player);
        start_cure(w, e, c, ticks, starter);
    }
    if let Some(s) = p.sound {
        let (a, b) = with_rng(w, e, |r| (r.next_int(1000), r.next_int(1000)));
        let at = w.get::<Pos>(e).map_or([0.0; 3], |q| [q.0.x, q.0.y, q.0.z]);
        w.resource_mut::<Events>().host.sound(s, at, 1.0, 1.0 + (a - b) as f32 / 1000.0 * p.pitch_spread);
    }
    take_from_hand(w, i);
    true
}
/// `ZombieVillager.startConverting`: the count runs from `ticks`, Weakness goes, Strength lasts as long, the client shakes (event 16).
pub fn start_cure(w: &mut World, e: Entity, c: Cure, ticks: i32, starter: Option<u128>) {
    effects::remove(w, e, Kind::Weakness);
    if let Some(k) = c.grants {
        effects::add(w, e, k, 0, ticks);
    }
    let until = crate::cow::now_of(w, e) + ticks as u64;
    w.entity_mut(e).insert((Converting { until }, Curing { into: c.into, starter }));
    after(w, e, 1, cure_tick);
    w.resource_mut::<Events>().host.entity_event(e, c.event);
}
/// `ZombieVillager.tick`: the count drops by `getConversionProgress` each tick. `until` moves by `1 - progress`, so that `until - now` is
/// the count left, as in vanilla.
fn cure_tick(w: &mut World, e: Entity) {
    let Some(c) = w.get::<Converting>(e).copied() else { return };
    if w.get::<Curing>(e).is_none() {
        return;
    }
    let progress = convert_timer::cure_progress(w, e);
    let until = (c.until + 1).saturating_sub(progress as u64);
    if crate::rules::convert_timer::count_over(crate::cow::now_of(w, e), until) {
        finish_cure(w, e);
    } else {
        if let Some(mut t) = w.get_mut::<Converting>(e) {
            t.until = until;
        }
        after(w, e, 1, cure_tick);
    }
}
/// `ZombieVillager.finishConversion`: the villager keeps the type, profession, level and finalized flag of the zombie, and the equipment;
/// it is nauseous for 200 ticks, the level event 1027 sounds unless the zombie is silent, and the starter gets the cure criterion
/// (host event) and the reputation (`ZOMBIE_VILLAGER_CURED`: major +20, minor +25).
fn finish_cure(w: &mut World, e: Entity) {
    let Some(c) = w.get::<Curing>(e).copied() else { return };
    w.entity_mut(e).remove::<(Converting, Curing)>();
    let carried = crate::nbt_fields::creatures::carried_villager(w, e);
    let finalized = w.get::<crate::bridge::sync_a::VillagerFinalized>(e).is_some();
    let silent = w.get::<crate::behaviours::elder_curse::Silent>(e).is_some();
    let at = w.get::<Pos>(e).map(|p| [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32]);
    let Some(n) = crate::conversion::convert(w, e, c.into, crate::conversion::ConvertKeep::ALL) else { return };
    if let Some((variant, profession, level)) = carried
        && let Some(mut m) = w.get_mut::<crate::behaviours::trade_offers::Merchant>(n)
    {
        (m.variant, m.profession, m.level) = (variant, profession, level);
    }
    if finalized {
        w.entity_mut(n).insert(crate::bridge::sync_a::VillagerFinalized);
    }
    // `ZombieVillager.finishConversion`: `Villager.refreshBrain` after the profession is given.
    crate::brain::refresh(w, n);
    effects::add(w, n, Kind::Nausea, 0, 200);
    if let Some(at) = at.filter(|_| !silent) {
        w.resource_mut::<Events>().host.level_event(1027, at, 0);
    }
    if let Some(player) = c.starter.and_then(|u| crate::player_uuids::id_of(w, u)) {
        w.resource_mut::<Events>().host.cured_zombie_villager(n, player);
        crate::behaviours::gossip::add(w, n, player, crate::rules::gossip::Kind::MajorPositive, 20);
        crate::behaviours::gossip::add(w, n, player, crate::rules::gossip::Kind::MinorPositive, 25);
    }
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
