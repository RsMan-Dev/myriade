//! Vexes (`Vex`): summoned by evokers, flying through walls around their
//! bound origin, charging at their target, and wasting away after their
//! limited life.
use super::{
    Damage, DamageKind, MobEvent, Species, Who,
    anim::{Poses, State},
    control::Operation,
    goals::{
        Cx, Float, Goal, HurtByTarget, LookAtPlayer, MOVE, NearestTarget, goal_clone, reduced,
    },
};
use crate::health::DamageLevel;
use horizon_core::geometry::trig::Mth;
use horizon_core::{geometry::Vec3, random::RandomSource};
use std::f32::consts::PI;

// Vex state: `flag` charging, `counter2` the owner's mob id (0 none),
// `home` the bound origin, `egg_time` the life left (-1 unlimited).

/// `VexChargeAttackGoal`.
#[derive(Clone, Debug)]
struct Charge;
impl Goal for Charge {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `VexRandomMoveGoal`: drift to an empty block near the bound origin.
#[derive(Clone, Debug)]
struct RandomMove;
impl Goal for RandomMove {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}
/// `VexCopyOwnerTargetGoal`: the evoker's target.
#[derive(Clone, Debug)]
struct CopyOwnerTarget;
impl Goal for CopyOwnerTarget {
    goal_clone!();
    fn flags(&self) -> u8 {
        super::goals::TARGET
    }
}
/// `TargetingConditions.test` for the owner's target: `canBeSeenByAnyone`
/// leaves out the dead and spectators.
fn copyable(w: &crate::mobs::MobWorld, t: Who) -> bool {
    match t {
        Who::Player => w.player.as_ref().is_some_and(|p| p.alive && !p.spectator),
        Who::Mob(id) => w.seen(id).is_some_and(|s| s.alive),
    }
}



#[cfg(test)]
mod owner_save_tests {
    use super::*;
    use crate::entity_nbt::{mob_from_nbt, mob_to_nbt};
    use crate::mobs::WhoLookup;
    use horizon_core::nbt::Tag;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// A vex with the id `EVOKER` in the level: what `Vex.setOwner` records
    /// when an evoker summons it.
    const EVOKER: u32 = 7;


    /// `Level.getEntity(uuid)` for the test: `Game` answers this with
    /// `Game::who_by_uuid`.
    struct Level(HashMap<[i32; 4], Who>);
    impl WhoLookup for Level {
        fn who_by_uuid(&self, uuid: [i32; 4]) -> Option<Who> {
            self.0.get(&uuid).copied()
        }
    }

    /// `Vex.addAdditionalSaveData` stores its owner reference's UUID
    /// (`EntityReference.store`, nothing at all without an owner) and
    /// `readAdditionalSaveData` reads it back (`EntityReference.read`), so a
    /// summoned vex keeps the evoker whose target it copies across a chunk
    /// save. Horizon models the owner as the owner's mob id and wrote neither,
    /// so a reloaded vex had no owner to copy a target from.
    #[test]
    fn a_vexs_owner_uuid_survives_the_chunk_save() {
        let evoker_uuid = [3, 1, 4, 1];
        let mut summoned = vex();
        assert_eq!(owner(&summoned, None), None, "a vex nobody summoned");
        summoned.extra.counter2 = EVOKER as i32;
        assert_eq!(owner(&summoned, None), Some(EVOKER), "the evoker's id");
        let uuid_of = |id: u32| (id == EVOKER).then_some(evoker_uuid);
        let saved = mob_to_nbt(&summoned, [0; 4], None, None, uuid_of);
        assert_eq!(
            saved.get("owner"),
            Some(&Tag::IntArray(evoker_uuid.to_vec()))
        );
        // The loaded vex knows the UUID: no id survives a save, and its owner
        // is found again only through the level (`EntityReference.getEntity`).
        let types = [("minecraft:vex".into(), summoned.kind.clone())].into();
        let back = mob_from_nbt(&saved, &types, 2, None).unwrap();
        assert_eq!(back.extra.owner, Some(evoker_uuid));
        assert_eq!(back.extra.counter2, 0);
        let level = Level(HashMap::from([(evoker_uuid, Who::Mob(42))]));
        assert_eq!(owner(&back, Some(&level)), Some(42), "the evoker, found");
        // An owner the level does not have, and a UUID naming the player, are
        // no owner: `Vex.setOwner` takes a `Mob`.
        assert_eq!(owner(&back, Some(&Level(HashMap::new()))), None);
        let player = Level(HashMap::from([(evoker_uuid, Who::Player)]));
        assert_eq!(owner(&back, Some(&player)), None);
        assert_eq!(owner(&back, None), None, "a level with no UUIDs");
        // A save read again keeps the reference even when nothing in the level
        // answers for it, as `EntityReference` does.
        let again = mob_to_nbt(&back, [0; 4], None, None, |_| None);
        assert_eq!(
            again.get("owner"),
            Some(&Tag::IntArray(evoker_uuid.to_vec()))
        );
    }

    /// `EntityReference.store` writes nothing for a vex with no owner, and a
    /// save that names none loads with none - neither the `owner` key a loaded
    /// compound carried nor an id left over may stand.
    #[test]
    fn a_vex_with_no_owner_saves_no_owner_key() {
        let mut loose = vex();
        let stale = Tag::compound([
            ("id", Tag::String("minecraft:vex".into())),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.5_f64.to_bits()),
                    Tag::Double(64.0_f64.to_bits()),
                    Tag::Double(0.5_f64.to_bits()),
                ]),
            ),
            ("owner", Tag::IntArray(vec![9, 9, 9, 9])),
        ]);
        let types = [("minecraft:vex".into(), loose.kind.clone())].into();
        let loaded = mob_from_nbt(&stale, &types, 3, None).unwrap();
        assert_eq!(loaded.extra.owner, Some([9, 9, 9, 9]));
        loose.extra.owner = None;
        loose.extra.counter2 = 0;
        let saved = mob_to_nbt(&loose, [0; 4], Some(&stale), None, |_| None);
        assert_eq!(saved.get("owner"), None, "no owner, no key");
    }
}
