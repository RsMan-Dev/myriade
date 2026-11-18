//! The pass as systems: `bevy_ecs` systems (queries, commands) run in an explicit order, one thread.
//! The cows are classified into lanes, then the steps of the tick are systems over the FREE lane (a cow no
//! other cow's turn can read or write: it commutes with every other turn), and the COUPLED cows take the
//! ordered pass of `cowworld.rs`. The reasoning, step by step, is in `suivi/ecs-prototype.md` section 10.
//!
//! The pass borrows what a `'static` resource cannot hold (the block world, the level's random, the
//! snapshot): it is the INPUT of each system (`PassIn`), so no schedule can run them; `run_pass` calls them in
//! order (`run_system_cached_with`, deferred commands applied after each).
use super::{
    components::*,
    event::CowEvent,
    view::Cow,
};
use crate::{
    health::DamageLevel,
    mobs::{MobWorld, PendingPushes, Seen, seen_grid::SeenGrid},
};
use bevy_ecs::{prelude::*, system::SystemInput};
use horizon_core::geometry::Vec3;

/// A cow's progress through the steps of one tick (between systems).
#[derive(Component, Default)]
pub struct Turn {
    pub gone: bool,
    pub was_on_fire: bool,
}
/// The cow's turn reads nothing of another cow and writes nothing another cow reads (this pass).
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Free;

/// What the pass borrows.
pub struct Ctx<'i> {
    pub w: &'i MobWorld<'i>,
    pub level: DamageLevel<'i>,
    pub events: &'i mut Vec<CowEvent>,
    pub snapshot: &'i mut Vec<Seen>,
    pub grid: &'i mut SeenGrid,
    pub pushes: &'i mut PendingPushes,
    pub keep: &'i mut Vec<bool>,
    pub order: &'i [Entity],
    pub dmax: f64,
    pub lanes: bool,
    /// The mutation of the test: every cow is declared free.
    pub all_free: bool,
}
/// The system input that carries it.
pub struct PassIn<'i>(pub Ctx<'i>);
impl SystemInput for PassIn<'_> {
    type Param<'i> = PassIn<'i>;
    type Inner<'i> = Ctx<'i>;
    fn wrap(this: Self::Inner<'_>) -> Self::Param<'_> {
        PassIn(this)
    }
}
/// Counters of the lanes, for the tests and the report.
#[derive(Resource, Default)]
pub struct LaneStats {
    pub free: u64,
    pub coupled: u64,
}

/// The world the free cows see: the snapshot of the pass start (a free cow reads no row), no solid cow.
fn view<'a>(w: &'a MobWorld<'_>, snapshot: &'a [Seen], grid: &'a SeenGrid) -> MobWorld<'a> {
    MobWorld { others: snapshot, others_grid: Some(grid), others_solid: Some(&[]), ..w.clone() }
}

/// The systems of a pass, in their order (the order is this function's: explicit, one thread).
#[allow(clippy::too_many_arguments)]
pub fn run_pass(
    world: &mut World,
    w: &MobWorld<'_>,
    level: &mut DamageLevel<'_>,
    events: &mut Vec<CowEvent>,
    snapshot: &mut Vec<Seen>,
    grid: &mut SeenGrid,
    pushes: &mut PendingPushes,
    keep: &mut Vec<bool>,
    order: &[Entity],
    lanes: bool,
    all_free: bool,
) {
    let dmax = snapshot.iter().fold(1.0_f64, |m, s| m.max(reach(s.velocity)));
    macro_rules! ctx {
        () => {
            Ctx {
                w,
                level: level.reborrow(),
                events: &mut *events,
                snapshot: &mut *snapshot,
                grid: &mut *grid,
                pushes: &mut *pushes,
                keep: &mut *keep,
                order,
                dmax,
                lanes,
                all_free,
            }
        };
    }
    world.run_system_cached_with::<PassIn<'static>, (), _, _>(classify, ctx!()).expect("classify");
    world.run_system_cached_with::<PassIn<'static>, (), _, _>(head, ctx!()).expect("head");
    world.run_system_cached_with::<PassIn<'static>, (), _, _>(body, ctx!()).expect("body");
    world.run_system_cached_with::<PassIn<'static>, (), _, _>(ordered, ctx!()).expect("ordered");
    world.run_system_cached_with::<PassIn<'static>, (), _, _>(tail, ctx!()).expect("tail");
}

/// How far a goal looks for another cow (`FollowParent` 9 blocks, `Breed` 8), in blocks.
const GOAL_RANGE: f64 = 10.0;

/// The reach of a cow's turn: how far it can move or be moved before the end of the pass.
fn reach(v: Vec3) -> f64 {
    v.x.abs().max(v.y.abs()).max(v.z.abs()) + 1.0
}

/// Which cows are free this pass (a conservative test, on the snapshot of the pass start).
fn classify(
    PassIn(cx): PassIn<'_>,
    mut stats: ResMut<LaneStats>,
    q: Query<(&Ident, &ExtraC, &AiC, &GoalsC, Has<Free>)>,
    mut commands: Commands,
) {
    let mut near = Vec::new();
    for (slot, &e) in cx.order.iter().enumerate() {
        let Ok((id, extra, ai, goals, was_free)) = q.get(e) else { continue };
        let mut coupled = !cx.all_free
            && (!cx.lanes
                || extra.in_love > 0
                || extra.age < 0
                || goals.holds_other()
                || ai.last_hurt_by.is_some()
                || cx.pushes.waiting_for(id.id));
        if !coupled && !cx.all_free {
            let row = &cx.snapshot[slot];
            let d = reach(row.velocity);
            let grow = |b: horizon_core::geometry::Aabb, d: f64| b.inflate(Vec3::new(d, d, d));
            let mine = grow(row.bounds, d);
            // A reader (a baby following a parent, a cow in love seeking a mate) reads the rows of the cows within
            // `GOAL_RANGE` of it: they are coupled with it whatever their own state.
            let wide = grow(row.bounds, d.max(GOAL_RANGE) + cx.dmax + 0.5);
            let all = !cx.grid.candidates(wide, &mut near);
            let hit = |j: usize| {
                if j == slot {
                    return false;
                }
                let o = &cx.snapshot[j];
                let reader = o.baby || o.in_love;
                let radius = if reader { GOAL_RANGE + reach(o.velocity) } else { reach(o.velocity) };
                grow(o.bounds, radius).intersects(mine)
            };
            coupled = !cx.all_free
                && if all { (0..cx.snapshot.len()).any(hit) } else { near.iter().any(|&j| hit(j as usize)) };
        }
        if coupled {
            stats.coupled += 1;
            if was_free {
                commands.entity(e).remove::<Free>();
            }
        } else {
            stats.free += 1;
            if !was_free {
                commands.entity(e).insert(Free);
            }
        }
    }
}

/// The start of the tick (prelude, base tick) of the free cows.
fn head(PassIn(mut cx): PassIn<'_>, mut q: Query<(Cow, &mut Turn), With<Free>>) {
    let Ctx { w, level, events, snapshot, grid, .. } = &mut cx;
    let view = view(w, snapshot, grid);
    for (mut item, mut turn) in &mut q {
        *turn = match item.tick_head(&view, level, events) {
            Some(fire) => Turn { gone: false, was_on_fire: fire },
            None => Turn { gone: true, was_on_fire: false },
        };
    }
}
/// The AI step (goals, navigation, controls, travel, pushes, age, love) of the free cows.
fn body(PassIn(mut cx): PassIn<'_>, mut q: Query<(Cow, &Turn), With<Free>>) {
    let Ctx { w, level, events, snapshot, grid, .. } = &mut cx;
    let view = view(w, snapshot, grid);
    for (mut item, turn) in &mut q {
        if !turn.gone {
            item.ai_step(&view, level, events);
        }
    }
}

/// The turns in the order of the list: a coupled cow's whole tick, a free cow's effects of the blocks (the
/// only step that draws from the level's random: it stays at its cow's turn).
fn ordered(PassIn(mut cx): PassIn<'_>, mut q: Query<(Cow, &Turn, Has<Free>)>) {
    for (slot, &e) in cx.order.iter().enumerate() {
        let Ok((mut item, turn, free)) = q.get_mut(e) else { continue };
        if free {
            if turn.gone {
                cx.keep[slot] = false;
            } else {
                let view = MobWorld { others: &[], others_grid: None, others_solid: Some(&[]), ..cx.w.clone() };
                item.effects_from_blocks(&view, &mut cx.level, cx.events);
            }
        } else {
            super::cowworld::visit(
                &mut item,
                slot,
                cx.pushes,
                cx.snapshot,
                cx.grid,
                cx.w,
                &mut cx.level,
                cx.events,
                cx.keep,
            );
        }
    }
}

/// The end of the tick of the free cows (freezing, rain, body, animation, flowers).
fn tail(PassIn(mut cx): PassIn<'_>, mut q: Query<(Cow, &Turn), With<Free>>) {
    let Ctx { w, level, snapshot, grid, .. } = &mut cx;
    let view = view(w, snapshot, grid);
    for (mut item, turn) in &mut q {
        if !turn.gone {
            item.tick_tail(&view, level, turn.was_on_fire);
        }
    }
}
