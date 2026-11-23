//! The ECS view of an entity as the tracker's `EntityState` (`lan/entity.rs`). The fields the ECS has no source for keep their
//! default (listed in the report of the greffe): equipment stacks, the metadata, the tracker's flags.
use crate::lan::entity::EntityState;
use bevy_ecs::world::World;
use horizon_ecs_proto::bridge::{ids::id_of, view::EntityView};
use horizon_ecs_proto::leash::Holder;

/// Writes `v` into `out` under the network id `id`. The leash names the holder's network id: a player's id, or the id of a followed entity.
pub fn fill(v: &EntityView, id: i32, world: &World, out: &mut EntityState) {
    out.reset(id, u128::from(v.uid), v.kind);
    out.position = v.position;
    out.velocity = v.velocity;
    out.yaw = v.yaw;
    out.on_ground = v.on_ground;
    out.horizontal_collision = v.horizontal_collision;
    if let Some((head_yaw, pitch)) = v.head {
        out.head_yaw = head_yaw;
        out.pitch = pitch;
    }
    if let Some(h) = v.hook {
        use horizon_protocol::entity_data::{serializer, Value};
        out.data = h.owner as i32;
        let hooked = match h.hooked {
            Some(horizon_ecs_proto::fishing_hook::Target::Entity(e)) => id_of(world, e),
            Some(horizon_ecs_proto::fishing_hook::Target::Player(p)) => Some(p),
            None => None,
        };
        if let Some(t) = hooked {
            out.meta.push((8, serializer::INT, Value::Int(t as i32 + 1)));
        }
        if h.biting {
            out.meta.push((9, serializer::BOOLEAN, Value::Bool(true)));
        }
    }
    out.leash = match v.leash {
        Some(Holder::Player(p)) => Some(p as i32),
        Some(Holder::Entity(e)) => id_of(world, e).map(|i| i as i32),
        _ => None,
    };
}
