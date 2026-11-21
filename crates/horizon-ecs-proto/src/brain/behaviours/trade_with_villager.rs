//! `TradeWithVillager` (`villager_brain.rs:1623`, `:2909`): while its interaction target is a villager of its own species that it still
//! sees, a villager looks at it, and within 5 blocks it meets it (`VillagerMeet`: the gossips go round by `gossip::meet`). The duration
//! is the entry's (60 ticks).
use crate::behaviours::food_inventory::Stock;
use crate::behaviours::food_stock as stock;
use crate::behaviours::inventory;
use crate::behaviours::trade_offers::{Merchant, TradeData};
use horizon_client_lib::mobs::villager::food_points;
use crate::behaviours::gossip;
use crate::behaviours::play_tag::INTERACTION_TARGET;
use crate::brain::*;
use crate::cow::{Pos, SpeciesId};
use crate::goals::d2;
use crate::rules::trade_with_villager::{is_thrown, thrown_count, toss_velocity};
use crate::targeting::{info, Who};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_trade_with_villager",
    mems: &[&INTERACTION_TARGET, &LOOK_TARGET, &VISIBLE_MOBS],
    can_start,
    start,
    tick,
    ..Behaviour::DEFAULT
};

/// The interaction target, when it is a villager of the own species that is still seen.
fn partner(w: &mut World, e: Entity) -> Option<Entity> {
    let other = match get(w, e, &INTERACTION_TARGET) {
        Some(MemValue::Who(Who::Entity(o))) => *o,
        _ => return None,
    };
    let own = w.get::<SpeciesId>(e).map(|s| s.0);
    let who = Who::Entity(other);
    (info(w, who).is_some_and(|i| Some(i.species) == own) && visible_contains(w, e, who)).then_some(other)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    partner(w, e).is_some()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    gaze(w, e);
}

fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(other) = partner(w, e) else { return false };
    gaze(w, e);
    let (Some(me), Some(at)) = (w.get::<Pos>(e).map(|p| p.0), info(w, Who::Entity(other)).map(|i| i.pos)) else { return false };
    if d2(me, at) <= 5.0 {
        if gossip::meet(w, e, other) {
            crate::behaviours::summon_defender::check(w, e, &crate::behaviours::summon_defender::MEETING);
        }
        share(w, e, other, me, at);
    }
    true
}

/// `TradeWithVillager.tick` after the gossip: surplus food (to anyone for a farmer, else to a villager that wants more), a farmer's
/// wheat over 32, and the wares the other's profession requests and ours does not, each as the half of a big stack
/// (`throwHalfStack`: more than half a stack gives half of it, more than 24 gives what is over 24), tossed toward the other.
fn share(w: &mut World, e: Entity, other: Entity, me: horizon_core::geometry::Vec3, at: horizon_core::geometry::Vec3) {
    let items = stock::items(w);
    let Some(td) = w.get_resource::<TradeData>() else { return };
    let (farmer, mine, theirs) = {
        let prof = |x: Entity| w.get::<Merchant>(x).map_or(0, |m| usize::from(m.profession));
        let req = |x: Entity| td.trades.professions.entry(prof(x)).map(|p| p.requested.clone()).unwrap_or_default();
        (td.trades.professions.entry(prof(e)).is_some_and(|p| p.id.as_str().ends_with("farmer")), req(e), req(other))
    };
    let max = |n: &str| u32::from(items.get(n).map_or(64, |i| i.max_stack) as u8).max(1);
    let mut thrown = Vec::new();
    if stock::has_excess(w, e, items) && (farmer || stock::wants_more(w, other, items)) {
        thrown.extend(take_half(w, e, |n| food_points(items, n) > 0, &max));
    }
    if farmer && inventory::count(w, e, "minecraft:wheat") > 32 {
        thrown.extend(take_half(w, e, |n| n == "minecraft:wheat", &max));
    }
    let wanted: Vec<&String> = theirs.iter().filter(|i| !mine.contains(i)).collect();
    if !wanted.is_empty() {
        thrown.extend(take_half(w, e, |n| wanted.iter().any(|x| x.as_str() == n), &max));
    }
    let eye = w.get::<crate::goals::Look>(e).map_or(1.62, |l| l.eye);
    let k = f64::from(0.3_f32);
    let from = horizon_core::geometry::Vec3::new(me.x, me.y + eye - k, me.z);
    let v = toss_velocity(me, at, k);
    for (name, n) in thrown {
        crate::item::drop_item(w, horizon_client_lib::items::ItemStack::new(&name, u32::from(n)), from, v, 10);
    }
}

fn take_half(w: &mut World, e: Entity, wanted: impl Fn(&str) -> bool, max: &dyn Fn(&str) -> u32) -> Option<(String, u8)> {
    let mut s = w.get::<Stock>(e)?.clone();
    let i = s.0.iter().position(|(n, c)| *c > 0 && wanted(n) && is_thrown(*c, max(n)))?;
    let (name, c) = s.0[i].clone();
    let n = thrown_count(c, max(&name));
    s.0[i].1 -= n;
    s.0.retain(|(_, c)| *c > 0);
    if s.0.is_empty() {
        w.entity_mut(e).remove::<Stock>();
    } else {
        w.entity_mut(e).insert(s);
    }
    Some((name, n))
}

fn gaze(w: &mut World, e: Entity) {
    let who = match get(w, e, &INTERACTION_TARGET) {
        Some(MemValue::Who(who)) => *who,
        _ => return,
    };
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(who, true)), None);
}
