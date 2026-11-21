//! The lead (`Leashable`): a creature held by a player, by another creature or by a fence knot. The state is one component, `Leash`
//! (the holder and the spin the rope gives the body); the parameters are `Leashable` (the elastic distance, the snapping distance, the speed
//! at which a slack lead walks the creature to its holder), laid by the rule `LEASH`. A directory, `Leashes`, answers "what does this holder
//! hold" without a scan. The tick (`leash_tick`, only the leashed ones) is `Leashable.tickLeash`: the lead snaps beyond the snapping
//! distance, pulls (a force on the body and a torque on the yaw) beyond the elastic one, the creature's home is the holder's block, and a
//! slack lead walks it to within two blocks of the holder. Events: `leashed`, `unleashed` (read once, the tick after). The goals that stop
//! walking when the creature is held read the predicate `horizon:is_leashed` (`Entry::when`). Nothing here is about one species.
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use crate::rules::leash::{center_of, follow_target, is_stretched, spin_friction, spin_step};
use horizon_core::geometry::Vec3;
use std::collections::HashMap;

/// What holds the other end of a lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Holder {
    /// A player of the bench.
    Player(u32),
    Entity(Entity),
    /// A fence knot on the block.
    Knot([i32; 3]),
}
/// The creature is on a lead.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_remove = leash_removed)]
pub struct Leash {
    pub holder: Holder,
    /// `Leashable.angularMomentum` (the yaw turns by it each tick, with a friction).
    pub spin: f64,
    /// The block the slack lead last asked a path to (a path is asked again when the holder's side moves, not every tick).
    goal: [i32; 3],
}
/// The creature accepts a lead.
#[derive(Component, Clone, Copy, Debug)]
pub struct Leashable {
    /// `Leashable.leashElasticDistance`, `leashSnapDistance`, `followLeashSpeed`.
    pub elastic: f64,
    pub snap: f64,
    pub follow_speed: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct LeashParams {
    pub elastic: f64,
    pub snap: f64,
    pub follow_speed: f64,
}
impl LeashParams {
    pub const DEFAULT: Self = Self { elastic: 6.0, snap: 12.0, follow_speed: 1.0 };
    /// `Llama.followLeashSpeed`.
    pub const LLAMA: Self = Self { follow_speed: 2.0, ..Self::DEFAULT };
    /// The ghast family snaps further out.
    pub const GHAST: Self = Self { elastic: 10.0, snap: 16.0, follow_speed: 1.0 };
}
/// Who holds what: written by `attach` and by the removal of `Leash` (a despawn included), read by the modules that serve the held.
#[derive(Resource, Default)]
pub struct Leashes {
    held: HashMap<Holder, Vec<Entity>>,
}
impl Leashes {
    pub fn held_by(&self, h: Holder) -> &[Entity] {
        self.held.get(&h).map_or(&[], Vec::as_slice)
    }
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}
fn leash_removed(mut world: DeferredWorld, ctx: HookContext) {
    let Some(h) = world.get::<Leash>(ctx.entity).map(|l| l.holder) else { return };
    if let Some(mut l) = world.get_resource_mut::<Leashes>()
        && let Some(v) = l.held.get_mut(&h)
    {
        v.retain(|&x| x != ctx.entity);
        if v.is_empty() {
            l.held.remove(&h);
        }
    }
}
pub static LEASH: Rule = Rule { name: "leash", equip, attrs: no_attrs, interact };
fn equip(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<LeashParams>(p);
    w.entity_mut(e).insert(Leashable { elastic: p.elastic, snap: p.snap, follow_speed: p.follow_speed });
}
/// A lead in hand puts the creature on a lead; the holder's empty hand takes it off.
fn interact(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    if let Some(l) = w.get::<Leash>(e) {
        return l.holder == Holder::Player(i.player) && detach(w, e).is_some();
    }
    i.item == lead() && attach(w, e, Holder::Player(i.player))
}

pub fn holder_of(w: &World, e: Entity) -> Option<Holder> {
    w.get::<Leash>(e).map(|l| l.holder)
}
pub fn is_leashed(w: &World, e: Entity) -> bool {
    w.get::<Leash>(e).is_some()
}
/// The creatures this holder holds.
pub fn held_by(w: &World, h: Holder) -> Vec<Entity> {
    w.get_resource::<Leashes>().map_or_else(Vec::new, |l| l.held_by(h).to_vec())
}
pub use crate::rules::leash::{HolderState, elastic_pull, knot_state};
fn player_state(p: Player) -> HolderState {
    // The bench keeps no box for its players: the standing one (0,6 by 1,8).
    let position = Vec3::new(p.x, p.y, p.z);
    HolderState { position, center: Vec3::new(p.x, p.y + 0.9, p.z), width: 0.6, height: 1.8, yaw: 0.0, velocity: Vec3::ZERO }
}
/// `Leashable.canHaveALeashAttachedTo` and the lead put on: false when it is not leashable, already held, or the holder is out of reach.
pub fn attach(w: &mut World, e: Entity, holder: Holder) -> bool {
    let Some(l) = w.get::<Leashable>(e).copied() else { return false };
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return false };
    let Some(state) = holder_state(w, holder) else { return false };
    if center_of(b).distance(state.center) > l.snap {
        return false;
    }
    attach_from_save(w, e, holder)
}

/// The lead put on with no reach test: `setLeashedTo` as `Leashable.restoreLeashFromSave` calls it. False when `e` is not leashable, already held, or the holder is absent.
pub fn attach_from_save(w: &mut World, e: Entity, holder: Holder) -> bool {
    if w.get::<Leashable>(e).is_none() || w.get::<Leash>(e).is_some() || w.get::<Bounds>(e).is_none() {
        return false;
    }
    if holder_state(w, holder).is_none() {
        return false;
    }
    let goal = [i32::MIN; 3];
    w.entity_mut(e).insert((Leash { holder, spin: 0.0, goal }, Moving));
    w.get_resource_or_insert_with(Leashes::default).held.entry(holder).or_default().push(e);
    w.resource_mut::<Events>().leashed.push(e);
    true
}
/// The lead comes off (`Mob.onLeashRemoved`: the home is dropped); the holder it had.
pub fn detach(w: &mut World, e: Entity) -> Option<Holder> {
    let h = w.get::<Leash>(e)?.holder;
    w.entity_mut(e).remove::<(Leash, crate::spots::Home)>();
    w.resource_mut::<Events>().unleashed.push(e);
    w.resource_mut::<Events>().host.unleashed(e);
    Some(h)
}
fn holder_state(w: &World, holder: Holder) -> Option<HolderState> {
    match holder {
        Holder::Player(id) => w.resource::<Online>().get(id).map(player_state),
        Holder::Entity(h) => entity_state(w, h),
        Holder::Knot(p) => Some(knot_state(p)),
    }
}

fn entity_state(w: &World, h: Entity) -> Option<HolderState> {
    let (p, b) = (w.get::<Pos>(h)?.0, w.get::<Bounds>(h)?.0);
    let (lo, hi) = (b.min_position(), b.max_position());
    Some(HolderState {
        position: p,
        center: center_of(b),
        width: hi.x - lo.x,
        height: hi.y - lo.y,
        yaw: w.get::<Yaw>(h).map_or(0.0, |y| y.0),
        velocity: w.get::<Vel>(h).map_or(Vec3::ZERO, |v| v.0),
    })
}
/// `PathfinderMob.closeRangeLeashBehaviour`: the slack lead walks the creature to within two blocks of the holder.
fn walk_to_holder(w: &mut World, e: Entity, target: [f64; 3], speed: f64) {
    let block = [target[0].floor() as i32, target[1].floor() as i32, target[2].floor() as i32];
    let idle = w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none();
    let Some(mut l) = w.get_mut::<Leash>(e) else { return };
    if l.goal != block || idle {
        l.goal = block;
        go_to_at(w, e, target, speed, 1);
    }
}

/// `Leashable.tickLeash`, each tick of each leashed creature.
/// Not a system of its own (a registered system costs a thousand instructions a tick even when nothing is held): `mount::ridden_drive`, which runs
/// every tick anyway before the move, calls it with its parameters; the writes to the body go through commands (the velocity, the yaw and the fall distance belong to
/// other queries of that system). A held creature that is itself ridden skips the tick's tension.
pub(crate) type Held<'a> = (Entity, &'a mut Leash, &'a Leashable, &'a Pos, &'a Vel, &'a Yaw, &'a Bounds, &'a Body, &'a Contact, &'a FluidState, Option<&'a crate::spots::Home>, Has<crate::behaviours::panic::Panicking>);
pub(crate) type HeldFilter = Without<crate::mount::Ridden>;
#[allow(clippy::type_complexity)]
pub(crate) fn tick(
    level: &Level,
    on: &Online,
    q: &mut Query<Held, HeldFilter>,
    holders: &Query<(&Pos, &Bounds, Option<&Vel>, Option<&Yaw>), (Without<Leash>, Without<crate::mount::Ridden>)>,
    commands: &mut Commands,
) {
    for (e, mut l, k, pos, vel, yaw, bounds, body, c, fs, home, panicking) in q.iter_mut() {
        let state = match l.holder {
            Holder::Player(id) => on.get(id).map(player_state),
            Holder::Knot(p) => Some(knot_state(p)),
            Holder::Entity(h) => holders.get(h).ok().map(|(p, b, v, y)| {
                let (lo, hi) = (b.0.min_position(), b.0.max_position());
                HolderState {
                    position: p.0,
                    center: center_of(b.0),
                    width: hi.x - lo.x,
                    height: hi.y - lo.y,
                    yaw: y.map_or(0.0, |y| y.0),
                    velocity: v.map_or(Vec3::ZERO, |v| v.0),
                }
            }),
        };
        let Some(state) = state else {
            // The holder is gone: the lead drops (a player out of the bench keeps it).
            if !matches!(l.holder, Holder::Player(_)) {
                commands.queue(move |w: &mut World| {
                    detach(w, e);
                });
            }
            continue;
        };
        // `PathfinderMob.whenLeashedTo`: the home is the holder's block, within the elastic distance less one.
        let center = [state.position.x.floor() as i32, state.position.y.floor() as i32, state.position.z.floor() as i32];
        let radius = k.elastic as i32 - 1;
        if home.is_none_or(|h| h.center != center || h.radius != radius) {
            commands.entity(e).insert(crate::spots::Home { center, radius });
        }
        let mine = center_of(bounds.0);
        let distance = mine.distance(state.center);
        if distance > k.snap {
            commands.queue(move |w: &mut World| {
                detach(w, e);
            });
            continue;
        }
        let stretched = is_stretched(distance, k.elastic, state.width, body.width);
        let pulled = stretched
            && elastic_pull(pos.0, yaw.0, (f64::from(body.width), f64::from(body.height)), &state, vel.0, k.elastic).is_some_and(|(force, torque)| {
                l.spin += 10.0 * torque;
                let rel = Vec3::new(state.velocity.x - vel.0.x, state.velocity.y - vel.0.y, state.velocity.z - vel.0.z);
                let dv = Vec3::new(force[0] * 0.8 + rel.x * 0.11, force[1] * 0.2 + rel.y * 0.11, force[2] * 0.8 + rel.z * 0.11);
                let low = vel.0.y + dv.y > -0.5 && c.fall_distance > 1.0;
                commands.queue(move |w: &mut World| {
                    impulse(w, e, dv);
                    if low && let Some(mut c) = w.get_mut::<Contact>(e) {
                        c.fall_distance = 1.0;
                    }
                });
                true
            });
        if !pulled && !panicking {
            let target = follow_target(pos.0, state.position);
            let speed = k.follow_speed;
            commands.queue(move |w: &mut World| walk_to_holder(w, e, target, speed));
        }
        // `tickLeash`: the yaw turns by the spin, which the ground's friction wears.
        if l.spin != 0.0 {
            let turned = (f64::from(yaw.0) - l.spin) as f32;
            commands.queue(move |w: &mut World| {
                if let Some(mut y) = w.get_mut::<Yaw>(e) {
                    y.0 = turned;
                }
            });
            let ground = c.on_ground.then(|| {
                let mw = level.mw.0;
                let below = mw.world.block(pos.0.x.floor() as i32, (pos.0.y - 0.500_001).floor() as i32, pos.0.z.floor() as i32);
                mw.facts.get(below).friction
            });
            l.spin = spin_step(l.spin, spin_friction(ground, fs.in_water));
        }
    }
}
