//! `Rolled` (fiche 19) : tant que l'animal est en boule (état autre que IDLE), ce but tient MOVE et LOOK à la priorité 2, ce
//! qui empêche les buts de marche et de regard de plus haut numéro de démarrer. Il n'a aucun tick et aucun système : seul
//! `set_state` le réveille ou l'arrête, donc coût nul.
use super::armadillo_state::{self as st, IDLE};
use super::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Self = Self {};
}

pub static MODULE: Module = Module {
    name: "rolled",
    flags: MOVE | LOOK,
    equip,
    can_use,
    requires: &[Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {
    st::ensure(w, e);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    st::of(w, e) != IDLE
}
/// Écrit l'état ; un état autre que IDLE demande le but, IDLE l'arrête (les buts qui attendaient ses drapeaux repartent).
pub fn set_state(w: &mut World, e: Entity, state: u8) {
    st::ensure(w, e);
    if let Some(mut s) = w.get_mut::<st::ArmadilloState>(e) {
        s.0 = state;
    }
    if state == IDLE {
        end(w, e, &MODULE);
    } else {
        wake(w, e, &MODULE);
    }
}

#[cfg(test)]
mod tests {
    use super::super::armadillo_state::{ROLLING, SCARED};
    use super::super::baby_follow_adult;
    use super::*;
    use crate::cow::{ActiveUntil, Herd, Pos, SpeciesTable, spawn_species};
    use crate::species::Config;
    use horizon_core::geometry::Vec3;

    /// Un animal qui tient ce but (priorité 2) et un bébé qui suit un adulte (priorité 4) : le bébé ne doit tourner
    /// que quand le but ne tient pas les drapeaux.
    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([
            Entry::new(&MODULE, 2, &Params::DEFAULT),
            Entry::new(&baby_follow_adult::MODULE, 4, &baby_follow_adult::Params::DEFAULT),
        ]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity, m: &'static Module) -> bool {
        h.world.get::<crate::goals::GoalSet>(e).unwrap().is_running(m)
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    /// Un bébé et un adulte à 7 blocs (distance dans la portée du suivi).
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = config();
        let baby = spawn_species(&mut h.world, &sp, 0.5, 0.5, 11);
        let adult = spawn_species(&mut h.world, &sp, 7.5, 0.5, 12);
        for e in [baby, adult] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.world.resource_mut::<crate::goals::Entered>().0.push(adult);
        h.tick();
        let y = h.world.get::<Pos>(baby).unwrap().0.y;
        h.teleport(adult, Vec3::new(7.5, y, 0.5));
        h.make_baby(baby, 3000);
        h.world.resource_mut::<crate::goals::Entered>().0.push(baby);
        Some((h, baby))
    }

    /// Le but tient les drapeaux tant que l'état n'est pas IDLE : le bébé (qui partirait à 7 blocs) attend ; quand l'état
    /// revient à IDLE, le but s'arrête et le bébé démarre (témoin positif du départ).
    #[test]
    fn a_rolled_state_holds_the_flags_of_the_walking_goals_until_idle() {
        let Some((mut h, baby)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        set_state(&mut h.world, baby, SCARED);
        ticks(&mut h, 10);
        assert!(running(&h, baby, &MODULE), "le but tient en boule");
        assert_eq!(st::of(&h.world, baby), SCARED);
        assert!(!running(&h, baby, &baby_follow_adult::MODULE), "le suivi ne démarre pas pendant la boule");
        set_state(&mut h.world, baby, ROLLING);
        ticks(&mut h, 4);
        assert!(running(&h, baby, &MODULE), "un autre état non IDLE tient aussi");
        set_state(&mut h.world, baby, IDLE);
        ticks(&mut h, 10);
        assert!(!running(&h, baby, &MODULE), "IDLE arrête le but");
        assert!(running(&h, baby, &baby_follow_adult::MODULE), "le suivi démarre quand les drapeaux sont libres");
    }
}
