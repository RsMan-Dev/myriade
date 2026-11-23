//! The navigation a mod drives on a creature: a path it supplies (followed as given, no search), the flags of `PathNavigation` and
//! the path maluses. They are written on `NavParams`, the creature's own copy of its navigation, which every search reads.
use crate::behaviours::{nearest_bed::path_mob, restrict_sun::AvoidSun};
use crate::cow::{Clock, Following, Moving, Nav, NavParams, NeedsPath};
use crate::hostile::Daylight;
use crate::locomotion::{FlyMode, SwimMode, swim::SwimParams};
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::{
    MobWorld,
    path::{Evaluator, Path, PathNode, PathType},
};

/// The flag that is a marker (`AvoidSun`), not a field of `NavParams`.
const AVOID_SUN: &str = "avoid_sun";

/// The path type `kind` names (`PathType::from_name`); None for a name that is none.
fn path_type(kind: &str) -> Option<PathType> {
    let t = PathType::from_name(kind);
    (t != PathType::Blocked || kind == "BLOCKED").then_some(t)
}

/// The configuration flag `name` of `n`; None for a name that is no configuration flag.
fn config_flag(n: &NavParams, name: &str) -> Option<bool> {
    Some(match name {
        "can_float" => n.can_float,
        "can_open_doors" => n.can_open_doors,
        "can_pass_doors" => n.can_pass_doors,
        _ => return None,
    })
}

/// Writes the configuration flag `name` of `n`; false for a name that is no configuration flag.
fn set_config_flag(n: &mut NavParams, name: &str, on: bool) -> bool {
    match name {
        "can_float" => n.can_float = on,
        "can_open_doors" => n.can_open_doors = on,
        "can_pass_doors" => n.can_pass_doors = on,
        _ => return false,
    }
    true
}

/// The evaluator the creature's search uses, by its mode: the air's, the water's (amphibious or not), else the walk's, the
/// amphibious one when the species has it (as the cow's `pathfind`).
pub fn evaluator(world: &World, e: Entity) -> Evaluator {
    let swim = world.get::<SwimParams>(e);
    if world.get::<FlyMode>(e).is_some() {
        Evaluator::Fly
    } else if world.get::<SwimMode>(e).is_some() {
        match swim.and_then(|p| p.amphibious) {
            Some(prefers_shallow) => Evaluator::Amphibious { prefers_shallow },
            None => Evaluator::Swim { breach: swim.is_some_and(|p| p.breach) },
        }
    } else {
        match swim.and_then(|p| p.amphibious) {
            Some(prefers_shallow) => Evaluator::Amphibious { prefers_shallow },
            None => Evaluator::Walk,
        }
    }
}

/// `GroundPathNavigation.trimPath` with `avoidSun`: the nodes before the first one the sky sees are kept.
pub fn trim_before_sky(path: &mut Path, sees_sky: impl Fn(i32, i32, i32) -> bool) {
    if let Some(i) = path.nodes.iter().position(|n| sees_sky(n.x, n.y, n.z)) {
        path.nodes.truncate(i);
    }
}

/// The node clock of a supplied path (the node timeout of `doStuckDetection`): the node it counts, its ticks there and the limit.
/// Set by `follow` on the creature, removed when the path ends.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NodeClock {
    pub node: usize,
    pub timer: u32,
    pub limit: u32,
}

/// The ticks a creature at `at` may take to reach `n` at `speed`: `dist / speed * 20`, saturating (a zero speed never times out).
fn node_limit(at: [f64; 3], n: &PathNode, speed: f64) -> u32 {
    let (dx, dy, dz) = (f64::from(n.x) + 0.5 - at[0], f64::from(n.y) - at[1], f64::from(n.z) + 0.5 - at[2]);
    ((dx * dx + dy * dy + dz * dz).sqrt() / speed * 20.0) as u32
}

/// The node timeout of a supplied path: a creature that stays on one node past three times its limit drops the path.
/// Only the creatures that carry a `NodeClock` are visited.
pub fn node_timeout(mut cmd: Commands, mut q: Query<(Entity, &mut NodeClock, &mut Nav, &crate::cow::Pos)>) {
    for (e, mut c, mut nav, pos) in &mut q {
        let speed = nav.speed;
        let Some(path) = nav.path.as_mut().filter(|p| !p.is_done()) else {
            cmd.entity(e).remove::<NodeClock>();
            continue;
        };
        if path.next != c.node {
            let at = [pos.0.x, pos.0.y, pos.0.z];
            *c = NodeClock { node: path.next, timer: 0, limit: node_limit(at, &path.next_node(), speed) };
        } else {
            c.timer += 1;
            if c.timer > c.limit.saturating_mul(3) {
                nav.path = None;
                cmd.entity(e).remove::<(NodeClock, Following, Moving)>().insert(crate::cow::PathEnded);
            }
        }
    }
}

/// The node timeout runs after `follow` moved the creatures of the tick.
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(node_timeout.after(crate::cow::follow));
}

/// `PathNavigation.moveTo(Path, speed)` for the path `nodes` a mod supplies: each node is floored and classified, the path is
/// trimmed as the oracle trims it, then followed as given (no search, no recompute target). False when `e` has no navigation
/// or no node is left.
pub fn follow(world: &mut World, mw: &MobWorld<'_>, e: Entity, nodes: &[[f64; 3]], speed: f64) -> bool {
    let e = crate::mount::nav_owner(world, e);
    if world.get::<Nav>(e).is_none() {
        return false;
    }
    let Some((pm, _)) = path_mob(world, e) else { return false };
    let nodes: Vec<PathNode> = nodes
        .iter()
        .map(|at| PathNode { x: at[0].floor() as i32, y: at[1].floor() as i32, z: at[2].floor() as i32, kind: PathType::Blocked })
        .collect();
    let target = nodes.last().map_or([0; 3], |n| [n.x, n.y, n.z]);
    let mut path = Path { nodes, next: 0, target, dist_to_target: 0.0, reached: true };
    let finder = crate::pathsearch::Finder::new(mw, &pm, evaluator(world, e));
    for n in &mut path.nodes {
        n.kind = finder.path_type_at(n.x, n.y, n.z);
    }
    path.raise_cauldron_nodes(|x, y, z| mw.has_tag("minecraft:cauldrons", mw.block_name(x, y, z)));
    // `GroundPathNavigation.trimPath` with `avoidSun`, as the cow's pathfind applies it.
    if world.get::<AvoidSun>(e).is_some() && world.get_resource::<Daylight>().is_some_and(|d| d.0) {
        let p = pm.position;
        if !mw.can_see_sky(p.x.floor() as i32, (p.y + 0.5).floor() as i32, p.z.floor() as i32) {
            trim_before_sky(&mut path, |x, y, z| mw.can_see_sky(x, y, z));
        }
    }
    if path.nodes.is_empty() {
        return false;
    }
    let now = world.resource::<Clock>().of(e);
    let at = [pm.position.x, pm.position.y, pm.position.z];
    let clock = NodeClock { node: 0, timer: 0, limit: node_limit(at, &path.nodes[0], speed) };
    if let Some(mut nav) = world.get_mut::<Nav>(e) {
        nav.path = Some(path);
        nav.speed = speed;
        nav.reach = 0;
        nav.last_check = now.saturating_sub(1);
        nav.last_pos = pm.position;
    }
    world.entity_mut(e).remove::<NeedsPath>().insert((Following, Moving, clock));
    true
}

/// The flag `name` of `e`: the marker `AvoidSun` for `avoid_sun`, else the field of `NavParams`. None for an unknown name or a
/// creature without `NavParams`.
pub fn flag(world: &World, e: Entity, name: &str) -> Option<bool> {
    let n = world.get::<NavParams>(e)?;
    if name == AVOID_SUN {
        return Some(world.get::<AvoidSun>(e).is_some());
    }
    config_flag(n, name)
}

/// Writes the flag `name` of `e` (the marker `AvoidSun` for `avoid_sun`); false for an unknown name or a creature without
/// `NavParams`.
pub fn set_flag(world: &mut World, e: Entity, name: &str, on: bool) -> bool {
    if world.get::<NavParams>(e).is_none() {
        return false;
    }
    if name == AVOID_SUN {
        if on {
            world.entity_mut(e).insert(AvoidSun);
        } else {
            world.entity_mut(e).remove::<AvoidSun>();
        }
        return true;
    }
    world.get_mut::<NavParams>(e).is_some_and(|mut n| set_config_flag(&mut n, name, on))
}

/// The path malus of `kind` for `e` (`NavParams.malus`, else the default of the type). None for an unknown type or a creature
/// without `NavParams`.
pub fn malus(world: &World, e: Entity, kind: &str) -> Option<f32> {
    malus_of(world, e, path_type(kind)?)
}

/// The path malus of the type `t` for `e` (see `malus`).
pub fn malus_of(world: &World, e: Entity, t: PathType) -> Option<f32> {
    let n = world.get::<NavParams>(e)?;
    Some(n.malus[t as usize].unwrap_or_else(|| t.malus()))
}

/// Writes the path malus of `kind` for `e`; false for an unknown type or a creature without `NavParams`.
pub fn set_malus(world: &mut World, e: Entity, kind: &str, value: f32) -> bool {
    let Some(t) = path_type(kind) else { return false };
    world.get_mut::<NavParams>(e).is_some_and(|mut n| {
        n.malus[t as usize] = Some(value);
        true
    })
}
