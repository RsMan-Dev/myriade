//! The hook taken back by its owner (`FishingHook.retrieve`), as the host reads it: the creature to pull, the catch and the rod's
//! damage. The host rolls the loot and gives the pull and the xp; the hook is gone once read. A target removed since the hook
//! took it lets the hook fall back: it is then a catch or nothing, never a pull.
use crate::cow::*;
use crate::fishing_hook::{Hook, Phase, Target};
use crate::goals::Online;
use crate::item::Item;
use crate::rules::fishing_hook::reel_damage;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// What the host applies for one hook taken back.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Retrieve {
    /// The damage to the rod: 5 on a creature, 3 on an item, 1 (2 on the ground) for a catch, 2 on the ground otherwise.
    pub damage_item: u32,
    /// The creature or the player still hooked, to pull towards the owner.
    pub pulled: Option<Who>,
    /// The hook held a bite and no target: a catch.
    pub caught: bool,
    /// Where the hook is, and whether it is in open water (the loot's table).
    pub at: Vec3,
    pub open_water: bool,
    /// The rod's Luck of the Sea.
    pub luck: u8,
}

/// The target a hook still holds: a creature or an item still alive in the world, or a player still online.
fn held(w: &World, t: Target) -> Option<Target> {
    let alive = match t {
        Target::Entity(c) => w.get_entity(c).is_ok() && !w.get::<Health>(c).is_some_and(|hp| hp.0 <= 0.0),
        Target::Player(id) => w.get_resource::<Online>().is_some_and(|o| o.get(id).is_some()),
    };
    alive.then_some(t)
}

/// Reads the hook `hook` of `player` and takes it back. Nothing when the hook is not theirs; the hook is despawned otherwise.
pub fn retrieve(w: &mut World, hook: Entity, player: u32) -> Retrieve {
    let Some(h) = w.get::<Hook>(hook) else { return Retrieve::default() };
    if h.owner != player {
        return Retrieve::default();
    }
    let (bite, on_ground, open_water, luck) = (h.nibble > 0, h.on_ground, h.open_water, h.luck);
    let at = w.get::<Pos>(hook).map_or(Vec3::ZERO, |p| p.0);
    let target = match h.phase {
        Phase::Hooked(t) => held(w, t),
        _ => None,
    };
    let hooked = match target {
        Some(Target::Entity(c)) => Some(w.get::<Item>(c).is_some()),
        Some(Target::Player(_)) => Some(false),
        None => None,
    };
    let pulled = match target {
        Some(Target::Entity(c)) => Some(Who::Entity(c)),
        Some(Target::Player(id)) => Some(Who::Player(id)),
        None => None,
    };
    let damage = reel_damage(hooked, bite, on_ground);
    let caught = target.is_none() && bite;
    w.despawn(hook);
    Retrieve { damage_item: damage as u32, pulled, caught, at, open_water, luck }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fishing_hook::PARAMS;
    use horizon_core::random::LegacyRandom;

    fn hook(owner: u32, phase: Phase, nibble: i32, on_ground: bool) -> Hook {
        Hook {
            params: &PARAMS,
            owner,
            phase,
            rng: LegacyRandom::new(1),
            lure: 0,
            luck: 0,
            tick_count: 0,
            life: 0,
            on_ground,
            out_of_water: 0,
            open_water: false,
            nibble,
            time_until_lured: 0,
            time_until_hooked: 0,
            fish_angle: 0.0,
            deflect: None,
            last_deflected: None,
        }
    }

    #[test]
    fn bitten_hook_is_a_catch_and_is_taken_back() {
        let mut w = World::new();
        let e = w.spawn(hook(7, Phase::Bobbing, 5, false)).id();
        let r = retrieve(&mut w, e, 7);
        assert_eq!((r.damage_item, r.pulled, r.caught), (1, None, true));
        assert!(w.get_entity(e).is_err());
    }

    #[test]
    fn hook_of_a_removed_target_falls_back() {
        let mut w = World::new();
        let c = w.spawn_empty().id();
        w.despawn(c);
        let e = w.spawn(hook(7, Phase::Hooked(Target::Entity(c)), 0, false)).id();
        let r = retrieve(&mut w, e, 7);
        assert_eq!((r.damage_item, r.pulled, r.caught), (0, None, false));
    }

    #[test]
    fn hooked_creature_is_pulled_and_costs_five() {
        let mut w = World::new();
        let c = w.spawn_empty().id();
        let e = w.spawn(hook(7, Phase::Hooked(Target::Entity(c)), 0, false)).id();
        let r = retrieve(&mut w, e, 7);
        assert_eq!((r.damage_item, r.pulled, r.caught), (5, Some(Who::Entity(c)), false));
    }

    #[test]
    fn other_players_hook_is_left_alone() {
        let mut w = World::new();
        let e = w.spawn(hook(7, Phase::Bobbing, 5, false)).id();
        assert_eq!(retrieve(&mut w, e, 8), Retrieve::default());
        assert!(w.get_entity(e).is_ok());
    }
}
