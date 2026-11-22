//! `PiglinSpecificSensor` (javap `ai/sensing/PiglinSpecificSensor`, `doTick`): every scan, one pass over the visible entities fills the
//! piglin memories. A baby hoglin, an adult huntable hoglin (no `HUNTED_RECENTLY`), the first baby piglin (the nemesis), the first
//! zombified piglin; the adult piglins and brutes are listed and counted, and those within 8 blocks are the nearby ones. The repellent
//! is the `repellent_and_adult` sensor's. The nearest targetable player not wearing piglin-safe armour and the nearest player holding
//! a loved item (main or off hand) are read from the players' worn and held items (`equipment::worn_by_player`).
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::Pos;
use crate::equipment::worn_by_player;
use crate::goals::{d2, Slot};
use crate::items;
use crate::targeting::{info, Who};

pub static SENSOR: Sensor = Sensor {
    name: "minecraft:piglin_specific",
    mems: &[
        &NEAREST_VISIBLE_NEMESIS,
        &NEAREST_VISIBLE_HUNTABLE_HOGLIN,
        &NEAREST_VISIBLE_BABY_HOGLIN,
        &NEAREST_VISIBLE_ZOMBIFIED,
        &NEARBY_ADULT_PIGLINS,
        &NEAREST_VISIBLE_ADULT_PIGLINS,
        &VISIBLE_ADULT_PIGLIN_COUNT,
        &VISIBLE_ADULT_HOGLIN_COUNT,
        &NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD,
        &NEAREST_PLAYER_HOLDING_WANTED_ITEM,
    ],
    scan,
    requires: &[Cap::Targets],
    ..Sensor::DEFAULT
};

/// `PiglinSpecificSensor` reads `NearestVisibleLivingEntities`: the empty list when nothing is visible.
fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let visible = match get(w, e, &VISIBLE_MOBS) {
        Some(MemValue::Whos(list)) => list.clone(),
        _ => Vec::new(),
    };
    let table = w.resource::<SpeciesTable>().0.clone();
    let id = |name: &str| table.iter().find(|s| s.name == name).map(|s| s.id);
    let (hoglin, piglin, brute, zombified) = (id(crate::species::HOGLIN), id(crate::species::PIGLIN), id("minecraft:piglin_brute"), id("minecraft:zombified_piglin"));
    let (mut baby_hoglin, mut huntable, mut nemesis, mut zombie) = (None, None, None, None);
    let (mut adults, mut adult_hoglins) = (Vec::new(), 0i64);
    for who in visible {
        let Some(i) = info(w, who) else { continue };
        if matches!(who, Who::Player(_)) {
            continue;
        }
        if Some(i.species) == hoglin {
            if i.baby {
                baby_hoglin.get_or_insert(who);
            } else {
                adult_hoglins += 1;
                if let (None, Who::Entity(h)) = (huntable, who) {
                    if !present(w, h, &HUNTED_RECENTLY) {
                        huntable = Some(who);
                    }
                }
            }
        } else if Some(i.species) == brute {
            adults.push(who);
        } else if Some(i.species) == piglin {
            if i.baby {
                nemesis.get_or_insert(who);
            } else {
                adults.push(who);
            }
        } else if Some(i.species) == zombified {
            zombie.get_or_insert(who);
        }
    }
    let me = w.get::<Pos>(e).map(|p| p.0);
    let nearby: Vec<Who> = adults.iter().copied().filter(|&who| me.zip(info(w, who)).is_some_and(|(p, i)| d2(p, i.pos) <= 64.0)).collect();
    put(w, e, &NEAREST_VISIBLE_NEMESIS, nemesis);
    put(w, e, &NEAREST_VISIBLE_HUNTABLE_HOGLIN, huntable);
    put(w, e, &NEAREST_VISIBLE_BABY_HOGLIN, baby_hoglin);
    put(w, e, &NEAREST_VISIBLE_ZOMBIFIED, zombie);
    let players = match get(w, e, &NEAREST_VISIBLE_TARGETABLE_PLAYERS) {
        Some(MemValue::Whos(list)) => list.clone(),
        _ => Vec::new(),
    };
    let bare = nearest_player(w, e, &players, |w, p| !wears_safe_armor(w, p));
    let holding = nearest_player(w, e, &players, |w, p| holds_loved(w, p));
    put(w, e, &NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD, bare);
    put(w, e, &NEAREST_PLAYER_HOLDING_WANTED_ITEM, holding);
    set(w, e, &NEARBY_ADULT_PIGLINS, MemValue::Whos(nearby), None);
    set(w, e, &VISIBLE_ADULT_PIGLIN_COUNT, MemValue::Int(adults.len() as i64), None);
    set(w, e, &VISIBLE_ADULT_HOGLIN_COUNT, MemValue::Int(adult_hoglins), None);
    set(w, e, &NEAREST_VISIBLE_ADULT_PIGLINS, MemValue::Whos(adults), None);
}

/// A memory holding one entity, erased when there is none (`Brain.setMemory(key, Optional.empty())`).
fn put(w: &mut World, e: Entity, k: &MemKey, who: Option<Who>) {
    match who {
        Some(who) => set(w, e, k, MemValue::Who(who), None),
        None => erase(w, e, k),
    }
}

/// The nearest player of `list` that passes `pred`, by its position from the piglin.
fn nearest_player(w: &World, e: Entity, list: &[Who], pred: impl Fn(&World, u32) -> bool) -> Option<Who> {
    let me = w.get::<Pos>(e)?.0;
    list.iter()
        .filter_map(|&who| match who {
            Who::Player(p) if pred(w, p) => info(w, who).map(|i| (d2(me, i.pos), who)),
            _ => None,
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, who)| who)
}

/// Any armour slot holds an item of the `minecraft:piglin_safe_armor` tag.
fn wears_safe_armor(w: &World, p: u32) -> bool {
    [Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet].into_iter().any(|s| items::has_tag("minecraft:piglin_safe_armor", worn_by_player(w, p, s)))
}

/// The main or off hand holds a loved item (`minecraft:piglin_loved`).
fn holds_loved(w: &World, p: u32) -> bool {
    [Slot::Main, Slot::Off].into_iter().any(|s| items::has_tag("minecraft:piglin_loved", worn_by_player(w, p, s)))
}
