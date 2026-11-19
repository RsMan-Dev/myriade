//! The tamable capability (`Cap::Tamable`): an entity that can have an owner. The state is one component, `Owner` (who, a player
//! or a creature); sitting is the frame's occupation `OCC_SITTING` (`goals::occupy`). Taming and the order to sit or stand are an
//! interaction, a RULE of the species (`TAME`, with its parameters: the item, the chance, the health of a tame one). What a pet
//! does with it are modules (`sit_when_ordered`, `follow_owner`, `owner_target`), the conditions of an entry are the named
//! predicates `horizon:is_tame` and `horizon:is_sitting`. Nothing here is about one species.
use crate::behaviours::Interaction;
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};
use std::collections::HashMap;

/// Whose it is (`TamableAnimal.getOwner`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Owner(pub Option<Who>);
/// The pets of each player, so that what happens to a player wakes its pets without a scan.
#[derive(Resource, Default)]
pub struct Pets(pub HashMap<u32, Vec<Entity>>);

/// The parameters of the taming rule (`Wolf.mobInteract`: a bone, one chance in three).
#[derive(Clone, Copy, Debug)]
pub struct TameParams {
    pub items: &'static [&'static str],
    /// One chance in this many (`random.nextInt(chance) == 0`).
    pub chance: i32,
    /// `TamableAnimal.applyTamingSideEffects`: the health it has once tame.
    pub health: f32,
}
impl TameParams {
    pub const WOLF: Self = Self { items: &["minecraft:bone"], chance: 3, health: 40.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct TameRule(pub TameParams);
pub static TAME: Rule = Rule { name: "tame", equip, attrs: no_attrs, interact };

pub fn insert_resources(w: &mut World) {
    w.insert_resource(Pets::default());
}
/// Only a world with a species that can have an owner has the system that answers the owner's combat record.
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    if crate::species::registry().species.iter().any(|r| r.caps.contains(&crate::behaviours::Cap::Tamable)) {
        s.add_systems(owner_events.after(apply_damage));
    }
}
fn equip(w: &mut World, e: Entity, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<TameParams>(p);
    w.entity_mut(e).insert(TameRule(p));
}
/// A player used an item on it: a tame one of this player sits or stands; a wild one is tamed by the item with one chance in
/// `chance` (the item is used up whatever the draw).
fn interact(w: &mut World, e: Entity, p: &(dyn std::any::Any + Sync), i: &Interaction) -> bool {
    let p = *params::<TameParams>(p);
    let Some(owner) = w.get::<Owner>(e).map(|o| o.0) else { return false };
    match owner {
        Some(Who::Player(o)) if o == i.player => {
            let sitting = is_occupied(w, e, OCC_SITTING);
            order_sit(w, e, !sitting);
            true
        }
        Some(_) => false,
        None => {
            if !p.items.iter().any(|n| item_id(n) == i.item) {
                return false;
            }
            if with_rng(w, e, |r| r.next_int(p.chance)) == 0 {
                tame(w, e, Who::Player(i.player), p.health);
            }
            true
        }
    }
}
/// `AbstractHorse.tameWithName`: the owner, nothing else (no sitting, the health stays).
pub fn set_owner(w: &mut World, e: Entity, owner: Who) {
    w.entity_mut(e).insert(Owner(Some(owner)));
    if let Who::Player(id) = owner {
        w.resource_mut::<Pets>().0.entry(id).or_default().push(e);
    }
}
/// `TamableAnimal.tame` and `Wolf.mobInteract`'s side effects: the owner, the health, sitting, no target.
pub fn tame(w: &mut World, e: Entity, owner: Who, health: f32) {
    w.entity_mut(e).insert(Owner(Some(owner)));
    if let Some(mut h) = w.get_mut::<Health>(e) {
        h.0 = health;
    }
    if let Who::Player(id) = owner {
        w.resource_mut::<Pets>().0.entry(id).or_default().push(e);
    }
    order_sit(w, e, true);
    wake(w, e, &crate::behaviours::follow_owner::MODULE);
    // `Not(is_tame)` is read at the start only: a tamed animal that is running away (from players, from llamas) stops (the `Mob` tests `!tame` each tick).
    crate::goals::end(w, e, &crate::behaviours::avoid_entity::MODULE);
    wake_block_goals(w, e);
}
/// The goals that lie or sit on a block, which refused it for lack of an owner or for sitting are asked again (`Mob`: asked every other tick).
fn wake_block_goals(w: &mut World, e: Entity) {
    use crate::behaviours::{cat_lie_on_block, cat_relax_on_owner, cat_sit_on_block};
    wake(w, e, &cat_lie_on_block::MODULE);
    wake(w, e, &cat_sit_on_block::MODULE);
    wake(w, e, &cat_relax_on_owner::MODULE);
}
/// `TamableAnimal.setOrderedToSit`: the order is the occupation `SITTING`; the module that obeys it is woken (to sit) or ended (to stand).
pub fn order_sit(w: &mut World, e: Entity, sit: bool) {
    stop_nav(w, e);
    crate::targeting::clear_target(w, e);
    if sit {
        occupy(w, e, OCC_SITTING);
        wake(w, e, &crate::behaviours::sit_when_ordered::MODULE);
    } else {
        release(w, e, OCC_SITTING);
        wake(w, e, &crate::behaviours::follow_owner::MODULE);
        wake_block_goals(w, e);
        if let Some(slot) = slot_of(w, e, &crate::behaviours::sit_when_ordered::MODULE) {
            w.resource_mut::<Requests>().0.push(Req::End(e, slot));
        }
    }
}
/// The player that owns it and where it stands.
pub fn owner_player(w: &World, e: Entity) -> Option<Player> {
    match w.get::<Owner>(e)?.0? {
        Who::Player(id) => w.resource::<Online>().get(id),
        Who::Entity(_) => None,
    }
}
pub fn owner_distance_sqr(w: &World, e: Entity) -> Option<f64> {
    let p = owner_player(w, e)?;
    Some(d2(w.get::<Pos>(e)?.0, Vec3::new(p.x, p.y, p.z)))
}
/// `TamableAnimal.unableToMoveToOwner`: ordered to sit (or on a lead, riding: not in the frame).
pub fn unable_to_move_to_owner(w: &World, e: Entity) -> bool {
    is_occupied(w, e, OCC_SITTING)
}
/// A player's combat record changed (hurt by something, hit something): its pets' target modules are asked at the next tick.
fn owner_events(mut ps: ResMut<crate::hostile::PlayerState>, pets: Res<Pets>, goals: Query<&crate::behaviours::owner_target::OwnerTargets>, mut reqs: ResMut<Requests>) {
    if ps.combat_dirty.is_empty() {
        return;
    }
    for id in std::mem::take(&mut ps.combat_dirty) {
        for &pet in pets.0.get(&id).into_iter().flatten() {
            if let Ok(t) = goals.get(pet) {
                for s in &t.0 {
                    reqs.0.push(Req::Start(pet, s.slot));
                }
            }
        }
    }
}
/// `TamableAnimal.tryToTeleportToOwner`: ten tries around the owner, the first block it can stand on.
pub fn teleport_to_owner(w: &mut World, e: Entity) {
    let Some(at) = owner_player(w, e) else { return };
    let base = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
    for _ in 0..10 {
        let (xd, zd) = with_rng(w, e, |r| (r.next_int(7) - 3, r.next_int(7) - 3));
        if xd.abs() < 2 && zd.abs() < 2 {
            continue;
        }
        let yd = with_rng(w, e, |r| r.next_int(3) - 1);
        let p = [base[0] + xd, base[1] + yd, base[2] + zd];
        if can_teleport_to(w, e, p) {
            teleport(w, e, Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5));
            stop_nav(w, e);
            return;
        }
    }
}
/// `TamableAnimal.canTeleportTo`: a block to stand on that is not leaves, no fluid in the way, nothing in the box.
fn can_teleport_to(w: &World, e: Entity, p: [i32; 3]) -> bool {
    let mw = w.resource::<Level>().mw.0;
    let below = mw.facts.get(mw.world.block(p[0], p[1] - 1, p[2]));
    if !below.full_collision || below.name.ends_with("_leaves") {
        return false;
    }
    if mw.facts.get(mw.world.block(p[0], p[1], p[2])).fluid != horizon_client_lib::blocks::Fluid::None {
        return false;
    }
    let Some(b) = w.get::<Body>(e) else { return false };
    let half = f64::from(b.width) / 2.0;
    let (x, y, z) = (f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5);
    mw.no_collision(Aabb::new(Vec3::new(x - half, y, z - half), Vec3::new(x + half, y + f64::from(b.height), z + half)))
}
