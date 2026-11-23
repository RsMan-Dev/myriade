use crate::blocks::Fluid;
use horizon_core::{geometry::Vec3, random::RandomSource};





#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{DamageKind, Who};

    fn hit(attacker: Option<Who>) -> Entry {
        Entry {
            source: Damage {
                kind: DamageKind::MobAttack,
                attacker,
                from: None,
                attacker_species: None,
            },
            damage: 2.0,
            in_water: false,
            fall_distance: 0.0,
        }
    }

    #[test]
    fn old_hits_are_forgotten_after_100_ticks_or_300_in_a_fight() {
        let mut t = Tracker::default();
        t.record(hit(None), 10);
        t.recheck(true, 110);
        assert_eq!(t.entries.len(), 1, "100 ticks later it still remembers");
        t.recheck(true, 111);
        assert!(t.entries.is_empty());
        t.record(hit(Some(Who::Mob(7))), 200);
        assert!(t.in_combat);
        t.recheck(true, 500);
        assert_eq!(t.entries.len(), 1, "a fight is remembered for 300 ticks");
        t.record(hit(None), 501);
        assert_eq!(t.entries.len(), 1, "the new hit comes after forgetting");
        assert!(!t.in_combat);
    }

    #[test]
    fn a_dead_mob_keeps_its_hits_for_its_death_message() {
        let mut t = Tracker::default();
        t.record(hit(Some(Who::Mob(7))), 0);
        t.recheck(false, 1000);
        assert_eq!(t.entries.len(), 1);
    }
}
