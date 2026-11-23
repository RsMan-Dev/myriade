//! The pickups of the players through the bridge (lot 71): the stacks and the orbs in a player's box, the throwers, the end of life of the
//! stacks and orbs that the host may extend. The box is the player's standing box at `feet` (0.6 wide, 1.8 tall: `Avatar`) grown by
//! `reach` sideways and 0.5 up and down, as `Player.aiStep` grows its box (1.0 and 0.5 there).
//! A stack is taken as `ItemEntity.playerTouch` takes it: only by its target (or by anyone without one) and what the host's `room`
//! accepts; the rest stays on the ground.
use crate::bridge::host::EcsWorld;
use crate::cow::Pos;
use crate::item::{self, Item};
use crate::orb::{self, Orb, OrbIndex};
use crate::targeting::Who;
use bevy_ecs::{entity::Entity, prelude::*};
use myriade_engine_lib::entities::ExperienceOrb;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};

/// The box of a player standing at `feet`, grown by `reach` sideways and 0.5 up and down.
fn player_area(feet: [f64; 3], reach: f64) -> Aabb {
    let [x, y, z] = feet;
    Aabb::new(Vec3::new(x - 0.3, y, z - 0.3), Vec3::new(x + 0.3, y + 1.8, z + 0.3)).inflate(Vec3::new(reach, 0.5, reach))
}

/// The pickable stacks whose box meets `area` and whose target is `uuid` (or none): the host's `room` takes what it accepts of each,
/// the stack is removed when it is all taken. Returns (network id or 0, the stack as taken, place), in the order they joined.
fn take_items(w: &mut World, uuid: [i32; 4], area: Aabb, room: &mut dyn FnMut(&ItemStack) -> u32) -> Vec<(u32, ItemStack, [f64; 3])> {
    let mut out = Vec::new();
    for e in item::items_in(w, area) {
        out.extend(take_entity(w, uuid, e, room));
    }
    out
}

/// One stack taken by the player `uuid`, as `take_items` takes it; the rest stays under the same entity.
fn take_entity(w: &mut World, uuid: [i32; 4], e: Entity, room: &mut dyn FnMut(&ItemStack) -> u32) -> Option<(u32, ItemStack, [f64; 3])> {
    if !item::pickable(w, e) {
        return None;
    }
    let it = w.get::<Item>(e)?;
    if it.target.is_some_and(|t| t != uuid) {
        return None;
    }
    let (count, mut taken) = (it.stack.count, it.stack.clone());
    let n = room(&taken).min(count);
    if n == 0 {
        return None;
    }
    taken.count = n;
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let id = crate::bridge::ids::id_of(w, e).unwrap_or(0);
    if n == count {
        w.entity_mut(e).despawn();
    } else if let Some(mut it) = w.get_mut::<Item>(e) {
        it.stack.count = count - n;
    }
    Some((id, taken, at))
}

impl EcsWorld {
    /// The stacks in the player's box at `feet`, taken as the host's `room` accepts (`room` gives the count it takes of a stack): (network
    /// id, the stack taken, place). Only a player picks up (`who`), with its UUID `uuid` for the stacks that have a target.
    pub fn pickup_items(
        &mut self,
        mw: &MobWorld<'_>,
        who: Who,
        uuid: [i32; 4],
        feet: [f64; 3],
        reach: f64,
        room: &mut dyn FnMut(&ItemStack) -> u32,
    ) -> Vec<(u32, ItemStack, [f64; 3])> {
        if !matches!(who, Who::Player(_)) {
            return Vec::new();
        }
        self.lent(mw, |w| take_items(w, uuid, player_area(feet, reach), room))
    }

    /// The network ids and stacks of the stacks in the player's box at `feet` that it may take now, without taking them.
    pub fn pickup_candidates(&mut self, mw: &MobWorld<'_>, who: Who, uuid: [i32; 4], feet: [f64; 3], reach: f64) -> Vec<(u32, ItemStack)> {
        if !matches!(who, Who::Player(_)) {
            return Vec::new();
        }
        self.lent(mw, |w| {
            item::items_in(w, player_area(feet, reach))
                .into_iter()
                .filter(|&e| item::pickable(w, e))
                .filter_map(|e| {
                    let it = w.get::<Item>(e)?;
                    (it.target.is_none_or(|t| t == uuid)).then(|| (crate::bridge::ids::id_of(w, e).unwrap_or(0), it.stack.clone()))
                })
                .collect()
        })
    }

    /// The stack `id` in the player's box at `feet`, taken as `room` accepts: what it took and its place, `None` when the box does not hold it
    /// or the player may not take it. A stack not taken whole stays under `id`.
    pub fn pickup_item(
        &mut self,
        mw: &MobWorld<'_>,
        who: Who,
        uuid: [i32; 4],
        id: u32,
        feet: [f64; 3],
        reach: f64,
        room: &mut dyn FnMut(&ItemStack) -> u32,
    ) -> Option<(ItemStack, [f64; 3])> {
        if !matches!(who, Who::Player(_)) {
            return None;
        }
        let e = self.entity_of(id)?;
        self.lent(mw, |w| {
            if !item::items_in(w, player_area(feet, reach)).contains(&e) {
                return None;
            }
            take_entity(w, uuid, e, room).map(|(_, stack, at)| (stack, at))
        })
    }

    /// The orbs in the player's box at `feet`, in the order of their ids, each with its network id (bound now with `alloc` when it has none).
    /// They stay until `collect_orb`.
    pub fn pickup_orbs(&mut self, mw: &MobWorld<'_>, feet: [f64; 3], reach: f64, alloc: &mut dyn FnMut() -> u32) -> Vec<(u32, Entity)> {
        let found: Vec<Entity> = self.lent(mw, |w| {
            let area = player_area(feet, reach);
            let mut v: Vec<(u32, Entity)> = w
                .resource::<OrbIndex>()
                .near(area)
                .into_iter()
                .filter_map(|e| {
                    let (p, o) = (w.get::<Pos>(e)?, w.get::<Orb>(e)?);
                    ExperienceOrb::box_at(p.0).intersects(area).then_some((o.id, e))
                })
                .collect();
            v.sort_unstable();
            v.dedup();
            v.into_iter().map(|x| x.1).collect()
        });
        found.into_iter().map(|e| (self.id_for(e, alloc), e)).collect()
    }

    /// A player collects one of the orb: its value, or `None` for an orb that does not exist.
    pub fn collect_orb(&mut self, mw: &MobWorld<'_>, orb: Entity) -> Option<u32> {
        self.lent(mw, |w| orb::collect(w, orb))
    }

    /// Who threw the stack `e` (its NBT key `Thrower`); nothing for an entity that is not a stack.
    pub fn set_thrower(&mut self, e: Entity, who: [i32; 4]) {
        item::set_thrower(self.world_mut(), e, who);
    }

    /// Pushes the end of life of the stack or the orb `e` back by `ticks`.
    pub fn extend_item_life(&mut self, e: Entity, ticks: u32) {
        item::extend_life(self.world_mut(), e, ticks);
    }

    /// Whether the host listens to the end of life of the stacks (`on`): they are then held and reported, not removed.
    pub fn set_item_expiry_hooked(&mut self, on: bool) {
        self.world_mut().resource_mut::<item::ExpiryPolicy>().hooked = on;
    }

    /// Whether the host listens to the end of life of the orbs (`on`): they are then held and reported, not removed.
    pub fn set_orb_expiry_hooked(&mut self, on: bool) {
        self.world_mut().resource_mut::<item::ExpiryPolicy>().orbs = on;
    }

    /// The host's answer to a held end of life: `None` removes it, `Some(t)` extends the life by `t` ticks.
    pub fn resolve_item_expiry(&mut self, e: Entity, extend: Option<u32>) {
        item::resolve_expiry(self.world_mut(), e, extend);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = crate::cow::Herd::new(0, 1.0, true, false, 1).world;
        item::insert_resources(&mut w);
        w
    }

    #[test]
    fn a_stack_taken_by_its_id_keeps_the_rest_under_that_id() {
        let mut w = world();
        let e = item::drop_item(&mut w, ItemStack::new("minecraft:apple", 8), Vec3::new(1.5, 64.0, 1.5), Vec3::new(0.0, 0.0, 0.0), 0);
        let mut room = |s: &ItemStack| {
            assert_eq!(s.count, 8, "the room is asked for the whole stack");
            5
        };
        let (taken, _) = take_entity(&mut w, [0; 4], e, &mut room).map(|(_, s, at)| (s, at)).expect("taken");
        assert_eq!(taken.count, 5);
        assert_eq!(w.get::<Item>(e).map(|it| it.stack.count), Some(3), "the rest stays under the entity");
    }
}
