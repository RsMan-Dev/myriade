//! Damage at contact (`AbstractCubeMob.playerTouch`, `push(Entity)`, `dealDamage`, `canDealDamage`, jar 26.3): a body that carries
//! `ContactDamage` hurts the player whose box it touches, and the creature that carries `ContactVictim` (the iron golem) when their
//! boxes overlap (the push pass of `cow/motion.rs` calls `touch_victim`). Params: the smallest size that bites (`isTiny` cuts the
//! slime, not the magma cube) and the damage added to the attack damage (`MagmaCube.getAttackDamage`: +2).
//! `dealDamage`: alive, teams allow it, the target is within melee reach (`isWithinMeleeAttackRange`: the box grown by the default
//! reach on x and z), in line of sight; then `hurtServer` of the attack damage; a hurt target gets the attack sound and `hostile::landed`.
//! Cost: no species name, no system, no tick. The player touches (jar `Player.aiStep`): `player_moved` runs when the bridge sets a player
//! that moved, one index query whose window follows the widest carrier box (`Carriers`, nothing at all without a carrier); a carrier that
//! moves onto a player standing still asks `Online` from `push`. The push hook asks two sparse markers only for a pair of overlapping
//! boxes. `doPostAttackEffects` goes through `hostile::landed` (`melee_gear::blow`), as any melee blow; the cube holds no weapon.
use super::*;
use bevy_ecs::{lifecycle::HookContext, world::DeferredWorld};
use crate::bridge::teams::allied;
use crate::cow::*;
use crate::goals::*;
use crate::hostile::{landed, PlayerState};
use crate::rules::hostile::melee_reach;
use crate::size::Size;
use crate::targeting::{self, Who};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `canDealDamage`: the size under which the body does not bite (`isTiny`: 1 is tiny, so 2 for a slime, 1 for a magma cube).
    pub min_size: i32,
    /// Added to the attack damage (`MagmaCube.getAttackDamage`).
    pub bonus: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { min_size: 2, bonus: 0.0 };
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = carrier_added, on_remove = carrier_removed)]
pub struct ContactDamage {
    min_size: i32,
    bonus: f32,
}
/// A creature a body with `ContactDamage` hurts when their boxes overlap (`push` of the cube tests `instanceof IronGolem`).
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct ContactVictim;
pub static VICTIM: crate::rules::Rule = crate::rules::Rule { name: "contact_victim", equip: equip_victim, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_victim(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(ContactVictim);
}

/// The box `Player.aiStep` touches the entities of: the player's box grown by (1, 0.5, 1).
const PLAYER_REACH: (f64, f64) = (1.0, 0.5);

/// The carriers alive and the widest half width among their boxes (it only grows while one lives): the window of the index query, so a
/// cube of any size is found. Without a carrier nothing here runs.
#[derive(Resource, Default)]
pub struct Carriers {
    count: u32,
    half: f64,
}
fn carrier_added(mut w: DeferredWorld, _: HookContext) {
    if let Some(mut c) = w.get_resource_mut::<Carriers>() {
        c.count += 1;
    }
}
fn carrier_removed(mut w: DeferredWorld, _: HookContext) {
    if let Some(mut c) = w.get_resource_mut::<Carriers>() {
        c.count = c.count.saturating_sub(1);
        if c.count == 0 {
            c.half = 0.0;
        }
    }
}
/// The box of a carrier changed (its creation, `set_size`): the window covers it.
pub fn widen(w: &mut World, e: Entity) {
    if w.get::<ContactDamage>(e).is_none() {
        return;
    }
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return };
    let half = ((b.max_position().x - b.min_position().x).max(b.max_position().z - b.min_position().z)) / 2.0;
    let mut c = w.get_resource_or_insert_with(Carriers::default);
    c.half = c.half.max(half);
}
fn init(w: &mut World) {
    w.init_resource::<Carriers>();
}

pub static MODULE: Module = Module {
    name: "contact_damage",
    flags: 0,
    resources: init,
    equip,
    can_use: never,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.init_resource::<Carriers>();
    w.entity_mut(e).insert(ContactDamage { min_size: p.min_size, bonus: p.bonus });
    widen(w, e);
}
/// `SoundEvents.SLIME_ATTACK`, volume 1, pitch `(r1 - r2) * 0.2 + 1`: `hostile::landed` plays it after a landed blow of a carrier
/// (no `hostile::Attack` is put on a cube: the melee readers would take it for a melee attacker).
pub fn attack_sound(w: &mut World, e: Entity) {
    let pitch = with_rng(w, e, |r| 1.0 + (r.next_float() - r.next_float()) * 0.2);
    crate::behaviours::act_phase::sound(w, e, "entity.slime.attack", 1.0, pitch);
}

/// The box `Player.aiStep` touches the entities of.
pub fn touch_area(p: &crate::goals::Player) -> horizon_core::geometry::Aabb {
    targeting::player_box(p).inflate(Vec3::new(PLAYER_REACH.0, PLAYER_REACH.1, PLAYER_REACH.0))
}
/// A player came or moved (the bridge's `set_player`): `Player.aiStep` touches the carriers whose box meets its grown box. Nothing runs
/// without a carrier; the window of the index query follows the widest box present.
pub fn player_moved(w: &mut World, p: crate::goals::Player) {
    let Some(half) = w.get_resource::<Carriers>().filter(|c| c.count > 0).map(|c| c.half) else { return };
    let area = touch_area(&p);
    let mut hits: Vec<Entity> = Vec::new();
    w.resource::<Index>().for_radius(p.x, p.z, PLAYER_REACH.0 + 0.3 + half, |c| {
        if w.get::<ContactDamage>(c).is_some() && w.get::<Bounds>(c).is_some_and(|b| b.0.intersects(area)) {
            hits.push(c);
        }
    });
    for c in hits {
        deal_damage(w, c, Who::Player(p.id));
    }
}
/// A carrier moved next to a player (`push` found the boxes meet): the player touches it, as it does every tick in the jar.
pub fn touch_player(w: &mut World, cube: Entity, id: u32) {
    deal_damage(w, cube, Who::Player(id));
}
/// `AbstractCubeMob.push(Entity)` for a pair of overlapping boxes: the body hurts the creature that is a `ContactVictim`.
pub fn touch_victim(w: &mut World, cube: Entity, other: Entity) {
    if w.get::<ContactVictim>(other).is_some() {
        deal_damage(w, cube, Who::Entity(other));
    }
}

/// `canDealDamage` then `dealDamage` of `e` on `who`.
pub fn deal_damage(w: &mut World, e: Entity, who: Who) {
    let Some(cd) = w.get::<ContactDamage>(e).copied() else { return };
    if w.get::<Health>(e).is_none_or(|h| h.0 <= 0.0) || w.get::<Dying>(e).is_some() || w.get::<crate::dragon::flight::NoAi>(e).is_some() {
        return;
    }
    // `isTiny`: a body of size 1 (a body without a size counts as 1).
    if w.get::<Size>(e).map_or(1, |s| s.0) < cd.min_size {
        return;
    }
    let (Some(pos), Some(bounds), Some(eye)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Bounds>(e).map(|b| b.0), w.get::<targeting::Eye>(e).map(|x| x.0)) else { return };
    let Some(i) = targeting::info(w, who).filter(|i| !i.exempt) else { return };
    if allied(w, Who::Entity(e), who) {
        return;
    }
    let r = melee_reach();
    if !bounds.inflate(Vec3::new(r, 0.0, r)).intersects(i.bounds) {
        return;
    }
    let mw = w.resource::<Level>().mw.0;
    if !mw.line_of_sight_for(Some(pos.y), Vec3::new(pos.x, pos.y + eye, pos.z), Vec3::new(i.pos.x, i.eye, i.pos.z)) {
        return;
    }
    let amount = w.get::<AttackDamage>(e).map_or(0.0, |d| d.0) + cd.bonus;
    match who {
        Who::Player(id) => {
            let tick = w.resource::<Clock>().0;
            w.resource_mut::<PlayerState>().hurt(tick, id, amount, (pos.x, pos.z), Some(Who::Entity(e)));
            // The host gives no answer for a player: the blow is taken as landed (as the melee does).
            landed(w, e, who, amount);
        }
        // The victim's `apply_damage` tells `landed` the result.
        Who::Entity(t) => w.resource_mut::<DamageQueue>().add_by_entity(t, amount, (pos.x, pos.z), e),
    }
}
