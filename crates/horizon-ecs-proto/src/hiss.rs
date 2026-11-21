//! A creature that hisses at what scares off a flier (`Cat.hiss`, called by `Phantom.PhantomSweepAttackGoal.canContinueToUse` for each
//! cat within 16 blocks of the phantom, every 20 ticks). The rule `HISSES` gives a species the marker `Hisses` with its sound; nothing
//! ticks: the diving flier asks `scare` at its own pace and only the creatures near it are visited, through the spatial index.
use crate::cow::Bounds;
use crate::rules::{no_attrs, no_interact, params, Rule};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// `Cat.getSoundSet().hissSound()` of the default (classic) set; the other sets are not kept by the ECS.
pub const CAT_HISS: Voice = Voice { adult: "minecraft:entity.cat.hiss", baby: "minecraft:entity.baby_cat.hiss" };
/// The sounds of a hissing species: the adult's and the baby's (`getSoundSet`: `isBaby`).
#[derive(Clone, Copy, Debug)]
pub struct Voice {
    pub adult: &'static str,
    pub baby: &'static str,
}
/// `Phantom...canContinueToUse`: the box of the flier grown by this many blocks.
pub const SCARE_RANGE: f64 = 16.0;
/// The ticks between two searches (`catSearchTick = tickCount + 20`).
pub const SEARCH_EVERY: u64 = 20;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Hisses(pub &'static Voice);
/// The own time of the flier's next search (`catSearchTick`), on the flier.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct NextSearch(pub u64);

/// The rule `hisses`, parameter: the sound.
pub static HISSES: Rule = Rule {
    name: "hisses",
    equip: |w, e, p| {
        w.entity_mut(e).insert(Hisses(params::<Voice>(p)));
    },
    attrs: no_attrs,
    interact: no_interact,
};

/// `Cat.hiss`: the creature `e` makes its sound where it is.
pub fn hiss(w: &mut World, e: Entity) {
    let Some(v) = w.get::<Hisses>(e).map(|h| h.0) else { return };
    let baby = w.get::<crate::cow::GrowAt>(e).is_some();
    let sound = if baby { v.baby } else { v.adult };
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return };
    let c = b.bottom_center();
    let at = [c.x, c.y + (b.max_position().y - b.min_position().y) / 2.0, c.z];
    // `getVoicePitch`: (r1 - r2) * 0.2 + 1.5 for a baby, + 1.0 otherwise.
    let base = if baby { 1.5 } else { 1.0 };
    let pitch = w.get_mut::<crate::cow::Rng>(e).map_or(1.0, |mut r| {
        let a = r.next_float();
        (a - r.next_float()) * 0.2 + base
    });
    w.resource_mut::<crate::goals::Events>().host.sound(sound, at, 1.0, pitch);
}

/// The creatures that hiss within `SCARE_RANGE` of `flier`'s box and are alive.
pub fn hissers_near(w: &World, flier: Entity) -> Vec<Entity> {
    let Some(b) = w.get::<Bounds>(flier).map(|b| b.0) else { return Vec::new() };
    let r = Vec3::new(SCARE_RANGE, SCARE_RANGE, SCARE_RANGE);
    crate::area::creatures_in(w, b.inflate(r), flier)
        .into_iter()
        .filter(|&c| w.get::<Hisses>(c).is_some() && !crate::tear_down::is_dead(w, c))
        .collect()
}

/// The search of `flier` when its own time `now` has come: every hisser near hisses; true when there was one (the flier is scared).
/// None while the next search is not due (the flier keeps what the last one said).
pub fn scare(w: &mut World, flier: Entity, now: u64) -> Option<bool> {
    if w.get::<NextSearch>(flier).is_some_and(|n| now <= n.0) {
        return None;
    }
    w.entity_mut(flier).insert(NextSearch(now + SEARCH_EVERY));
    let cats = hissers_near(w, flier);
    for &c in &cats {
        hiss(w, c);
    }
    Some(!cats.is_empty())
}
