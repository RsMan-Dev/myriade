//! Target dispatch: the level, position and type a command's target names,
//! by family. Moved out of `mod.rs` unchanged (arismoko/horizon#2791's discipline).
use super::*;

impl Game<'_> {
    /// The dragon whose `EnderDragonPart` has this id (the dragon's id + 1 + k, k < 8), and k.
    pub(in crate::game) fn dragon_part_of(&self, id: u32) -> Option<(horizon_ecs_proto::bridge::dragon::DragonView, usize)> {
        self.ecs.dragon_views().into_iter().find_map(|(d, v)| {
            let k = id.checked_sub(d + 1)? as usize;
            (k < 8).then_some((v, k))
        })
    }
    /// The level `t` is in (`Entity.level`): a server's players have their
    /// own, which need not be the one in place; everything else is in the
    /// level in place.
    pub(in crate::game) fn target_level(&self, t: Target) -> Dim {
        let conn = match t {
            Target::Player => self.lan_acting,
            Target::Guest(conn) => Some(conn),
            _ => None,
        };
        conn.and_then(|c| self.command_guest(c))
            .map_or(self.dim, |g| Dim::from_id(&g.dimension))
    }
    pub(in crate::game) fn target_position(&self, t: Target) -> Option<[f64; 3]> {
        match t {
            Target::Player => self
                .player
                .as_ref()
                .map(|p| [p.position.x, p.position.y, p.position.z]),
            Target::Guest(conn) => self.command_guest(conn).map(|g| g.position),
            Target::Mob(id) => self.ecs.position(id),
            Target::Item(id) => self
                .ecs.item(id)
                .map(|i| [i.position.x, i.position.y, i.position.z]),
            Target::Orb(id) => self.ecs.orb(id).map(|o| [o.position.x, o.position.y, o.position.z]),
            Target::Projectile(id) => self.ecs_view(id).map(|v| v.position),
            Target::Stand(id) => self
                .armor_stand_saved(id)
                .map(|a| [a.position.x, a.position.y, a.position.z]),
            Target::Frame(id) => self.ecs_frames().into_iter().find(|f| f.id == id).map(|f| {
                let (lo, hi) = f.bounds();
                [
                    (lo[0] + hi[0]) / 2.0,
                    (lo[1] + hi[1]) / 2.0,
                    (lo[2] + hi[2]) / 2.0,
                ]
            }),
            Target::Painting(id) => self.ecs_paintings().into_iter().find(|p| p.id == id).map(|p| {
                let (lo, hi) = p.bounds();
                [
                    (lo[0] + hi[0]) / 2.0,
                    (lo[1] + hi[1]) / 2.0,
                    (lo[2] + hi[2]) / 2.0,
                ]
            }),
            Target::Boat(id) => self.ecs.position(id),
            Target::Cart(id) => self.ecs_view(id).map(|v| v.position),
            Target::Other(id) => self.ecs.position(id),
            Target::Crystal(id) => self.ecs.position(id),
            Target::Dragon(id) => self.ecs.position(id),
            Target::Cushion(id) => self.cushion_saved(id).map(|c| [c.position.x, c.position.y, c.position.z]),
            Target::DragonPart(id) => self.dragon_part_of(id).map(|(v, k)| v.parts[k]),
            Target::Tnt(id) | Target::Falling(id) => self.ecs.position(id),
            Target::Rocket(id) => self.ecs.position(id),
            Target::Cloud(id) => self.ecs.position(id),
            Target::Hook(id) => self.fishing_hook_by_id(id).map(|(p, _)| [p.x, p.y, p.z]),
            Target::Trident(id) => self.ecs.position(id),
            Target::Fangs(id) => self.ecs.position(id),
            Target::Spawner(id) => self.ecs.position(id),
            Target::Knot(id) => self.natural_knot(id).map(|fence| {
                let p = horizon_client::mobs::leash::knot_state(fence).position;
                [p.x, p.y, p.z]
            }),
            Target::General(id) => self.ecs_view(id).map(|v| v.position),
        }
    }
    pub(in crate::game) fn target_type(&self, t: Target) -> String {
        match t {
            Target::Player | Target::Guest(_) => "minecraft:player".into(),
            Target::Mob(id) => self.ecs.kind(id).unwrap_or_default().to_owned(),
            Target::Item(_) => "minecraft:item".into(),
            Target::Orb(_) => "minecraft:experience_orb".into(),
            Target::Projectile(id) => self.ecs_mob_kind(id).unwrap_or_default().into(),
            Target::Stand(_) => "minecraft:armor_stand".into(),
            Target::General(id) => self.ecs_mob_kind(id).unwrap_or_default().into(),
            Target::Frame(id) => {
                if self.ecs_frames().into_iter().any(|f| f.id == id && f.glow) {
                    "minecraft:glow_item_frame".into()
                } else {
                    "minecraft:item_frame".into()
                }
            }
            Target::Painting(_) => "minecraft:painting".into(),
            Target::Boat(id) => self.boat_entity_type(id).unwrap_or_else(|| "minecraft:oak_boat".into()),
            Target::Cart(id) => self
                .ecs
                .cart_kind(id)
                .map_or_else(|| "minecraft:minecart".into(), Into::into),
            Target::Other(id) => self.ecs.kind(id).map(str::to_owned).unwrap_or_default(),
            Target::Crystal(_) => "minecraft:end_crystal".into(),
            // `EnderDragonPart` takes its parent's type.
            Target::Dragon(_) | Target::DragonPart(_) => "minecraft:ender_dragon".into(),
            Target::Tnt(_) => "minecraft:tnt".into(),
            Target::Cushion(_) => "minecraft:cushion".into(),
            Target::Falling(_) => "minecraft:falling_block".into(),
            Target::Rocket(_) => "minecraft:firework_rocket".into(),
            Target::Trident(_) => "minecraft:trident".into(),
            Target::Fangs(_) => "minecraft:evoker_fangs".into(),
            Target::Spawner(_) => "minecraft:ominous_item_spawner".into(),
            Target::Cloud(_) => "minecraft:area_effect_cloud".into(),
            Target::Hook(_) => "minecraft:fishing_bobber".into(),
            Target::Knot(_) => "minecraft:leash_knot".into(),
        }
    }
}

/// The network id of a target that is no player: its entity id.
pub(super) fn non_player_entity_id(t: Target) -> Option<i32> {
    let (Target::Mob(id)
    | Target::Item(id)
    | Target::Orb(id)
    | Target::Projectile(id)
    | Target::Stand(id)
    | Target::Frame(id)
    | Target::Painting(id)
    | Target::Boat(id)
    | Target::Cart(id)
    | Target::Other(id)
    | Target::Fangs(id)
    | Target::Crystal(id)
    | Target::Dragon(id)
    | Target::DragonPart(id)
    | Target::Tnt(id)
    | Target::Cushion(id)
    | Target::Falling(id)
    | Target::Rocket(id)
    | Target::Trident(id)
    | Target::Spawner(id)
    | Target::Cloud(id)
    | Target::Hook(id)
    | Target::General(id)
    | Target::Knot(id)) = t
    else {
        return None;
    };
    i32::try_from(id).ok()
}
