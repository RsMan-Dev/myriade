//! `DoorInteractGoal` as a service of the door goals (`break_door`, `raider_open_door`, `interact_with_door`): not a module, no entry of its own.
//! The door a creature meets (`can_use`: it bumped into something and a wooden door is on its path within 1.5 blocks, or at its head), whether it
//! is open, opening and closing it (a block state written through the frame), and the marker + system that wake the door goals: the wake is the
//! contact with a wall (`Contact.horizontal`) of a creature that follows a path, read only for the creatures that carry `DoorSeeker`.
use super::*;
use crate::{blocks, cow::*, goals::*};
use horizon_world::registry::BlockStateId;
use crate::rules::door_interact::{near_door_column, other_half};

/// The door of the goal: where it is, and whether one is there (`DoorInteractGoal.doorPos`, `hasDoor`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DoorAt {
    pub pos: [i32; 3],
    pub has: bool,
}
/// `DoorBlock.isWoodenDoor`: a door a hand opens, which is any but iron.
pub fn wooden_door(w: &World, p: [i32; 3]) -> bool {
    let mw = blocks::world_of(w);
    let name = mw.block_name(p[0], p[1], p[2]);
    name.ends_with("_door") && name != "minecraft:iron_door"
}
fn is_door(w: &World, p: [i32; 3]) -> bool {
    blocks::world_of(w).block_name(p[0], p[1], p[2]).ends_with("_door")
}
/// A door that mobs open (`#minecraft:mob_interactable_doors`) at `p`: whether it is open; `None` for another block.
pub fn mob_door_open(w: &World, p: [i32; 3]) -> Option<bool> {
    let mw = blocks::world_of(w);
    let name = mw.block_name(p[0], p[1], p[2]);
    (name.ends_with("_door") && mw.has_tag("minecraft:mob_interactable_doors", name)).then(|| blocks::property(mw, p, "open").as_deref() == Some("true"))
}
pub use crate::rules::door_interact::closer_to_center;
/// `DoorInteractGoal.canUse` (less the ground navigation, which is the module's requirement): the creature touched a wall, and a wooden door is
/// within 1.5 blocks (horizontally) of one of the next two nodes of its path, or at its own head. The door found is kept in `DoorAt`.
pub fn can_use(w: &mut World, e: Entity) -> bool {
    if !w.get::<Contact>(e).is_some_and(|c| c.horizontal) {
        return false;
    }
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let mut found = None;
    if let Some(path) = w.get::<Nav>(e).and_then(|n| n.path.as_ref()).filter(|p| !p.is_done()) {
        for node in path.nodes.iter().take((path.next + 2).min(path.nodes.len())) {
            let door = [node.x, node.y + 1, node.z];
            let (dx, dz) = (p.x - f64::from(node.x), p.z - f64::from(node.z));
            if near_door_column(dx, dz) && wooden_door(w, door) {
                found = Some(door);
                break;
            }
        }
    } else {
        return false;
    }
    let door = found.or_else(|| {
        let head = [p.x.floor() as i32, p.y.floor() as i32 + 1, p.z.floor() as i32];
        wooden_door(w, head).then_some(head)
    });
    w.entity_mut(e).insert(DoorAt { pos: door.unwrap_or_default(), has: door.is_some() });
    door.is_some()
}
/// `DoorInteractGoal.isOpen`: the door is still there and open.
pub fn is_open(w: &mut World, e: Entity) -> bool {
    let Some(d) = w.get::<DoorAt>(e).copied().filter(|d| d.has) else { return false };
    if !is_door(w, d.pos) {
        w.get_mut::<DoorAt>(e).unwrap().has = false;
        return false;
    }
    blocks::property(blocks::world_of(w), d.pos, "open").as_deref() == Some("true")
}
/// The state `id` of the block at `p` with `open` changed.
fn with_open(w: &World, p: [i32; 3], open: bool) -> Option<BlockStateId> {
    let mw = blocks::world_of(w);
    let mut st = mw.registry.state(blocks::state_id(mw, p))?.clone();
    st.properties.insert("open".into(), if open { "true" } else { "false" }.into());
    mw.registry.id(&st)
}
/// `DoorBlock.setOpen` at `p`: the block state is written through the frame (the same effects as any block change).
pub fn set_open_at(w: &mut World, p: [i32; 3], open: bool) {
    if let Some(id) = with_open(w, p, open) {
        blocks::set_state_id(w, p, id);
    }
    // `DoorBlock.updateShape`: the other half of the door takes the same `open` (the game's `set_door_open` writes both halves).
    let other = other_half(p, blocks::property(blocks::world_of(w), p, "half").as_deref() == Some("lower"));
    if is_door(w, other)
        && let Some(id) = with_open(w, other, open)
    {
        blocks::set_state_id(w, other, id);
    }
}
/// `DoorInteractGoal.setOpen`.
pub fn set_open(w: &mut World, e: Entity, open: bool) {
    if let Some(d) = w.get::<DoorAt>(e).copied().filter(|d| d.has && is_door(w, d.pos)) {
        set_open_at(w, d.pos, open);
    }
}
/// The marker of a door goal: the slot of its entry (one marker type per module, so a creature may have several door goals).
pub trait Seeker: Component {
    fn slot(&self) -> u8;
}
/// One marker type per door module (a creature may have several door goals): its slot, read by `seek` for the creatures that follow a path.
macro_rules! seekers {
    ($($n:ident),*) => {$(
        #[derive(Component, Clone, Copy, Debug)]
        pub struct $n {
            pub slot: u8,
        }
        impl Seeker for $n {
            fn slot(&self) -> u8 {
                self.slot
            }
        }
    )*};
}
seekers!(BreakSeeker, RaidBreakSeeker, OpenSeeker);
/// The door goals are woken by a wall contact on the path: `Req::Start` for the goal of each creature that bumped (a goal asks `can_use` itself).
fn seek<S: Seeker>(q: Query<(Entity, &S, &Contact), With<Following>>, mut reqs: ResMut<Requests>) {
    for (e, s, c) in &q {
        if c.horizontal {
            reqs.0.push(Req::Start(e, s.slot()));
        }
    }
}
/// The module's `tick`: `seek` of its marker (run by `arbitrate` while a creature carries the module, no system in the schedule).
pub fn tick<S: Seeker>(w: &mut World) {
    tick_state(w, seek::<S>);
}
