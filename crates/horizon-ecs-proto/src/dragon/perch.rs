//! Le perchoir : les phases posées (Flaming, Scanning, Attacking), le dommage encaissé assis (`SittingDamage`), le compteur de
//! flammes (`FlameCount`) et le temps propre d'une phase posée (`PerchTime`). Elles n'ont pas d'état rare : tout tient en composants.
use super::breath;
use super::data;
use super::fight;
use super::phases::{self, Next, PhaseDef, PhaseId, PhaseState};
use crate::cow::Pos;
use crate::rules::dragon_perch as rule;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SittingDamage(pub f32);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PerchTime(pub i32);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FlameCount(pub i32);

/// Le dommage encaissé posé s'accumule ; passé `data::PERCH_LIMIT`, le dragon décolle et le compte repart de zéro.
pub fn sitting_hurt(w: &mut World, e: Entity, lost: f32) {
    let Some(id) = w.get::<PhaseId>(e).map(|p| p.0) else { return };
    if !data::is_sitting(id) {
        return;
    }
    let total = w.get::<SittingDamage>(e).map_or(0.0, |s| s.0) + lost;
    if rule::takes_off(total) {
        w.entity_mut(e).insert(SittingDamage(0.0));
        phases::set_phase(w, e, data::TAKEOFF);
    } else {
        w.entity_mut(e).insert(SittingDamage(total));
    }
}

/// Le dragon s'est posé : le compte des flammes repart de zéro.
pub fn landed(w: &mut World, e: Entity) {
    w.entity_mut(e).insert(FlameCount(0));
}

fn time(w: &World, e: Entity) -> i32 {
    w.get::<PerchTime>(e).map_or(0, |t| t.0)
}

fn set_time(w: &mut World, e: Entity, t: i32) {
    w.entity_mut(e).insert(PerchTime(t));
}

/// Le départ de la flamme : 2,5 blocs devant la tête, selon l'horizontale de la tête vers le dragon.
fn flame_origin(w: &World, e: Entity) -> [f64; 3] {
    let here = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let head = fight::head(w, e);
    let look = phases::unit2(head.x - here[0], head.z - here[2]);
    breath::flame_origin([head.x, head.y, head.z], [look.0, look.1], |x, y, z| fight::is_air(w, x, y, z))
}

pub struct Flaming;
impl PhaseDef for Flaming {
    fn rare(&self) -> bool {
        false
    }
    fn begin(&self, w: &mut World, e: Entity) {
        set_time(w, e, 0);
        let n = w.get::<FlameCount>(e).map_or(0, |c| c.0) + 1;
        w.entity_mut(e).insert(FlameCount(n));
    }
    fn end(&self, w: &mut World, _: Entity) {
        breath::end_flames(w);
    }
    /// Au dixième tick, la flamme part ; au deux-centième, le dragon reprend le perchoir ou décolle après quatre flammes.
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let t = time(w, e) + 1;
        set_time(w, e, t);
        if rule::flame_ends(t) {
            let to = rule::flame_next(w.get::<FlameCount>(e).map_or(0, |c| c.0));
            return Some((to, PhaseState::default()));
        }
        if rule::flame_fires(t) {
            let at = flame_origin(w, e);
            breath::spawn_flame(w, at);
        }
        None
    }
}

pub struct Scanning;
impl PhaseDef for Scanning {
    fn rare(&self) -> bool {
        false
    }
    fn begin(&self, w: &mut World, e: Entity) {
        set_time(w, e, 0);
    }
    /// Chaque tick : un joueur à moins de 20 blocs (à 10 près en hauteur) tourne la tête vers lui, et ouvre l'attaque après 25
    /// ticks ; sans lui, après 100 ticks, la charge vise le joueur à moins de 150 blocs, sinon le décollage.
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let t = time(w, e) + 1;
        set_time(w, e, t);
        let origin = w.get::<Pos>(e).map_or(Vec3::new(0.0, 0.0, 0.0), |p| p.0);
        if let Some((_, pp)) = fight::nearest_enemy(w, origin, Some(20.0), Some(10.0)) {
            if rule::attack_starts(t) {
                return Some((data::SITTING_ATTACKING, PhaseState::default()));
            }
            fight::turn_head(w, e, pp);
            return None;
        }
        if !rule::charge_starts(t) {
            return None;
        }
        Some(match fight::nearest_enemy(w, origin, Some(150.0), None) {
            Some((_, p)) => (phases::CHARGING, PhaseState { target: Some(p), ..Default::default() }),
            None => (data::TAKEOFF, PhaseState::default()),
        })
    }
}

pub struct Attacking;
impl PhaseDef for Attacking {
    fn rare(&self) -> bool {
        false
    }
    /// Quarante et un ticks d'attaque (`ticks > 40`), puis les flammes.
    fn begin(&self, w: &mut World, e: Entity) {
        set_time(w, e, 0);
    }
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let t = time(w, e) + 1;
        set_time(w, e, t);
        rule::attack_ends(t).then(|| (data::SITTING_FLAMING, PhaseState::default()))
    }
}
