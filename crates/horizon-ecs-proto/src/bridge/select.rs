//! What a host query names: an entity by kind, by tag and by radius. The radius is answered by the creature index (`Index`), never by a
//! scan of the world; a query without a radius reads every entity once (the only scan of the bridge, and no tick uses it).
use crate::cow::{Index, Pos, TypeName};
use crate::type_tags::{self, TagId};
use bevy_ecs::{entity::Entity, entity_disabling::Disabled, prelude::*};

/// The filters of a query: the species' name (`kind`), a tag (`tag`), a sphere (`near`: centre and radius), and at most `limit` results
/// (0: no limit). Sleeping entities are never returned.
#[derive(Clone, Debug, Default)]
pub struct Select {
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub near: Option<([f64; 3], f64)>,
    pub limit: usize,
}

/// The awake entities that match `q`, in entity order.
pub fn select(world: &World, q: &Select) -> Vec<Entity> {
    let Some(tag) = tag_of(q) else { return Vec::new() };
    let candidates: Vec<Entity> = match q.near {
        Some((at, r)) => {
            let mut v = Vec::new();
            world.resource::<Index>().for_radius(at[0], at[2], r, |e| v.push(e));
            v
        }
        None => world.iter_entities().map(|r| r.id()).collect(),
    };
    let mut out: Vec<Entity> = candidates.into_iter().filter(|&e| keep(world, q, tag, e)).collect();
    out.sort_unstable();
    out.dedup();
    if q.limit > 0 {
        out.truncate(q.limit);
    }
    out
}

/// How many entities `select` would return for `q`, without its list when a sphere is given.
pub fn count(world: &World, q: &Select) -> usize {
    let Some(tag) = tag_of(q) else { return 0 };
    let n = match q.near {
        Some((at, r)) => {
            let mut n = 0;
            world.resource::<Index>().for_radius(at[0], at[2], r, |e| n += usize::from(keep(world, q, tag, e)));
            n
        }
        None => world.iter_entities().filter(|r| keep(world, q, tag, r.id())).count(),
    };
    if q.limit > 0 { n.min(q.limit) } else { n }
}

/// The tag id of `q`: None when the query names a tag that does not exist (no entity has it), Some(None) when it names none.
fn tag_of(q: &Select) -> Option<Option<TagId>> {
    match q.tag.as_deref() {
        Some(t) => type_tags::id(t).map(Some),
        None => Some(None),
    }
}

fn keep(world: &World, q: &Select, tag: Option<TagId>, e: Entity) -> bool {
    let Ok(r) = world.get_entity(e) else { return false };
    if r.contains::<Disabled>() {
        return false;
    }
    if q.kind.as_deref().is_some_and(|k| r.get::<TypeName>().map(|t| t.0) != Some(k)) {
        return false;
    }
    if let Some((at, rad)) = q.near {
        let Some(p) = r.get::<Pos>().map(|p| p.0) else { return false };
        let d = [p.x - at[0], p.y - at[1], p.z - at[2]];
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > rad * rad {
            return false;
        }
    }
    tag.is_none_or(|t| type_tags::has(world, t, e))
}
