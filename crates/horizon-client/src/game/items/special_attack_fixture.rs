//! Scoped Player.attack follow-up probe; invoked by the dispenser's headless world.
use super::{
    Game,
    boats::Boat,
    dragon::Dragon,
    minecarts::{CartKind, Minecart},
};
use horizon_client::{
    Result,
    entities::ArmorStand,
    item_data::parse_stack,
    motion::PlayerMotion,
};
use horizon_core::geometry::Vec3;
use serde_json::Value;

impl Game<'_> {
    pub(super) fn fixture_special_attack(&mut self, op: &Value) -> Result<Vec<String>> {
        let kind = op["target"]
            .as_str()
            .ok_or("special attack target missing")?;
        let creative = op["creative"].as_bool().unwrap_or(false);
        let mut player = PlayerMotion::new(self.motion, Vec3::new(0.5, 0.0, -2.0))?;
        player.invulnerable = creative;
        player.may_fly = creative;
        player.ground.on_ground = true;
        player.velocity = Vec3::new(0.1, 0.0, 0.1);
        player.fall_distance = op["fall"].as_f64().unwrap_or(0.0);
        if let Some(damage) = op["attack_damage"].as_f64() {
            let key = player
                .attributes
                .key("minecraft:attack_damage")
                .ok_or("attack damage missing")?;
            let handle = player
                .attributes
                .get_instance(Some(key))
                .map_err(|error| format!("attack damage attribute: {error:?}"))?
                .ok_or("attack damage missing")?;
            player
                .attributes
                .edit(&handle, |value, observer| {
                    value.set_base_value(damage, observer)
                })
                .map_err(|error| format!("attack damage update: {error:?}"))?;
        }
        player.set_sprinting(op["sprint"].as_bool().unwrap_or(false))?;
        self.player = Some(player);
        self.mode = if creative {
            horizon_client::command::args::GameMode::Creative
        } else {
            horizon_client::command::args::GameMode::Survival
        };
        self.creative = creative;
        self.inventory.fill(None);
        self.inventory[self.selected] = parse_stack(op["item"].as_str().unwrap_or("-"))?;
        self.offhand = None;
        // Every case installed a fresh player attribute map; the previous
        // actor's applied-effect cache must not suppress this actor's modifiers.
        self.potions = Default::default();
        if op["weakness"].as_bool().unwrap_or(false) {
            self.add_player_effect(horizon_client::effects::EffectInstance::new(
                "weakness", 200, 0,
            ));
        }
        self.attack_ticks = op["ticks"].as_u64().unwrap_or(20) as u32;
        self.world.game_time = 100;
        let at = Vec3::new(0.5, 1.0, 0.5);
        let center = match kind {
            "boat" => {
                self.boats.push(Boat::new(1, "oak".into(), at, 0.0));
                self.boats[0].bounds().center()
            }
            "cart" => {
                self.carts.push(Minecart::new(1, CartKind::Rideable, at));
                self.carts[0].bounds().center()
            }
            "stand" => {
                let mut stand = ArmorStand::new(1, Vec3::new(0.5, 0.0, 0.5), 0.0);
                stand.last_hit = if op["break"].as_bool().unwrap_or(false) {
                    100
                } else {
                    0
                };
                self.armor_stands.push(stand);
                self.armor_stands[0].bounds().center()
            }
            "dragon" => {
                let mut dragon = Dragon::new(Vec3::ZERO, 0.0, 0);
                for part in &mut dragon.parts {
                    part.0 = Vec3::new(100.0, 100.0, 100.0);
                    part.1 = Vec3::ZERO;
                }
                dragon.parts[0].0 = at;
                dragon.parts[0].1 = at;
                self.dragon.replace(dragon);
                self.special_targets()[0].1.center()
            }
            "painting" => {
                let id = self.ecs_with_mw(|ecs, mw| ecs.hang_painting(mw, [0, 1, 0], 2, Some("minecraft:kebab"), None, &mut |_| 0, &mut || 1));
                let p = id.and_then(|id| self.ecs.entity_of(id)).and_then(|e| self.painting_from_ecs(e)).ok_or("special attack painting not hung")?;
                let (lo, hi) = p.bounds();
                Vec3::new(
                    (lo[0] + hi[0]) / 2.0,
                    (lo[1] + hi[1]) / 2.0,
                    (lo[2] + hi[2]) / 2.0,
                )
            }
            "frame" | "fixed_frame" | "filled_frame" => {
                let id = self.ecs_with_mw(|ecs, mw| ecs.hang_frame(mw, [0, 1, 0], 2, false, None, &mut || 1));
                if kind == "filled_frame" {
                    let stack = horizon_client::items::ItemStack::new("minecraft:stone", 1);
                    if let Some(id) = id {
                        self.ecs_with_mw(|ecs, mw| ecs.set_frame_item(mw, id, Some(stack)));
                    }
                }
                let f = id.and_then(|id| self.ecs.entity_of(id)).and_then(|e| self.frame_from_ecs(e)).ok_or("special attack frame not hung")?;
                let (lo, hi) = f.bounds();
                Vec3::new(
                    (lo[0] + hi[0]) / 2.0,
                    (lo[1] + hi[1]) / 2.0,
                    (lo[2] + hi[2]) / 2.0,
                )
            }
            _ => return Err(format!("unknown special attack target {kind}").into()),
        };
        let eye = self.eye_position().ok_or("special attack player missing")?;
        let dx = center.x - eye[0];
        let dy = center.y - eye[1];
        let dz = center.z - eye[2];
        self.yaw = (-dx).atan2(dz).to_degrees() as f32;
        self.pitch = (-dy).atan2((dx * dx + dz * dz).sqrt()).to_degrees() as f32;
        self.attack();
        // A ray miss must not pass as an accepted hit with an unchanged cost.
        let accepted = match kind {
            "boat" => Some(self.boats.first().is_none_or(|b| b.hurt_time > 0)),
            "cart" => Some(self.carts.first().is_none_or(|c| c.hurt_time > 0)),
            "stand" => Some(
                self.armor_stands
                    .first()
                    .is_none_or(|s| !op["break"].as_bool().unwrap_or(false) && s.last_hit == 100),
            ),
            "dragon" => Some(self.dragon.as_ref().is_some_and(|d| d.health < 200.0)),
            _ => None,
        };
        let outcome = accepted.map_or("control", |hit| if hit { "accepted" } else { "unchanged" });
        let player = self.player.take().ok_or("special attack player missing")?;
        Ok(vec![format!(
            "special {outcome} {:08x} {:016x} {:016x} {} {}",
            player.health.food.exhaustion.to_bits(),
            player.velocity.x.to_bits(),
            player.velocity.z.to_bits(),
            player.sprinting,
            self.attack_ticks
        )])
    }
}

#[cfg(test)]
mod tests {
    /// Invoke the actual attack methods in an empty scratch Game, without
    /// starting a server, opening a world or entering the game loop.
    #[test]
    #[ignore = "needs prepared resources and HORIZON_SPECIAL_ATTACK_CASES=INPUT,OUTPUT"]
    fn special_attack_cases_without_world_startup() {
        let spec = std::env::var("HORIZON_SPECIAL_ATTACK_CASES").expect("case paths required");
        let mut resources = None;
        let mut game = crate::game::game_tests::tests::server_game(&mut resources);
        game.build_registries();
        game.dispense_fixture(&spec).unwrap();
    }
}
