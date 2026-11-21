//! The AI's reads of one entity, from its components: the target, the last attacker, the owner, the pauses, the walk and the
//! restriction. Each is one component's field; an entity without the component reads as None, false, or the vanilla default.
use super::host::EcsWorld;
use crate::behaviours::{bed::Sleeping, item_pickup::Pickup, move_towards_restriction::Restriction};
use crate::cow::{LastHurtBy, MoveTo, Nav, SpeciesId, SpeciesTable};
use crate::despawn::Persistent;
use crate::dragon::flight::NoAi;
use crate::raids::Aggressive;
use crate::tame::Owner;
use crate::targeting::{Target, Who};
use bevy_ecs::entity::Entity;

impl EcsWorld {
    /// The target of `e` (`Target.who`).
    pub fn ai_target(&self, e: Entity) -> Option<Who> {
        self.world().get::<Target>(e).map(|t| t.who)
    }

    /// The last attacker of `e` (`LastHurtBy.who`).
    pub fn last_hurt_by(&self, e: Entity) -> Option<Who> {
        self.world().get::<LastHurtBy>(e).map(|l| l.who)
    }

    /// The owner of `e` when it is an entity (the summoner of a vex); a player owner is not an entity.
    pub fn vex_owner(&self, e: Entity) -> Option<Entity> {
        match self.world().get::<Owner>(e)?.0? {
            Who::Entity(o) => Some(o),
            Who::Player(_) => None,
        }
    }

    /// `Aggressive`: a raider that attacks for good.
    pub fn aggressive(&self, e: Entity) -> bool {
        self.world().get::<Aggressive>(e).is_some()
    }

    /// `Mob.isNoAi`: the AI of `e` is paused.
    pub fn ai_paused(&self, e: Entity) -> bool {
        self.world().get::<NoAi>(e).is_some()
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
        self.world().get::<Nav>(e).is_none_or(|n| n.path.is_none())
    }

    /// `Nav.speed`, the speed modifier of the goal that asked for the path.
    pub fn nav_speed_modifier(&self, e: Entity) -> Option<f64> {
        self.world().get::<Nav>(e).map(|n| n.speed)
    }

    /// `Species.malus` of the species of `e` for the path type `kind` (an index below `PATH_TYPES`).
    pub fn malus(&self, e: Entity, kind: u8) -> Option<f32> {
        let w = self.world();
        let s = w.get::<SpeciesId>(e)?;
        *w.get_resource::<SpeciesTable>()?.0.get(usize::from(s.0))?.malus.get(usize::from(kind))?
    }

    /// Whether `e` has a restriction (its home).
    pub fn has_restriction(&self, e: Entity) -> bool {
        self.world().get::<Restriction>(e).is_some()
    }

    /// The block the restriction of `e` is centred on.
    pub fn restrict_center(&self, e: Entity) -> Option<[i32; 3]> {
        self.world().get::<Restriction>(e).map(|r| r.center)
    }

    /// The radius of the restriction of `e`, in blocks.
    pub fn restrict_radius(&self, e: Entity) -> Option<i32> {
        self.world().get::<Restriction>(e).map(|r| r.radius)
    }

    /// `Mob.isWithinHome(Vec3)`: `at` is nearer than the radius to the centre of the home block; no restriction (or radius -1) means
    /// anywhere.
    pub fn is_within_restriction(&self, e: Entity, at: [f64; 3]) -> bool {
        let Some(r) = self.world().get::<Restriction>(e) else { return true };
        if r.radius == -1 {
            return true;
        }
        let d = |i: usize| f64::from(r.center[i]) + 0.5 - at[i];
        d(0) * d(0) + d(1) * d(1) + d(2) * d(2) < f64::from(r.radius) * f64::from(r.radius)
    }

    /// The block `e` sleeps in (`Sleeping`).
    pub fn sleeping_pos(&self, e: Entity) -> Option<[i32; 3]> {
        self.world().get::<Sleeping>(e).map(|s| s.0)
    }

    /// `Pickup.picks_up` (`mobGriefing`): whether `e` picks up the loot it walks over.
    pub fn can_pick_up_loot(&self, e: Entity) -> Option<bool> {
        self.world().get::<Pickup>(e).map(|p| p.picks_up)
    }

    /// `Mob.persistenceRequired`: `e` never despawns on its own.
    pub fn requires_custom_persistence(&self, e: Entity) -> bool {
        self.world().get::<Persistent>(e).is_some()
    }
}
