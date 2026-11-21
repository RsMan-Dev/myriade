//! `FollowParentGoal`: a baby 3 blocks or more from the nearest adult of its species walks to it. Woken by events: the
//! adult it watches moves (`Mother`), or the baby or an adult near it changes column of the spatial index.
use super::*;
use crate::rules::follow_parent::{apart, beyond_nine, follows, in_watch_box, near_adult, next_repath};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.1 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct FollowParent {
    pub speed: f64,
    pub recalc: u64,
    pub slot: u8,
    /// What `can_use` chose for `start`.
    parent: Option<Entity>,
}
/// The adult a baby was born to: it wakes the baby's follow-parent goal when it moves.
#[derive(Component, Clone, Debug)]
pub struct Mother(pub Vec<Entity>);
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Trailing {
    pub parent: Entity,
}
/// The babies of the herd.
#[derive(Resource, Default)]
pub struct Babies(pub Vec<Entity>);

pub static MODULE: Module = Module {
    name: "follow_parent",
    flags: 0,
    resources,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.insert_resource(Babies::default());
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(FollowParent { speed: p.speed, recalc: 0, slot: inst.slot, parent: None });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((mother_moved, cell_moves, follow_parent_run).chain().in_set(slot));
}
/// `parent` is watched for `baby`: its moves wake the baby's follow-parent goal (an event on the adult that was
/// the nearest when the baby last looked, not a scan of the herd).
fn watch(world: &mut World, parent: Entity, baby: Entity) {
    match world.get_mut::<Mother>(parent) {
        Some(mut m) => {
            if !m.0.contains(&baby) {
                m.0.push(baby);
            }
        }
        None => {
            world.entity_mut(parent).insert(Mother(vec![baby]));
        }
    }
}
/// Makes `parent` the mother of `baby` and wakes the baby.
pub fn adopt(world: &mut World, parent: Entity, baby: Entity) {
    watch(world, parent, baby);
    wake(world, baby, &MODULE);
}
/// A baby of `ticks` ticks of age left.
pub fn make_baby(world: &mut World, e: Entity, ticks: u64) {
    let now = crate::cow::now_of(world, e);
    let sizes = *world.get::<Sizes>(e).unwrap();
    let p = world.get::<Pos>(e).unwrap().0;
    let mut b = world.get_mut::<Body>(e).unwrap();
    b.width = sizes.baby.0;
    b.height = sizes.baby.1;
    world.get_mut::<Bounds>(e).unwrap().0 = box_at(p, sizes.baby.0, sizes.baby.1);
    // `Mob.setBaby` of a creature that does not age (`Zombie`): the baby form stays, nothing grows it up.
    let ages = world.get::<crate::cow::SpeciesId>(e).is_none_or(|s| world.resource::<crate::cow::SpeciesTable>().0[s.0 as usize].tags & crate::cow::tag::AGEING != 0);
    if !ages {
        world.entity_mut(e).insert(GrowAt(u64::MAX));
        wake(world, e, &MODULE);
        return;
    }
    world.entity_mut(e).insert(GrowAt(now + ticks));
    wake(world, e, &MODULE);
    crate::cow::arm_at(world, e, now + ticks, Due::Grow);
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    if w.get::<GrowAt>(e).is_none() {
        return false;
    }
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, 9.0, |c| near.push(c));
    let mut best: Option<(Entity, f64)> = None;
    for c in near {
        let Some(cp) = w.get::<Pos>(c) else { continue };
        let (dx, dy, dz) = ((cp.0.x - pos.x).abs(), (cp.0.y - pos.y).abs(), (cp.0.z - pos.z).abs());
        if c != e && w.get::<SpeciesId>(c) == w.get::<SpeciesId>(e) && w.get::<GrowAt>(c).is_none() && near_adult(dx, dy, dz) {
            let d = d2(pos, cp.0);
            if best.is_none_or(|b| d <= b.1) {
                best = Some((c, d));
            }
        }
    }
    // The adult found, near or far, is watched: it moving wakes the baby again.
    if let Some(b) = best {
        watch(w, b.0, e);
    }
    match best.filter(|b| beyond_nine(b.1)) {
        Some(b) => {
            w.get_mut::<FollowParent>(e).unwrap().parent = Some(b.0);
            true
        }
        None => false,
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let now = crate::cow::now_of(w, e);
    let mut f = w.get_mut::<FollowParent>(e).unwrap();
    f.recalc = now;
    let parent = f.parent.unwrap();
    w.entity_mut(e).insert(Trailing { parent });
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Trailing>();
}
/// A mother moved: her babies that are 3 blocks or more away may follow her.
fn mother_moved(
    mut mothers: Query<(Entity, &Pos, &mut Mother), Changed<Bounds>>,
    babies: Query<(&Pos, Has<GrowAt>, &FollowParent)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for (e, pos, mut m) in &mut mothers {
        m.0.retain(|&b| babies.get(b).is_ok_and(|(_, baby, _)| baby));
        for &b in &m.0 {
            let (bp, _, f) = babies.get(b).unwrap();
            if apart(bp.0.x - pos.0.x, bp.0.z - pos.0.z) {
                reqs.0.push(Req::Start(b, f.slot));
            }
        }
        if m.0.is_empty() {
            cmd.entity(e).remove::<Mother>();
        }
    }
}
/// `FollowParentGoal.canUse` looks for the nearest adult of the species within 8 x 4 x 8: a baby is woken when an
/// adult comes within 9 blocks of it (an adult that changed column), or when it changes column itself. The babies are the
/// watchers (`GrowAt`) of the index: a mover that enters a column far from every baby costs nothing, and the others look up
/// the babies around them (not every baby against every mover).
fn cell_moves(
    moves: Res<CellMoves>,
    idx: Res<Index>,
    pos: Query<(&Pos, Has<GrowAt>)>,
    follow: Query<&FollowParent, With<GrowAt>>,
    mut reqs: ResMut<Requests>,
    mut asked: Local<std::collections::HashSet<Entity>>,
) {
    asked.clear();
    for &m in &moves.0 {
        if let Ok(f) = follow.get(m) {
            reqs.0.push(Req::Start(m, f.slot));
        }
        let Ok((mp, _)) = pos.get(m) else { continue };
        if !idx.watched_near(WATCH_FOLLOW_PARENT, idx.cell_of(mp.0.x, mp.0.z)) {
            continue;
        }
        let at = mp.0;
        idx.for_radius(at.x, at.z, 9.0, |b| {
            if b == m {
                return;
            }
            let (Ok(f), Ok((bp, _))) = (follow.get(b), pos.get(b)) else { return };
            // The goal's `can_use` draws nothing: one ask a tick per baby answers all the movers around it.
            if in_watch_box(at.x - bp.0.x, at.z - bp.0.z) && asked.insert(b) {
                reqs.0.push(Req::Start(b, f.slot));
            }
        });
    }
}
/// `FollowParentGoal.canContinue` and `tick`: the path to the parent is made again every 10 ticks.
fn follow_parent_run(
    clock: Res<Clock>,
    mut q: Query<(Entity, &Pos, &mut FollowParent, &Trailing, Has<GrowAt>)>,
    parents: Query<(&Pos, Has<GrowAt>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for (e, pos, mut f, t, baby) in &mut q {
        let parent = parents.get(t.parent).ok().filter(|(pp, pb)| !pb && follows(d2(pos.0, pp.0)));
        let (Some((pp, _)), true) = (parent, baby) else {
            reqs.0.push(Req::End(e, f.slot));
            continue;
        };
        if clock.of(e) >= f.recalc {
            f.recalc = next_repath(clock.of(e));
            go_to(&mut cmd, e, pp.0.x, pp.0.z, f.speed, 4);
        }
    }
}
