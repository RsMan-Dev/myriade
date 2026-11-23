//! Doors a mob gets through on its own: `DoorInteractGoal` with its two
//! users, `BreakDoorGoal` (zombies that can break doors, vindicators in a
//! raid) and `OpenDoorGoal` (`AbstractIllager.RaiderOpenDoorGoal`).
use super::{
    MobEvent, MobWorld, Species,
    goals::{Cx, Goal, MOVE, goal_clone, reduced},
    nav::NavKind,
    raider,
};
use horizon_core::{geometry::Vec3, random::RandomSource};

/// `DefaultDoorBreakTime`: `BreakDoorGoal.getDoorBreakTime` is
/// `max(240, doorBreakTime)`, and the vindicator's 6 is below it.
const DOOR_BREAK_TIME: i32 = 240;

/// The species that extend `Zombie` (`CanBreakDoors` is theirs to save).
pub fn is_zombie(s: Species) -> bool {
    matches!(
        s,
        Species::Zombie
            | Species::Husk
            | Species::Drowned
            | Species::ZombieVillager
            | Species::ZombifiedPiglin
    )
}
/// `DoorBlock.isWoodenDoor`: a door a hand opens, which is any but iron.
pub fn wooden_door(w: &MobWorld, p: [i32; 3]) -> bool {
    let name = w.block_name(p[0], p[1], p[2]);
    name.ends_with("_door") && name != "minecraft:iron_door"
}
/// `DoorInteractGoal`'s door: where it is and whether one is there.
#[derive(Clone, Debug, Default)]
struct Interact {
    door: [i32; 3],
    has_door: bool,
}
impl Interact {
    /// `DoorInteractGoal.isOpen`: the door is still there and open.
    fn is_open(&mut self, w: &MobWorld) -> bool {
        if !self.has_door {
            return false;
        }
        let d = self.door;
        if !w.block_name(d[0], d[1], d[2]).ends_with("_door") {
            self.has_door = false;
            return false;
        }
        w.prop(d[0], d[1], d[2], "open") == Some("true")
    }
    /// `DoorInteractGoal.setOpen`.
    fn set_open(&self, cx: &mut Cx, open: bool) {
        let d = self.door;
        if self.has_door && cx.w.block_name(d[0], d[1], d[2]).ends_with("_door") {
            cx.events.push(MobEvent::SetDoorOpen { pos: d, open });
        }
    }
}
/// `BlockPos.closerToCenterThan`.
pub(super) fn closer_to_center(p: [i32; 3], at: Vec3, distance: f64) -> bool {
    Vec3::new(
        f64::from(p[0]) + 0.5,
        f64::from(p[1]) + 0.5,
        f64::from(p[2]) + 0.5,
    )
    .distance_squared(at)
        < distance * distance
}

/// `BreakDoorGoal`: crack the door in the way for 240 ticks, then remove it.
/// A zombie does it on hard only; `Vindicator.VindicatorBreakDoorGoal` on
/// normal and hard, in a raid, and holds the mob's movement.
#[derive(Clone, Debug)]
pub struct BreakDoor {
    interact: Interact,
    vindicator: bool,
    break_time: i32,
    last_break_progress: i32,
}
impl BreakDoor {
    /// `Zombie.breakDoorGoal`, added when the zombie can break doors.
    pub fn zombie() -> Self {
        Self::new(false)
    }
    /// `Vindicator.VindicatorBreakDoorGoal`.
    pub fn vindicator() -> Self {
        Self::new(true)
    }
    fn new(vindicator: bool) -> Self {
        Self {
            interact: Interact::default(),
            vindicator,
            break_time: 0,
            last_break_progress: -1,
        }
    }
    /// `DOOR_BREAKING_PREDICATE`.
    fn valid_difficulty(&self, w: &MobWorld) -> bool {
        if self.vindicator {
            w.difficulty.normal || w.difficulty.hard
        } else {
            w.difficulty.hard
        }
    }
}
impl Goal for BreakDoor {
    goal_clone!();
    fn flags(&self) -> u8 {
        if self.vindicator { MOVE } else { 0 }
    }
    fn every_tick(&self) -> bool {
        true
    }
}

/// `AbstractIllager.RaiderOpenDoorGoal` (`OpenDoorGoal` that does not wait
/// to close): open the door in the way during a raid. It never continues, so
/// it closes the door again as soon as it stops.
#[derive(Clone, Debug, Default)]
pub struct RaiderOpenDoor {
    interact: Interact,
}
impl Goal for RaiderOpenDoor {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn every_tick(&self) -> bool {
        true
    }
}

/// `InteractWithDoor.create()`, the piglin brain's core behaviour: while it
/// follows a path, open the doors it is leaving and entering and close the
/// ones it opened or passed through (within 3 blocks, and not the two of the
/// step it takes). It takes no flags and runs every tick, as a core behaviour
/// does. (Another piglin coming through a door is not looked for.)
#[derive(Clone, Debug, Default)]
pub struct InteractWithDoor {
    /// `lastCheckedNode` and `remainingCooldown`.
    last_checked: Option<[i32; 3]>,
    cooldown: i32,
    /// `DOORS_TO_CLOSE`.
    doors: Vec<[i32; 3]>,
}
impl InteractWithDoor {
    /// A door mobs open (`MOB_INTERACTABLE_DOORS`) at `p`, and whether it is open.
    fn door_open(w: &MobWorld, p: [i32; 3]) -> Option<bool> {
        let name = w.block_name(p[0], p[1], p[2]);
        (name.ends_with("_door") && w.has_tag("minecraft:mob_interactable_doors", name))
            .then(|| w.prop(p[0], p[1], p[2], "open") == Some("true"))
    }
}
impl Goal for InteractWithDoor {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn every_tick(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::path::{Path, PathNode, PathType};
    use std::sync::Arc;

    /// A mod's `EntityMobGriefingEvent` answer for every mob.
    struct Answer(bool);
    impl crate::mobs::MobHooks for Answer {
        fn enderman_anger_suppressed(&self, _: u32, _: &crate::mobs::PlayerView) -> bool {
            false
        }
        fn can_grief(&self, _: u32, _: bool) -> bool {
            self.0
        }
        fn grief_hooked(&self) -> bool {
            true
        }
        fn living_visibility(&self, _: &crate::mobs::PlayerView, _: u32, v: f64) -> f64 {
            v
        }
    }


    /// Griefing as `griefing`, and `may` for whether the zombie may destroy
    /// the door at (0, 65, 0).
    struct Door {
        griefing: bool,
        may: bool,
    }
    impl crate::mobs::MobHooks for Door {
        fn enderman_anger_suppressed(&self, _: u32, _: &crate::mobs::PlayerView) -> bool {
            false
        }
        fn can_grief(&self, _: u32, _: bool) -> bool {
            self.griefing
        }
        fn grief_hooked(&self) -> bool {
            true
        }
        fn living_visibility(&self, _: &crate::mobs::PlayerView, _: u32, v: f64) -> f64 {
            v
        }
        fn can_entity_destroy(&self, _: u32, pos: [i32; 3], vanilla: bool) -> bool {
            assert_eq!(pos, [0, 65, 0]);
            assert!(vanilla);
            self.may
        }
    }
}
