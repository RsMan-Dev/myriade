//! Two parity operations on the dedicated server's ECS: the Happy Ghast leash dispatch and the shulker's attachment spots.
//! Each case runs on the server's own level and answers with the keys of its vanilla reference.
use crate::game::Game;
use crate::mobs::path::{Path, PathNode, PathType};
use myriade_engine::Result;
use horizon_ecs_proto::{
    behaviours::teleport_on,
    cow::{Contact, MoveTo, Nav},
    leash::{self, Holder},
};
use serde_json::{Value, json};

impl Game<'_> {
    /// `happy_ghast_leash`: one leash step per row, held by a fence knot at (0, 64, 0), with a path to (8, 64, 8) and the move control at (20, 70, 20).
    pub(super) fn op_happy_ghast_leash(&mut self, op: &Value) -> Result<Value> {
        let rows = op["rows"].as_array().ok_or("happy_ghast_leash: missing rows")?;
        let fence = self.pack.assets.registry.default_state("minecraft:oak_fence").ok_or("happy_ghast_leash: no fence")?;
        let _ = self.world.set(0, 64, 0, fence);
        let mut out = Vec::new();
        for row in rows {
            let kind = row["kind"].as_str().ok_or("happy_ghast_leash: missing kind")?;
            let at: [f64; 3] = std::array::from_fn(|i| row["position"][i].as_f64().unwrap_or(0.0));
            let name = if kind == "cow" { "minecraft:cow" } else { "minecraft:happy_ghast" };
            let id = self.ecs_spawn(name, at, None).ok_or("happy_ghast_leash: no creature")?;
            let e = self.ecs.entity_of(id).ok_or("happy_ghast_leash: no entity")?;
            if kind == "baby" {
                if let Some(ecs) = self.ecs.ecs_mut() {
                    ecs.set_baby(e, true);
                }
            }
            if let Some(ecs) = self.ecs.ecs_mut() {
                ecs.set_wanted(e, [20.0, 70.0, 20.0], 1.0);
            }
            if let Some(w) = self.ecs.world_mut() {
                let path = Path {
                    nodes: vec![PathNode { x: 8, y: 64, z: 8, kind: PathType::Open }],
                    next: 0,
                    target: [8, 64, 8],
                    dist_to_target: 0.0,
                    reached: true,
                };
                if let Some(mut nav) = w.get_mut::<Nav>(e) {
                    nav.path = Some(path);
                }
                if let Some(mut c) = w.get_mut::<Contact>(e) {
                    c.on_ground = false;
                }
                leash::attach_from_save(w, e, Holder::Knot([0, 64, 0]));
            }
            self.tick_ecs()?;
            let (snapped, wanted, path) = {
                let w = self.ecs.world().ok_or("happy_ghast_leash: no level")?;
                let wanted = w.get::<MoveTo>(e).is_some_and(|m| m.active);
                let path = w.get::<Nav>(e).and_then(|n| n.path.as_ref().map(|p| p.target));
                (!leash::is_leashed(w, e), wanted, path)
            };
            let [vx, vy, vz] = self.ecs_mob_velocity(id).map_or([0.0; 3], |v| [v.x, v.y, v.z]);
            out.push(json!({
                "kind": kind, "distance": row["distance"], "phase": row["phase"], "position": row["position"],
                "snapped": snapped, "wanted": wanted, "path": path, "velocity": [vx, vy, vz],
            }));
            self.ecs.despawn(e);
        }
        if let Some(air) = self.pack.assets.registry.default_state("minecraft:air") {
            let _ = self.world.set(0, 64, 0, air);
        }
        Ok(Value::Array(out))
    }

    /// `shulker_space`: whether a shulker at `current` may stay at `target` with `face`, per row, with the row's blocks placed on the server.
    pub(super) fn op_shulker_space(&mut self, op: &Value) -> Result<Value> {
        let rows = op["rows"].as_array().ok_or("shulker_space: missing rows")?;
        let air = self.pack.assets.registry.default_state("minecraft:air").ok_or("shulker_space: no air")?;
        let mut touched: Vec<[i32; 3]> = Vec::new();
        for row in rows {
            for cell in row["cells"].as_array().ok_or("shulker_space: missing cells")? {
                touched.push(std::array::from_fn(|i| cell[i].as_i64().unwrap_or(0) as i32));
            }
        }
        let mut out = Vec::new();
        for row in rows {
            for p in &touched {
                let _ = self.world.set(p[0], p[1], p[2], air);
            }
            for cell in row["cells"].as_array().ok_or("shulker_space: missing cells")? {
                let name = cell[3].as_str().ok_or("shulker_space: missing block")?;
                let state = self.pack.assets.registry.default_state(name).ok_or("shulker_space: unknown block")?;
                let p: [i32; 3] = std::array::from_fn(|i| cell[i].as_i64().unwrap_or(0) as i32);
                let _ = self.world.set(p[0], p[1], p[2], state);
            }
            let current: [f64; 3] = std::array::from_fn(|i| row["current"][i].as_f64().unwrap_or(0.0));
            let target: [i32; 3] = std::array::from_fn(|i| row["target"][i].as_i64().unwrap_or(0) as i32);
            let face = row["face"].as_u64().unwrap_or(0) as u8;
            let id = self.ecs_spawn("minecraft:shulker", current, None).ok_or("shulker_space: no shulker")?;
            let e = self.ecs.entity_of(id).ok_or("shulker_space: no entity")?;
            let stays = self.ecs.world().is_some_and(|w| teleport_on::can_stay_at(w, e, target, face));
            self.ecs.despawn(e);
            out.push(Value::Bool(stays));
        }
        for p in &touched {
            let _ = self.world.set(p[0], p[1], p[2], air);
        }
        Ok(Value::Array(out))
    }
}
