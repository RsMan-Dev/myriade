//! `SulfurCube` (`mobs/sulfur_cube.rs`, `cube.rs`). Its move is the hop (ground). Goals: `float` (1); `swallow_item` (2) takes a
//! swallowable item and holds it; `burn_on_touch` (2) burns the creature a hot cube touches (event-driven, no scan); `push_on_touch`
//! (2) shoves when a player walks in; `tempt` (3) by the swallowable items; `item_seek` (3) turns toward a searched item; `bounce_drag`
//! (0) applies the held archetype's friction, air drag and bounce. Size: `SizeParams` of the slime without a split (a cube never splits).
//! Constants are inline below. Priorities are NOT verified: no decompiled vanilla source is in the worktree or under the projects folder.
//! Not wired here: the fuse (`blast_fuse::ignite`, socle: flint and steel or burning arrow), the NBT (`swallow_hold::save`, socle).
use super::Def;
use crate::behaviours::{bounce_drag, burn_on_touch, float, item_seek, push_on_touch, swallow_item, tempt, Entry};
use crate::locomotion::{hop, Medium};
use crate::behaviours::shear::ShearDef;
use crate::behaviours::swallow_hold::{release, Held, SearchAfter};
use crate::rules::RuleEntry;
use crate::rules::size::HealthLaw;
use crate::size::{SizeParams, SIZE};

/// `SulfurCube.shear`: the held block comes out (an item thrown from the top of its body) and it does not search for 100 ticks.
/// The item leaves from the default passenger attachment of the jar (the body's height above the feet, `EntityAttachments.createDefault`).
pub static SHEARING: ShearDef = ShearDef {
    equip: |_, _| {},
    ready: |w, e| w.get::<Held>(e).is_some(),
    apply: |w, e| {
        let Some(item) = release(w, e) else { return false };
        let until = crate::cow::now_of(w, e) + 100;
        w.entity_mut(e).insert(SearchAfter(until));
        let Some(at) = w.get::<crate::cow::Pos>(e).map(|p| p.0) else { return true };
        let top = w.get::<crate::cow::Body>(e).map_or(1.0, |b| f64::from(b.height));
        crate::behaviours::shear::drop_stacks(w, vec![horizon_client_lib::items::ItemStack::new(&item, 1)], [at.x, at.y + top, at.z]);
        true
    },
    table: |_, _| None,
    sound: "minecraft:entity.sulfur_cube.eject",
    height: Some(0.0),
    puff: None,
};

/// The items a cube swallows: the archetype tags, resolved at runtime. Partial list: only the item names written in
/// `mobs/sulfur_cube.rs` are known offline (`tag` contents not verifiable here).
const SWALLOWABLE: &[&str] = &["minecraft:magma_block", "minecraft:tnt"];

pub static DEF: Def = Def {
    name: "minecraft:sulfur_cube",
    goals: &[
        Entry::new(&bounce_drag::MODULE, 0, &bounce_drag::Params::DEFAULT),
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&swallow_item::MODULE, 2, &swallow_item::Params { items: SWALLOWABLE }),
        Entry::new(&burn_on_touch::MODULE, 2, &burn_on_touch::Params::DEFAULT),
        Entry::new(&push_on_touch::MODULE, 2, &push_on_touch::Params::DEFAULT),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.25, items: SWALLOWABLE, stop: 2.5, range: 10.0, look_y: 95.0, calm: 100, ..tempt::Params::DEFAULT }),
        Entry::new(&item_seek::MODULE, 3, &item_seek::Params::DEFAULT),
    ],
    rules: &[RuleEntry::new(&SIZE, &SizeParams { split: None, health: HealthLaw::PerSize(4.0), ..SizeParams::SLIME }), RuleEntry::new(&crate::behaviours::shear::SHEAR, &SHEARING), RuleEntry::new(&crate::hand_swallow::HAND_SWALLOW, &crate::hand_swallow::SwallowParams::SULFUR_CUBE), RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT), RuleEntry::new(&crate::leash::GATE, &crate::leash::GateParams(crate::leash::holds_body))],
    locomotion: &[(Medium::Ground, Entry::new(&hop::MODULE, 0, &hop::Params::SLIME))],
    draws: Some(&crate::behaviours::spawn_draw::SPAWN_SIZE_BY_AGE),
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
