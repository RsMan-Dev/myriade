//! A hot sulfur cube (holding a block whose archetype has a contact damage) burns the creature it touches (`SulfurCube.mob_contacts`,
//! `applyContactDamage`, `sulfur_cube_hot`). Event-driven: the entity-push pass reports each touching pair (`Ev<Touched>`, to be produced
//! by the socle); the touched cube gets a `Touch` and its goal starts for that tick only. No scan of the entities here.
//! Vanilla checks the target is pushable and on the cube's team (`pushable_by`): the `Touched` event must already filter that.
//! DOUTE : `DamageQueue::add_by_entity` ne prend pas de type de dégât : `sulfur_cube_hot` n'est pas transmis (socle manquant).
//! DOUTE : `Touched` (paire d'entités poussées) n'existe pas dans `Events` : à ajouter par le socle au passage de la poussée d'entités.
use super::swallow_hold::Held;
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::touch::Toucher;

/// The creature the cube touched this tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Touch {
    pub target: Entity,
}

/// No parameters: the damage is the held block's archetype's.
#[derive(Clone, Copy, Debug)]
pub struct Params;
impl Params {
    pub const DEFAULT: Self = Self;
}

pub static MODULE: Module = Module {
    name: "burn_on_touch",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, _: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Toucher { reach: [1.0, 0.5, 1.0] });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(touched.in_set(slot));
}
/// A pair of entities pushed each other: a hot cube holding a block records the creature it touches and starts its goal.
fn touched(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<&Held>, mut commands: Commands) {
    for (cube, target) in ev.touched.iter() {
        let hot = q.get(cube).is_ok_and(|h| h.archetype.contact_damage.is_some());
        if hot {
            commands.entity(cube).insert(Touch { target });
            reqs.0.push(Req::Start(cube, 0));
        }
    }
}
fn arm(_: &mut World, _: Entity, _: Inst) {}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Touch>(e).is_some()
}
/// The contact of this tick: one hit of the archetype's contact damage on the touched creature (if alive).
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(touch) = w.get::<Touch>(e).copied() else { return };
    w.entity_mut(e).remove::<Touch>();
    let amount = w.get::<Held>(e).and_then(|h| h.archetype.contact_damage);
    let at = w.get::<Pos>(e).map(|p| (p.0.x, p.0.z));
    let alive = w.get::<Health>(touch.target).is_some_and(|h| h.0 > 0.0);
    if let (Some(amount), Some(at), true) = (amount, at, alive) {
        w.resource_mut::<DamageQueue>().add_by_entity(touch.target, amount, at, e);
    }
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
fn stop(_: &mut World, _: Entity, _: Inst) {}
