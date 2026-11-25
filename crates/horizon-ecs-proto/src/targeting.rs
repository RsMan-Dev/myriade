//! The target of an attacker, a service of the framework: a creature or a player that an attack module (melee, leap,
//! later a ranged attack) reads without knowing which module set it. `Target` is the current target with the memory of
//! the last sight; the modules that choose a target (`nearest_target`, `hurt_by`) write it with `set_target`, the
//! systems here keep it honest (`TargetGoal.canContinueToUse`) and clear it when it stops being valid.
use crate::IFloor as _;
use crate::cow::*;
use crate::goals::*;
use crate::effects::Invisible;
use crate::bridge::teams::{allied, allied_in, Teams};
use crate::hostile::{ChangedBlocks, PlayerState};
use bevy_ecs::{prelude::*, system::SystemParam};
use horizon_core::geometry::{Aabb, Vec3};

/// Whom an attack is aimed at: a player of the bench's list, or a creature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Who {
    Player(u32),
    Entity(Entity),
}
/// The current target of an attacker: whom, the last line-of-sight reading, the unseen evaluations counted (every other
/// tick) against `memory` (`unseenMemoryTicks` / 2), and the goal (rank in the selector) that holds it (`u8::MAX`: none).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = crate::behaviours::drink_potion::thirst_hook)]
pub struct Target {
    pub who: Who,
    /// `Mob.sees(target)` as last read; re-read by `sight_eval` only when an eye changes block or a block changes on or
    /// around the segment.
    pub seen: bool,
    zblock: (i32, i32, i32),
    pblock: (i32, i32, i32),
    unseen: u16,
    pub memory: u16,
    pub slot: u8,
    /// `TargetGoal.mustSee`: a target out of sight is lost after `memory` evaluations; else it is kept while in range.
    pub must_see: bool,
    /// `TargetGoal.getFollowDistance` as a fraction of the follow range.
    pub follow_scale: f64,
}
impl Target {
    pub fn new(who: Who, memory: u16, slot: u8) -> Self {
        Self { who, seen: false, zblock: (i32::MIN, 0, 0), pblock: (i32::MIN, 0, 0), unseen: 0, memory, slot, must_see: true, follow_scale: 1.0 }
    }
    pub fn player(id: u32, memory: u16) -> Self {
        Self::new(Who::Player(id), memory, u8::MAX)
    }
}
/// The height of the eyes above the feet (the line of sight starts there): a capability component of the targeting service.
#[derive(Component, Clone, Copy, Debug)]
pub struct Eye(pub f64);
/// How far the animal follows a target (`Attributes.FOLLOW_RANGE`): a capability component of the targeting service.
#[derive(Component, Clone, Copy, Debug)]
pub struct FollowRange(pub f64);
/// The functions called when an attacker gets a target (an attack module arms its first check there).
#[derive(Resource, Default)]
pub struct TargetWatchers(pub Vec<fn(&mut World, Entity)>);

/// What a target is, for the rules that read it.
#[derive(Clone, Copy, Debug)]
pub struct Info {
    pub pos: Vec3,
    pub eye: f64,
    pub bounds: Aabb,
    pub baby: bool,
    /// The species id of a creature (`u8::MAX` for a player).
    pub species: u8,
    pub player: bool,
    /// A player that cannot be targeted (creative, spectator) or a creature that is dead.
    pub exempt: bool,
    /// The visibility of a creature (`creature_visibility`); 1 for a player, whose visibility is read from `PlayerState`.
    pub visibility: f64,
}
pub fn player_box(p: &Player) -> Aabb {
    Aabb::new(Vec3::new(p.x - 0.3, p.y, p.z - 0.3), Vec3::new(p.x + 0.3, p.y + 1.8, p.z + 0.3))
}
/// The reading of a target from the resources and a creature lookup.
fn read(on: &Online, ps: &PlayerState, who: Who, ent: impl Fn(Entity) -> Option<(Vec3, Aabb, u8, bool, f64, f64)>) -> Option<Info> {
    match who {
        Who::Player(id) => {
            let p = on.get(id)?;
            if !ps.alive(id) {
                return None;
            }
            Some(Info { pos: Vec3::new(p.x, p.y, p.z), eye: p.y + 1.62, bounds: player_box(&p), baby: false, species: u8::MAX, player: true, exempt: !ps.targetable(id), visibility: 1.0 })
        }
        Who::Entity(e) => {
            let (pos, bounds, sid, baby, eye, visibility) = ent(e)?;
            Some(Info { pos, eye: pos.y + eye, bounds, baby, species: sid, player: false, exempt: false, visibility })
        }
    }
}
/// The targets of a system: players, creatures, their rules.
#[derive(SystemParam)]
pub struct Targets<'w, 's> {
    pub on: Res<'w, Online>,
    pub ps: Res<'w, PlayerState>,
    ents: Query<'w, 's, (&'static Pos, &'static Bounds, &'static SpeciesId, Has<GrowAt>, &'static Eye, Has<Invisible>, Option<&'static Equipment>)>,
    crumbling: Query<'w, 's, (), crate::tear_down::Dead>,
}
impl Targets<'_, '_> {
    pub fn info(&self, who: Who) -> Option<Info> {
        let mut i = read(&self.on, &self.ps, who, |e| self.ents.get(e).ok().map(|(p, b, s, g, l, i, q)| (p.0, b.0, s.0, g, l.0, creature_visibility(i, q))))?;
        // A crumbling creature is dying: `TargetingConditions` refuse the dead.
        i.exempt |= matches!(who, Who::Entity(e) if self.crumbling.contains(e));
        Some(i)
    }
}
/// `LivingEntity.getVisibilityPercent` of a creature: invisible, it is scaled by the worn slots (at least a tenth). Crouching and
/// the items' own factors are not held by the ECS: they count 1 here.
pub fn creature_visibility(invisible: bool, eq: Option<&Equipment>) -> f64 {
    if !invisible {
        return 1.0;
    }
    let worn = eq.map_or(0, |q| [q.head, q.chest, q.legs, q.feet].iter().filter(|&&i| i != 0).count());
    // The jar computes the cover as a float, widened before the product: 0.7 x (double) 0.1f.
    0.7 * f64::from((worn as f32 / 4.0).max(0.1))
}
/// `LivingEntity.getVisibilityPercent` of the creature `c` for a looker of species `looker`: [`creature_visibility`] times the worn head's
/// `mob_visibility` for that species (a skeleton skull fools a skeleton, not a stray), clamped to 0..=10. Crouching is not held by a creature.
pub fn creature_visibility_to(w: &World, c: Entity, looker: Option<u8>) -> f64 {
    let eq = w.get::<Equipment>(c);
    let base = creature_visibility(w.get::<Invisible>(c).is_some(), eq);
    let head = eq.map_or(0, |q| q.head);
    let (true, Some(looker)) = (head != 0, looker) else { return base };
    let name = crate::species::registry().species[usize::from(looker)].name;
    let fooled = crate::species::mob_heads::MOB_HEADS.iter().any(|(item, who)| who.contains(&name) && crate::items::item_id(item) == head);
    (if fooled { base * 0.5 } else { base }).clamp(0.0, 10.0)
}
/// The game's side of a mod's `living_visibility` (`LivingVisibilityEvent`): the visibility of player `player` as the mob `looking` (its
/// network id) sees it, applied last. Lent for one step like the mod goals; without a lend nothing changes.
pub trait LivingVisibilityHost {
    fn visibility(&self, player: u32, looking: u32, visibility: f64) -> f64;
}
/// The host lent for one step, its lifetime erased: `LivingVisibility` is emptied by `step_with` before the host's borrow ends.
#[derive(Clone, Copy)]
pub struct LentVisibility(&'static dyn LivingVisibilityHost, std::thread::ThreadId);
// SAFETY: it is only read by the nearest-target search, which runs with the whole world on the thread that lent it (`get` checks it in debug).
unsafe impl Send for LentVisibility {}
unsafe impl Sync for LentVisibility {}
impl LentVisibility {
    pub fn lend(host: &dyn LivingVisibilityHost) -> LentVisibility {
        // SAFETY: the only holder is `LivingVisibility`, emptied by `step_with` before `host`'s borrow ends.
        LentVisibility(unsafe { std::mem::transmute::<&dyn LivingVisibilityHost, &'static dyn LivingVisibilityHost>(host) }, std::thread::current().id())
    }
    pub fn get(self) -> &'static dyn LivingVisibilityHost {
        debug_assert_eq!(self.1, std::thread::current().id());
        self.0
    }
}
#[derive(bevy_ecs::prelude::Resource, Default)]
pub struct LivingVisibility(pub Option<LentVisibility>);

/// `Player.getVisibilityPercent` under a worn mob head: half the visibility against the mob species `looker` it fools (`MOB_HEADS`), 1 otherwise.
pub fn head_factor(helmet: u32, looker: Option<u8>) -> f64 {
    let (true, Some(looker)) = (helmet != 0, looker) else { return 1.0 };
    let name = crate::species::registry().species[usize::from(looker)].name;
    let fooled = crate::species::mob_heads::MOB_HEADS.iter().any(|(item, who)| who.contains(&name) && crate::items::item_id(item) == helmet);
    if fooled { 0.5 } else { 1.0 }
}
/// [`Targets::info`] for the code that holds the world.
pub fn info(w: &World, who: Who) -> Option<Info> {
    let mut i = read(w.resource::<Online>(), w.resource::<PlayerState>(), who, |e| {
        Some((w.get::<Pos>(e)?.0, w.get::<Bounds>(e)?.0, w.get::<SpeciesId>(e)?.0, w.get::<GrowAt>(e).is_some(), w.get::<Eye>(e)?.0, creature_visibility(w.get::<Invisible>(e).is_some(), w.get::<Equipment>(e))))
    })?;
    i.exempt |= matches!(who, Who::Entity(e) if crate::tear_down::is_dead(w, e));
    Some(i)
}
/// `TargetingConditions`: the range an attacker sees a target at: the follow range, scaled by the target's visibility,
/// never under 2 blocks.
pub fn sight_range(range: f64, visibility: f64) -> f64 {
    (range * visibility.clamp(0.0, 10.0)).max(2.0)
}
/// The attacker takes `who` as its target (the goal `slot` holds it): the attack modules are told.
pub fn set_target(w: &mut World, e: Entity, mut t: Target) {
    t.seen = false;
    // `Mob.sees` is read where it is used: a target taken after this tick's `sight_eval` is read now, not a tick late.
    if let (Some(pos), Some(eye), Some(i)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Eye>(e).map(|x| x.0), info(w, t.who)) {
        t.zblock = (pos.x.floor() as i32, (pos.y + eye).floor() as i32, pos.z.floor() as i32);
        t.pblock = (i.pos.x.floor() as i32, i.eye.floor() as i32, i.pos.z.floor() as i32);
        let mw = w.resource::<Level>().mw.0;
        t.seen = mw.line_of_sight_for(Some(pos.y), Vec3::new(pos.x, pos.y + eye, pos.z), Vec3::new(i.pos.x, i.eye, i.pos.z));
    }
    let who = t.who;
    let previous = w.get::<Target>(e).map(|p| p.who);
    w.entity_mut(e).insert(t);
    // `StartAttacking` erases the memory of the unreachable: a new target starts afresh.
    if let Some(mut n) = w.get_mut::<crate::cow::Nav>(e) {
        n.cant_reach_since = 0;
    }
    w.resource_mut::<Events>().target_acquired.push(e);
    w.resource_mut::<Events>().host.target_changed(e, Some(who), previous);
    let watchers = w.resource::<TargetWatchers>().0.clone();
    for f in watchers {
        f(w, e);
    }
}
/// The attacker has no target any more.
pub fn clear_target(w: &mut World, e: Entity) {
    if let Some(old) = w.entity_mut(e).take::<Target>() {
        w.resource_mut::<Events>().target_lost.push(e);
        w.resource_mut::<Events>().host.target_changed(e, None, Some(old.who));
        crate::behaviours::dig::target_lost(w, e);
    }
}
/// `Mob.sees` for the attackers that have a target: the line of sight is read again only when the attacker's eyes or the
/// target's change block, or a block changes on the box of the segment between them (inflated by a block); an attacker
/// that stands still, with a still target and still blocks, costs a comparison.
pub fn sight_eval(level: Res<Level>, tg: Targets, mut changed: ResMut<ChangedBlocks>, mut q: Query<(&Pos, &Eye, &mut Target)>) {
    let mw = level.mw.0;
    let block = |p: Vec3, eye: f64| (p.x.ifloor(), (p.y + eye).ifloor(), p.z.ifloor());
    for (pos, look, mut g) in &mut q {
        let Some(i) = tg.info(g.who) else { continue };
        let (zb, pb) = (block(pos.0, look.0), (i.pos.x.ifloor(), i.eye.ifloor(), i.pos.z.ifloor()));
        let mut dirty = zb != g.zblock || pb != g.pblock;
        if !dirty && !changed.0.is_empty() {
            let (lo, hi) = ((zb.0.min(pb.0) - 1, zb.1.min(pb.1) - 1, zb.2.min(pb.2) - 1), (zb.0.max(pb.0) + 1, zb.1.max(pb.1) + 1, zb.2.max(pb.2) + 1));
            dirty = changed.0.iter().any(|c| (lo.0..=hi.0).contains(&c.0) && (lo.1..=hi.1).contains(&c.1) && (lo.2..=hi.2).contains(&c.2));
        }
        if dirty {
            (g.zblock, g.pblock) = (zb, pb);
            g.seen = mw.line_of_sight_for(Some(pos.0.y), Vec3::new(pos.0.x, pos.0.y + look.0, pos.0.z), Vec3::new(i.pos.x, i.eye, i.pos.z));
        }
    }
    changed.0.clear();
}
/// `TargetGoal.canContinueToUse`, asked every other tick of the attackers that have a target: alive, not creative, within
/// the follow range, and (with `must_see`) seen at least once in `memory` evaluations; else the target is lost.
pub fn target_continue(clock: Res<Clock>, tg: Targets, teams: Option<Res<Teams>>, mut q: Query<(Entity, &Pos, &FollowRange, &mut Target)>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    if clock.0 % 2 != 0 {
        return;
    }
    for (e, pos, range, mut g) in &mut q {
        let follow = range.0 * g.follow_scale;
        let ok = tg.info(g.who).is_some_and(|i| !i.exempt && d2(pos.0, i.pos) <= follow * follow) && !teams.as_deref().is_some_and(|t| allied_in(t, Who::Entity(e), g.who));
        if g.seen {
            g.unseen = 0;
        } else {
            g.unseen += 1;
        }
        if !ok || (g.must_see && g.unseen > g.memory) {
            if g.slot == u8::MAX {
                cmd.queue(move |w: &mut World| clear_target(w, e));
            } else {
                reqs.0.push(Req::End(e, g.slot));
            }
        }
    }
}
/// `TargetingConditions` for `e`: `who` is alive and not creative or spectator (`Info.exempt`), not `e` itself, not in its team, and not
/// invulnerable (the `Invulnerable` flag, which `Warden.canTargetEntity` refuses).
pub fn can_target(w: &World, e: Entity, who: Who) -> bool {
    who != Who::Entity(e) && info(w, who).is_some_and(|i| !i.exempt) && !allied(w, Who::Entity(e), who) && !matches!(who, Who::Entity(t) if w.get::<crate::invulnerable::Invulnerable>(t).is_some())
}
/// `Mob.canAttack` refuses this target (the ghast): no mob takes it as a target, nor answers its hit. Put at creation by `Cap::RefusedByMobs`.
#[derive(bevy_ecs::prelude::Component, Clone, Copy, Debug)]
pub struct RefusedByMobs;
/// The first creature within `radius` of `e` (by identifier, ascending) that `e` can target, read from the spatial index.
pub fn attackable_near(w: &World, e: Entity, radius: f64) -> Option<Who> {
    let pos = w.get::<Pos>(e)?.0;
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, radius, |c| near.push(c));
    near.into_iter().filter(|&c| can_target(w, e, Who::Entity(c))).min_by_key(|c| c.index().index()).map(Who::Entity)
}
/// `Mob.sees`: the line of sight from the eyes of `e` to the eyes of `who` is free of blocks.
pub fn sees(w: &World, e: Entity, who: Who) -> bool {
    let (Some(pos), Some(look), Some(i)) = (w.get::<Pos>(e), w.get::<Eye>(e), info(w, who)) else {
        return false;
    };
    w.resource::<Level>().mw.0.line_of_sight_for(Some(pos.0.y), Vec3::new(pos.0.x, pos.0.y + look.0, pos.0.z), Vec3::new(i.pos.x, i.eye, i.pos.z))
}
/// The framework's systems of the targeting service (registered by the modules that choose targets, once).
pub fn register(s: &mut bevy_ecs::schedule::Schedule, slot: GoalSlot) {
    s.add_systems((sight_eval, target_continue).chain().after(apply_damage::<false>).in_set(slot));
}
