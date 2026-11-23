//! The AI's reads of one entity, from its components: the target, the last attacker, the owner, the pauses, the walk and the
//! restriction. Each is one component's field; an entity without the component reads as None, false, or the vanilla default.
use super::host::EcsWorld;
use crate::behaviours::{bed::Sleeping, item_pickup::Pickup, move_towards_restriction::Restriction};
use crate::cow::{LastHurtBy, MoveTo, Nav, SpeciesId, SpeciesTable};
use crate::despawn::Persistent;
use crate::dragon::flight::NoAi;
use crate::goals::AiPaused;
use crate::raids::Aggressive;
use crate::tame::Owner;
use crate::targeting::{Target, Who};
use bevy_ecs::{entity::Entity, world::World};

impl EcsWorld {
    /// The target of `e` (`Target.who`).
    pub fn ai_target(&self, e: Entity) -> Option<Who> {
        ai_target(self.world(), e)
    }

    /// Whether `e` is driven by a brain rather than goals (the `is_brain` of a target-change event).
    pub fn has_brain(&self, e: Entity) -> bool {
        self.world().get::<crate::brain::Brain>(e).is_some()
    }

    /// `NeutralMob.isAngry`: `e` holds an anger target or its anger countdown runs.
    pub fn is_angry(&self, e: Entity) -> bool {
        crate::behaviours::neutral_anger::is_angry(self.world(), e)
    }

    /// The last attacker of `e` (`LastHurtBy.who`).
    pub fn last_hurt_by(&self, e: Entity) -> Option<Who> {
        last_hurt_by(self.world(), e)
    }

    /// The owner of `e` when it is an entity (the summoner of a vex); a player owner is not an entity.
    pub fn vex_owner(&self, e: Entity) -> Option<Entity> {
        match self.world().get::<Owner>(e)?.0? {
            Who::Entity(o) => Some(o),
            Who::Player(_) => None,
        }
    }

    /// `Raider.getCurrentRaid`: None when `e` is no raider, `Some(raid)` otherwise (`raid` None while it is in none).
    pub fn raider_raid(&self, e: Entity) -> Option<Option<i32>> {
        self.world().get::<crate::raids::Raider>(e).map(|r| r.raid)
    }

    /// The living raiders of raid `raid`: entity, wave and health (a query of `Raider`, no scan of the world).
    pub fn raid_members(&self, raid: i32) -> Vec<(Entity, i32, f32)> {
        let w = self.world();
        let Some(q) = w.try_query::<(Entity, &crate::raids::Raider, &crate::cow::Health)>() else { return Vec::new() };
        q.iter_manual(w).filter(|(_, r, _)| r.raid == Some(raid)).map(|(e, r, h)| (e, r.wave, h.0)).collect()
    }

    /// The sections a village point someone holds makes (`PoiManager.isVillageCenter`); empty when there is no registry.
    pub fn village_sections(&self) -> Vec<[i32; 3]> {
        self.world().get_resource::<crate::poi::PoiRegistry>().map_or_else(Vec::new, |r| r.held_sections())
    }
    /// The village points someone holds (job sites, beds, bells); empty when there is no registry.
    pub fn village_points(&self) -> Vec<[i32; 3]> {
        self.world().get_resource::<crate::poi::PoiRegistry>().map_or_else(Vec::new, |r| r.held_points())
    }

    /// The coat of a creature (`Variant`: a fox's type, a rabbit's variant), 0 when it has none set.
    pub fn variant_of(&self, e: Entity) -> u8 {
        self.world().get::<crate::behaviours::variant::Variant>(e).map_or(0, |v| v.0)
    }

    /// `Aggressive`: a raider that attacks for good.
    pub fn aggressive(&self, e: Entity) -> bool {
        aggressive(self.world(), e)
    }

    /// The tame flag and the owner's uuid words of `e`; None when it has no `Owner` (a kind that cannot be tamed). A player
    /// owner reads through the players' uuids, an entity owner through its own uuid; None while the owner has none.
    pub fn tamable_values(&self, e: Entity) -> Option<(bool, Option<[i32; 4]>)> {
        let owner = self.world().get::<Owner>(e)?.0;
        let uuid = match owner {
            Some(Who::Player(p)) => self.player_uuid(p),
            Some(Who::Entity(o)) => self.uuid_words(o),
            None => None,
        };
        Some((owner.is_some(), uuid))
    }

    /// The player that owns `e` (`TamableAnimal.getOwner`); None when it is wild or its owner is a creature.
    pub fn owner_player(&self, e: Entity) -> Option<u32> {
        match self.world().get::<Owner>(e)?.0? {
            Who::Player(p) => Some(p),
            Who::Entity(_) => None,
        }
    }

    /// `Mob.isNoAi`: the flag of vanilla's `Mob.setNoAi` is set on `e`.
    pub fn no_ai(&self, e: Entity) -> bool {
        no_ai(self.world(), e)
    }

    /// The host's AI pause of `e` (`horizon/ai_paused`), which is not vanilla's flag.
    pub fn ai_paused(&self, e: Entity) -> bool {
        self.world().get::<AiPaused>(e).is_some()
    }

    /// The point `e` is walking to, while its `MoveTo` is active.
    pub fn wanted(&self, e: Entity) -> Option<[f64; 3]> {
        self.world().get::<MoveTo>(e).filter(|m| m.active).map(|m| [m.x, m.y, m.z])
    }

    /// `MoveTo.speed_modifier`.
    pub fn move_speed_modifier(&self, e: Entity) -> Option<f64> {
        self.world().get::<MoveTo>(e).map(|m| m.speed_modifier)
    }

    /// No path in progress (an entity without navigation has none).
    pub fn nav_is_done(&self, e: Entity) -> bool {
        nav_is_done(self.world(), e)
    }

    /// `Nav.speed`, the speed modifier of the goal that asked for the path.
    pub fn nav_speed_modifier(&self, e: Entity) -> Option<f64> {
        nav_speed_modifier(self.world(), e)
    }

    /// `Species.malus` of the species of `e` for the path type `kind` (an index below `PATH_TYPES`).
    pub fn malus(&self, e: Entity, kind: u8) -> Option<f32> {
        let w = self.world();
        let s = w.get::<SpeciesId>(e)?;
        *w.get_resource::<SpeciesTable>()?.0.get(usize::from(s.0))?.malus.get(usize::from(kind))?
    }

    /// Whether `e` has a restriction (its home).
    pub fn has_restriction(&self, e: Entity) -> bool {
        has_restriction(self.world(), e)
    }

    /// The block the restriction of `e` is centred on.
    pub fn restrict_center(&self, e: Entity) -> Option<[i32; 3]> {
        restrict_center(self.world(), e)
    }

    /// The radius of the restriction of `e`, in blocks.
    pub fn restrict_radius(&self, e: Entity) -> Option<i32> {
        restrict_radius(self.world(), e)
    }

    /// `Mob.isWithinHome(Vec3)`: `at` is nearer than the radius to the centre of the home block; no restriction (or radius -1) means
    /// anywhere.
    pub fn is_within_restriction(&self, e: Entity, at: [f64; 3]) -> bool {
        is_within_restriction(self.world(), e, at)
    }

    /// The block `e` sleeps in (`Sleeping`).
    pub fn sleeping_pos(&self, e: Entity) -> Option<[i32; 3]> {
        sleeping_pos(self.world(), e)
    }

    /// `Pickup.picks_up` (`mobGriefing`): whether `e` picks up the loot it walks over.
    pub fn can_pick_up_loot(&self, e: Entity) -> Option<bool> {
        can_pick_up_loot(self.world(), e)
    }

    /// `Mob.persistenceRequired`: `e` never despawns on its own.
    pub fn requires_custom_persistence(&self, e: Entity) -> bool {
        requires_custom_persistence(self.world(), e)
    }
}

// The reads of the step view, over the world itself: `EcsWorld`'s methods above delegate to these.

/// The target of `e` (`Target.who`): see `EcsWorld::ai_target`.
pub fn ai_target(w: &World, e: Entity) -> Option<Who> {
    w.get::<Target>(e).map(|t| t.who)
}

/// The last attacker of `e` (`LastHurtBy.who`): see `EcsWorld::last_hurt_by`.
pub fn last_hurt_by(w: &World, e: Entity) -> Option<Who> {
    w.get::<LastHurtBy>(e).map(|l| l.who)
}

/// `Aggressive`: a raider that attacks for good.
pub fn aggressive(w: &World, e: Entity) -> bool {
    w.get::<Aggressive>(e).is_some()
}

/// `Mob.isNoAi`: the flag of vanilla's `Mob.setNoAi` is set on `e`.
pub fn no_ai(w: &World, e: Entity) -> bool {
    w.get::<NoAi>(e).is_some()
}

/// No path in progress (an entity without navigation has none).
pub fn nav_is_done(w: &World, e: Entity) -> bool {
    w.get::<Nav>(e).is_none_or(|n| n.path.is_none())
}

/// `Nav.speed`, the speed modifier of the goal that asked for the path.
pub fn nav_speed_modifier(w: &World, e: Entity) -> Option<f64> {
    w.get::<Nav>(e).map(|n| n.speed)
}

/// Whether `e` has a restriction (its home).
pub fn has_restriction(w: &World, e: Entity) -> bool {
    w.get::<Restriction>(e).is_some_and(|r| r.radius != -1)
}

/// The block the restriction of `e` is centred on.
pub fn restrict_center(w: &World, e: Entity) -> Option<[i32; 3]> {
    w.get::<Restriction>(e).map(|r| r.center)
}

/// The radius of the restriction of `e`, in blocks.
pub fn restrict_radius(w: &World, e: Entity) -> Option<i32> {
    w.get::<Restriction>(e).map(|r| r.radius)
}

/// `Mob.isWithinHome(Vec3)`: see `EcsWorld::is_within_restriction`.
pub fn is_within_restriction(w: &World, e: Entity, at: [f64; 3]) -> bool {
    let Some(r) = w.get::<Restriction>(e) else { return true };
    if r.radius == -1 {
        return true;
    }
    let d = |i: usize| f64::from(r.center[i]) + 0.5 - at[i];
    d(0) * d(0) + d(1) * d(1) + d(2) * d(2) < f64::from(r.radius.wrapping_mul(r.radius))
}

/// The block `e` sleeps in (`Sleeping`).
pub fn sleeping_pos(w: &World, e: Entity) -> Option<[i32; 3]> {
    w.get::<Sleeping>(e).map(|s| s.0)
}

/// `Pickup.picks_up` (`mobGriefing`): whether `e` picks up the loot it walks over.
pub fn can_pick_up_loot(w: &World, e: Entity) -> Option<bool> {
    w.get::<Pickup>(e).map(|p| p.picks_up)
}

/// `Mob.persistenceRequired`: `e` never despawns on its own.
pub fn requires_custom_persistence(w: &World, e: Entity) -> bool {
    w.get::<Persistent>(e).is_some()
}
