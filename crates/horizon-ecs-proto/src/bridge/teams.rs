//! The scoreboard's teams as alliances (greffe lot 38). A member is in one team at most, as on the board. The team of the board is
//! sent as an event (`set_team`, `clear_team`), never read per tick; `allied` is a hash lookup for the rules that choose a target.
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use std::collections::HashMap;

/// The teams by name (their members) and the team of each member.
#[derive(Resource, Default)]
pub struct Teams {
    pub members: HashMap<String, Vec<Who>>,
    of: HashMap<Who, String>,
}

/// Whether `a` and `b` are in the same team (`Entity.isAlliedTo` for the team part of the rule).
pub fn allied_in(t: &Teams, a: Who, b: Who) -> bool {
    matches!((t.of.get(&a), t.of.get(&b)), (Some(x), Some(y)) if x == y)
}

/// [`allied_in`] for the code that holds the world: no team is known until a team is set.
pub fn allied(w: &World, a: Who, b: Who) -> bool {
    w.get_resource::<Teams>().is_some_and(|t| allied_in(t, a, b))
}

/// Sets the members of the team `name`: a member that was in another team leaves it.
pub fn set_team(world: &mut World, name: &str, members: &[Who]) {
    clear_team(world, name);
    world.init_resource::<Teams>();
    let t = world.resource_mut::<Teams>().into_inner();
    for &m in members {
        if let Some(old) = t.of.insert(m, name.to_owned()).filter(|old| old != name)
            && let Some(v) = t.members.get_mut(&old)
        {
            v.retain(|&x| x != m);
        }
    }
    t.members.insert(name.to_owned(), members.to_vec());
}

/// The team `name` has no member any more.
pub fn clear_team(world: &mut World, name: &str) {
    let Some(t) = world.get_resource_mut::<Teams>() else { return };
    let t = t.into_inner();
    for m in t.members.remove(name).unwrap_or_default() {
        if t.of.get(&m).map(String::as_str) == Some(name) {
            t.of.remove(&m);
        }
    }
}
