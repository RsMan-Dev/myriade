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
fn evaluator(world: &World, e: Entity) -> Evaluator {
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
            if let Some(i) = path.nodes.iter().position(|n| mw.can_see_sky(n.x, n.y, n.z)) {
                path.nodes.truncate(i);
            }
        }
    }
    if path.nodes.is_empty() {
        return false;
    }
    let now = world.resource::<Clock>().of(e);
    if let Some(mut nav) = world.get_mut::<Nav>(e) {
        nav.path = Some(path);
        nav.speed = speed;
        nav.reach = 0;
        nav.last_check = now.saturating_sub(1);
        nav.last_pos = pm.position;
    }
    world.entity_mut(e).remove::<NeedsPath>().insert((Following, Moving));
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
    let t = path_type(kind)?;
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
