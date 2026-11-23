//! The breathing and fall-reset cases of the mob parity harnesses, run on the server's ECS: a cow per case, read back after one tick.
//! The blocks of the breathing scenes are set by the harness before the operation.
use crate::game::Game;
use horizon_client::Result;
use horizon_core::geometry::Vec3;
use serde_json::{Value, json};

impl Game<'_> {
    /// `LivingEntity.baseTick` air of a cow in each scene, with each effect mask and air value: the air after one tick and the health bits.
    pub(super) fn op_mob_breathing(&mut self, _op: &Value) -> Result<Value> {
        let mut rows = Vec::new();
        for (scene, x, z) in [(0, 2.5, 2.5), (1, 6.5, 2.5), (2, 10.5, 2.5), (3, 6.5, 6.5), (4, 10.5, 6.5)] {
            for mask in 0..8 {
                for air in [301, 300, 10, 0, -19, -20, -21] {
                    let id = self.ecs_spawn("minecraft:cow", [x, 64.0, z], None).ok_or("mob_breathing: no cow")?;
                    self.ecs.set_no_ai(id, true);
                    self.ecs.set_no_gravity(id, true);
                    self.ecs.set_air(id, air);
                    for (bit, effect) in [(1, "minecraft:water_breathing"), (2, "minecraft:conduit_power"), (4, "minecraft:breath_of_the_nautilus")] {
                        if mask & bit != 0 {
                            self.ecs_add_effect(id, effect, 100, 0);
                        }
                    }
                    self.tick_ecs()?;
                    let air_after = self.ecs.air(id);
                    let health = self.ecs_living_health(id).unwrap_or(0.0).to_bits();
                    rows.push(json!({"scene": scene, "mask": mask, "air": air, "air_after": air_after, "health": format!("{health:08x}")}));
                    if let Some(e) = self.ecs.entity_of(id) {
                        self.ecs.despawn(e);
                    }
                }
            }
        }
        Ok(Value::Array(rows))
    }

    /// `LivingEntity.aiStep` fall distance of a cow in the dry air, for each flight effect, NoAI state and vertical velocity.
    pub(super) fn op_mob_fall_reset(&mut self, _op: &Value) -> Result<Value> {
        let mut rows = Vec::new();
        for effect in [None, Some("minecraft:levitation"), Some("minecraft:slow_falling")] {
            for no_ai in [false, true] {
                for vy in [0.0_f64, 0.2, -0.2] {
                    let id = self.ecs_spawn("minecraft:cow", [2.5, 64.0, 2.5], None).ok_or("mob_fall_reset: no cow")?;
                    self.ecs.set_no_ai(id, no_ai);
                    self.ecs.set_no_gravity(id, true);
                    if let Some(name) = effect {
                        self.ecs_add_effect(id, name, 100, 0);
                    }
                    if let Some(e) = self.ecs.entity_of(id) {
                        self.ecs.set_fall_distance(e, 12.0);
                        self.ecs.set_velocity(e, Vec3::new(0.0, vy, 0.0));
                    }
                    self.tick_ecs()?;
                    let fall = self.ecs.fall_distance(id);
                    let name = effect.unwrap_or("none").trim_start_matches("minecraft:");
                    rows.push(json!({"effect": name, "no_ai": no_ai, "velocity_y": format!("{:016x}", vy.to_bits()), "fall_distance": format!("{:016x}", fall.to_bits())}));
                    if let Some(e) = self.ecs.entity_of(id) {
                        self.ecs.despawn(e);
                    }
                }
            }
        }
        Ok(Value::Array(rows))
    }
}
