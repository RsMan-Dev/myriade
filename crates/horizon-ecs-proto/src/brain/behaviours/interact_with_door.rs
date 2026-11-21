//! `InteractWithDoor` (`Brain.InteractWithDoor`, the copper golem's core activity, priority 3; `animal_brain.rs:1400-1425`
//! (`try_start`), `:3149-3206` (`interact_with_doors`, `door_open`, `close_doors`)): a one-shot that, on a path, opens the doors
//! of the two nodes of the step being taken and closes the doors it has passed when they are within 3 blocks. Its state (the
//! last node, the cooldown, the doors to close) is a component of the entity, not a memory: a memory write wakes the brain,
//! which would ask again every tick.
use crate::behaviours::Cap;
use crate::blocks;
use crate::brain::*;
use crate::cow::{Nav, Pos};
use crate::rules::interact_with_door as rules;
use bevy_ecs::prelude::Component;
use horizon_client_lib::mobs::MobWorld;
use horizon_world::registry::BlockStateId;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:interact_with_door",
    requires: &[Cap::Body, Cap::Locomotion],
    one_shot: true,
    equip,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

/// The species' values: the cooldown set when the same node is asked twice (20 ticks, `animal_brain.rs:1416`), and the
/// squared distance under which a passed door is closed (9 = 3 blocks, `close_doors`).
pub struct Params {
    pub cooldown: i64,
    pub close_range_sqr: f64,
}
impl Params {
    pub const DEFAULT: Params = Params { cooldown: 20, close_range_sqr: 9.0 };
}

/// The state of the `Mob`'s `InteractWithDoor`: `last_node`, `cooldown`, `doors_to_close`.
#[derive(Component, Default, Clone, Debug)]
pub struct DoorState {
    pub last_node: Option<[i32; 3]>,
    pub cooldown: i64,
    pub doors: Vec<[i32; 3]>,
}

fn equip(w: &mut World, e: Entity, _: &Ctx) {
    if w.get::<DoorState>(e).is_none() {
        w.entity_mut(e).insert(DoorState::default());
    }
}

/// The step being taken: the node the path leaves and the node it goes to. `None` without a path to walk
/// (`path.next == 0 || path.is_done()`, as the `Mob`). The path is read in `Nav.path` (`cow.rs:121`, `Path.nodes`, `next`).
fn step(w: &World, e: Entity) -> Option<([i32; 3], [i32; 3])> {
    let path = w.get::<Nav>(e)?.path.as_ref()?;
    if path.next == 0 || path.is_done() {
        return None;
    }
    let (from, to) = (path.nodes[path.next - 1], path.next_node());
    Some(([from.x, from.y, from.z], [to.x, to.y, to.z]))
}

/// The step counter of `try_start`: the same node asked again sets the cooldown; another node counts it down, and the step
/// waits while it stays above 0. The counter moves whether or not the step is taken; the node is recorded only when it is.
fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let Some((_, to)) = step(w, e) else { return false };
    let p = c.get::<Params>();
    let Some(mut s) = w.get_mut::<DoorState>(e) else { return false };
    let (cooldown, taken) = rules::step_gate(s.last_node, to, s.cooldown, p.cooldown);
    s.cooldown = cooldown;
    // The Mob records the node only when the step is taken (`last_node = next` follows the early return).
    if taken {
        s.last_node = Some(to);
    }
    taken
}

/// `interact_with_doors`: asks for the doors of the step to open (the one left always, the one ahead only when it is closed),
/// then closes the remembered doors that are neither of the step and are within reach, and forgets the others.
fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some((from, to)) = step(w, e) else { return };
    let p = c.get::<Params>();
    let mw = blocks::world_of(w);
    let mut doors = w.get::<DoorState>(e).map(|s| s.doors.clone()).unwrap_or_default();
    let mut changes: Vec<([i32; 3], bool)> = Vec::new();
    for (door, always) in [(from, true), (to, false)] {
        let Some(open) = door_open(mw, door) else { continue };
        if !open {
            changes.push((door, true));
        }
        if (always || !open) && !doors.contains(&door) {
            doors.push(door);
        }
    }
    if let Some(me) = w.get::<Pos>(e).map(|q| q.0) {
        let mut kept = Vec::new();
        for door in doors {
            if from == door || to == door {
                kept.push(door);
                continue;
            }
            if !rules::in_close_range(me, door, p.close_range_sqr) || door_open(mw, door) != Some(true) {
                continue;
            }
            changes.push((door, false));
        }
        doors = kept;
    }
    if let Some(mut s) = w.get_mut::<DoorState>(e) {
        s.doors = doors;
    }
    for (door, open) in changes {
        // The other half takes the `open` of this one (`DoorBlock.updateShape` copies it from the neighbour half).
        let other = rules::other_half_dy(blocks::property(mw, door, "half").as_deref() == Some("upper"));
        for p in [door, [door[0], door[1] + other, door[2]]] {
            if door_open(mw, p).is_none() {
                continue;
            }
            if let Some(id) = door_with_open(mw, p, open) {
                blocks::set_state_id(w, p, id);
            }
        }
    }
}

/// `door_open`: a block whose name ends with `_door` and that carries the tag `minecraft:mob_interactable_doors`; its `open`.
fn door_open(mw: &MobWorld, p: [i32; 3]) -> Option<bool> {
    let name = mw.block_name(p[0], p[1], p[2]);
    (name.ends_with("_door") && mw.has_tag("minecraft:mob_interactable_doors", name)).then(|| mw.prop(p[0], p[1], p[2], "open") == Some("true"))
}

/// The block state at `p` with `open` changed and the other properties kept (`blocks::resolve` starts from the default state,
/// which would lose the facing and the hinge).
fn door_with_open(mw: &MobWorld, p: [i32; 3], open: bool) -> Option<BlockStateId> {
    let mut state = mw.registry.state(blocks::state_id(mw, p))?.clone();
    state.properties.insert("open".into(), if open { "true" } else { "false" }.into());
    mw.registry.id(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use horizon_client_lib::mobs::path::{Path, PathNode, PathType};

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static ENTRIES: [BEntry; 1] = [BEntry { behaviour: &BEHAVIOUR, activity: "core", priority: 3, params: &Params::DEFAULT, needs: &[], duration: None, kids: &[] }];

    fn brained(h: &mut Herd) -> Entity {
        let def = BrainDef {
            sensors: &[],
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: &ENTRIES,
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 91);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(bundle(plan));
        crate::brain::start(&mut h.world, e);
        e
    }
    /// The path of the entity: `nodes`, and the index of the next node (the step goes from `next - 1` to `next`).
    fn walk(h: &mut Herd, e: Entity, nodes: &[[i32; 3]], next: usize) {
        let nodes: Vec<PathNode> = nodes.iter().map(|p| PathNode { x: p[0], y: p[1], z: p[2], kind: PathType::Walkable }).collect();
        let last = nodes.last().map(|n| [n.x, n.y, n.z]).unwrap_or([0; 3]);
        h.world.get_mut::<Nav>(e).unwrap().path = Some(Path { nodes, next, target: last, dist_to_target: 0.0, reached: false });
    }
    /// One brain step (the entity is awake after a path change) and the world tick that applies the block changes.
    fn step(h: &mut Herd, e: Entity) {
        wake(&mut h.world, e);
        brain_step(&mut h.world);
        h.tick();
    }
    fn door(h: &mut Herd, p: [i32; 3], name: &'static str, open: bool) {
        let props: &'static [(&'static str, &'static str)] = if open { &[("open", "true")] } else { &[("open", "false")] };
        h.set_block_state(p[0], p[1], p[2], name, props);
    }
    /// The `open` property of the block at `p`, whatever the block is.
    fn open_prop(h: &Herd, p: [i32; 3]) -> Option<String> {
        blocks::property(blocks::world_of(&h.world), p, "open")
    }
    fn is_open(h: &Herd, p: [i32; 3]) -> Option<bool> {
        open_prop(h, p).map(|v| v == "true")
    }
    fn state(h: &Herd, e: Entity) -> DoorState {
        h.world.get::<DoorState>(e).unwrap().clone()
    }
    /// A cow standing at (x, 1, z) in the middle of the step's block.
    fn at(h: &mut Herd, e: Entity, x: f64, z: f64) {
        h.teleport(e, Vec3::new(x, 1.0, z));
    }

    /// On a path through two closed doors, the step opens both and remembers both; with no path, nothing moves.
    #[test]
    fn it_opens_the_doors_of_its_step_and_remembers_them() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        at(&mut h, e, 1.5, 0.5);
        door(&mut h, [0, 1, 0], "minecraft:oak_door", false);
        door(&mut h, [1, 1, 0], "minecraft:oak_door", false);
        h.tick();
        step(&mut h, e);
        assert_eq!(is_open(&h, [0, 1, 0]), Some(false), "sans chemin, rien ne bouge");
        walk(&mut h, e, &[[0, 1, 0], [1, 1, 0], [2, 1, 0]], 1);
        step(&mut h, e);
        assert_eq!(is_open(&h, [0, 1, 0]), Some(true), "la porte quittée s'ouvre");
        assert_eq!(is_open(&h, [1, 1, 0]), Some(true), "la porte d'en face s'ouvre");
        assert!(state(&h, e).doors.contains(&[0, 1, 0]) && state(&h, e).doors.contains(&[1, 1, 0]), "les deux portes sont retenues");
    }

    /// The door passed through is closed once the entity is within 3 blocks of it on the next step; a far one is forgotten and stays open.
    #[test]
    fn it_closes_a_passed_door_within_three_blocks_and_forgets_a_far_one() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        at(&mut h, e, 1.5, 0.5);
        door(&mut h, [0, 1, 0], "minecraft:oak_door", false);
        door(&mut h, [1, 1, 0], "minecraft:oak_door", false);
        door(&mut h, [2, 1, 0], "minecraft:oak_door", false);
        door(&mut h, [8, 1, 0], "minecraft:oak_door", true);
        h.tick();
        walk(&mut h, e, &[[0, 1, 0], [1, 1, 0]], 1);
        step(&mut h, e);
        h.world.get_mut::<DoorState>(e).unwrap().doors.push([8, 1, 0]);
        walk(&mut h, e, &[[1, 1, 0], [2, 1, 0]], 1);
        step(&mut h, e);
        assert_eq!(is_open(&h, [0, 1, 0]), Some(false), "la porte passée, à moins de 3 blocs, se ferme");
        assert_eq!(is_open(&h, [2, 1, 0]), Some(true), "la porte du pas suivant s'ouvre");
        assert_eq!(is_open(&h, [8, 1, 0]), Some(true), "une porte à plus de 3 blocs n'est pas fermée");
        assert!(!state(&h, e).doors.contains(&[8, 1, 0]), "et elle est oubliée");
    }

    /// A door outside the tag (iron) is never touched: the wooden door of the same step opens, the iron one stays closed.
    #[test]
    fn it_does_not_touch_a_door_outside_the_mob_tag() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        at(&mut h, e, 1.5, 0.5);
        door(&mut h, [0, 1, 0], "minecraft:oak_door", false);
        door(&mut h, [1, 1, 0], "minecraft:iron_door", false);
        h.tick();
        walk(&mut h, e, &[[0, 1, 0], [1, 1, 0]], 1);
        step(&mut h, e);
        assert_eq!(is_open(&h, [0, 1, 0]), Some(true), "témoin : la porte de bois s'ouvre");
        assert_eq!(open_prop(&h, [1, 1, 0]).as_deref(), Some("false"), "la porte de fer n'est pas dans le tag");
        assert!(!state(&h, e).doors.contains(&[1, 1, 0]), "et elle n'est pas retenue");
    }

    /// The same node asked twice sets a 20-tick cooldown; a different node then counts it down and is refused while it stays above 0.
    #[test]
    fn it_waits_its_cooldown_before_a_new_node() {
        let mut h = Herd::with_slab_awake(40, true, false);
        let e = brained(&mut h);
        at(&mut h, e, 1.5, 0.5);
        door(&mut h, [0, 1, 0], "minecraft:oak_door", false);
        door(&mut h, [1, 1, 0], "minecraft:oak_door", false);
        door(&mut h, [2, 1, 0], "minecraft:oak_door", false);
        h.tick();
        walk(&mut h, e, &[[0, 1, 0], [1, 1, 0]], 1);
        step(&mut h, e);
        assert_eq!(state(&h, e).cooldown, -1, "témoin : premier passage, le compteur passe sous 0");
        step(&mut h, e);
        assert_eq!(state(&h, e).cooldown, 20, "le même noeud redemandé met le délai à 20");
        walk(&mut h, e, &[[1, 1, 0], [2, 1, 0]], 1);
        step(&mut h, e);
        assert_eq!(state(&h, e).cooldown, 19, "un autre noeud décompte le délai");
        assert_eq!(is_open(&h, [2, 1, 0]), Some(false), "pendant le délai, le pas suivant est refusé");
    }
}
