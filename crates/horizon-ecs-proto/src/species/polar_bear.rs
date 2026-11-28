//! `PolarBear.registerGoals` (`mobs/polar_bear.rs:25-62`, fiche 68) : la configuration de l'ours, sans sa règle `R:polar_bear`.
//! Les cibles : une cible joueur (la moitié de la portée) quand un ourson est proche (`polar_bear:cub_nearby`), jamais pour un
//! ourson ; un joueur en colère (`neutral:angry`, sans le but qui pose la colère : aucun dans l'ours) ; le renard quand l'ours
//! n'est pas un ourson. Paniques : un ourson fuit toute cause, un adulte les causes d'environnement seulement.
//! DOUTES : la variante de `melee` (se dresser, grogner : `check_and_perform_attack`) n'est pas portée ; `hurt_by` alerte
//! les adultes quand un ourson est frappé (`alert_if_baby`, `goals.rs:2067`) ; la portée de chemin (`.reach()`) des
//! trois cibles n'existe pas dans `nearest_target` ; l'entrée de l'ourson est lue au démarrage seulement (`Entry::when`).
use super::Def;
use crate::behaviours::{neutral_anger, float, follow_parent, hurt_by, look_at_player, melee, nearest_target, panic, random_look, stroll, Cond, Entry};

/// `PolarBearHurtByTargetGoal.alertOther`: adults only.
fn is_adult(w: &bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> bool {
    w.get::<crate::cow::GrowAt>(e).is_none()
}
static BABY: Cond = Cond::Is("horizon:is_baby");
static NOT_BABY: Cond = Cond::Not(&Cond::Is("horizon:is_baby"));
static ADULT_CUB_NEAR: Cond = Cond::All(&[Cond::Not(&Cond::Is("horizon:is_baby")), Cond::Is("polar_bear:cub_nearby")]);
static ANGRY: Cond = Cond::Is("neutral:angry");

pub static DEF: Def = Def {
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 })],
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::PolarBear, ..crate::spawning::SpawnDef::ANIMAL }),
    name: "minecraft:polar_bear",
    caps: Some(super::FREEZE_IMMUNE_CAPS),
    targets: &[
        Entry::new(&neutral_anger::MODULE, 255, &neutral_anger::Params::DEFAULT),
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert_if_baby: true, alert_when: Some(is_adult), ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { players: true, follow_scale: 0.5, interval: 20, ..nearest_target::Params::DEFAULT }).when(&ADULT_CUB_NEAR),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params::PLAYER).when(&ANGRY),
        Entry::new(&nearest_target::MODULE, 4, &nearest_target::Params { types: &["minecraft:fox"], interval: 10, ..nearest_target::Params::DEFAULT }).when(&NOT_BABY),
    ],
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&melee::MODULE, 1, &melee::Params { speed: 1.25, follow_unseen: true, ..melee::Params::DEFAULT }),
        Entry::new(&panic::MODULE, 2, &panic::Params::DEFAULT).when(&BABY),
        Entry::new(&panic::MODULE, 2, &panic::Params { environmental: true, ..panic::Params::DEFAULT }).when(&NOT_BABY),
        Entry::new(&follow_parent::MODULE, 4, &follow_parent::Params { speed: 1.25 }),
        Entry::new(&stroll::MODULE, 5, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params::DEFAULT),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
    ],
    ..Def::EMPTY
};

#[cfg(test)]
mod tests {
    use crate::behaviours::Cond;
    use crate::cow::{GrowAt, Herd, Pos, SpeciesTable, spawn_species};
    use horizon_core::geometry::Vec3 as V;

    /// `polar_bear:cub_nearby`: a cub (a baby of the same species) whose box meets the bear's box inflated by (8, 4, 8). A cub at
    /// 3 blocks makes the adult hunt (témoin); the same cub at 30 blocks does not; an adult alone does not.
    #[test]
    fn a_cub_within_eight_blocks_is_the_hunt_condition() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        }
        let mut h = Herd::with_slab_awake(60, true, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let bear = *table.iter().find(|s| s.name == "minecraft:polar_bear").unwrap();
        let adult = spawn_species(&mut h.world, &bear, 0.5, 0.5, 61);
        let cub = spawn_species(&mut h.world, &bear, 3.5, 0.5, 62);
        h.world.entity_mut(cub).insert(GrowAt(u64::MAX));
        for _ in 0..3 {
            h.tick();
        }
        let hunt = Cond::Is("polar_bear:cub_nearby");
        assert!(hunt.eval(&h.world, adult), "témoin : un ourson à 3 blocks");
        let y = h.world.get::<Pos>(cub).unwrap().0.y;
        h.teleport(cub, V::new(30.5, y, 0.5));
        for _ in 0..3 {
            h.tick();
        }
        assert!(!hunt.eval(&h.world, adult), "un ourson à 30 blocs ne compte pas");
        h.teleport(cub, V::new(3.5, y, 0.5));
        h.tick();
        assert!(hunt.eval(&h.world, adult), "l'ourson revenu près compte de nouveau");
    }
}
