//! `SpellcasterCastingSpellGoal` and the state it reads: while a spell is being cast the caster stands still and looks at its target.
//! The cast is the component [`Casting`] (the spell id and the ticks left: `spellCastingTickCount` and `getCurrentSpell`), put by the
//! spells (`spell_use`), counted down by this module's tick (`customServerAiStep`) and removed when it reaches 0 (the goal then ends).
//! It knows no spell and no species. Not done: the look at a sheep (the wololo spell is not ported).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::targeting::{self, Target};

/// The caster is casting spell `spell` (1 summon, 2 fangs, 3 wololo, 4 disappear, 5 blindness: the clients' colours) for `ticks` more ticks.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Casting {
    pub ticks: i32,
    pub spell: u8,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Caster {
    slot: u8,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Standing;

pub static MODULE: Module = Module {
    name: "spell_casting",
    flags: MOVE | LOOK,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Goals, Cap::Head, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Caster { slot: inst.slot });
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Casting>(e).is_some_and(|c| c.ticks > 0)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
    w.entity_mut(e).insert(Standing);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Standing>();
}
/// The count goes down for every caster (a goal that does not run counts too), and the look follows the target while it stands.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, casting_run);
}
#[allow(clippy::type_complexity)]
fn casting_run(
    tg: targeting::Targets,
    mut q: Query<(Aim, &Caster, &mut Casting, Option<&Target>, Has<Standing>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, _, mut look, looking, _, _), c, mut casting, target, standing) in &mut q {
        casting.ticks -= 1;
        if standing && let Some(i) = target.and_then(|t| tg.info(t.who)) {
            aim(&mut cmd, e, &mut look, looking, [i.pos.x, i.eye, i.pos.z], None);
        }
        if casting.ticks <= 0 {
            cmd.entity(e).remove::<Casting>();
            reqs.0.push(Req::End(e, c.slot));
        }
    }
}
