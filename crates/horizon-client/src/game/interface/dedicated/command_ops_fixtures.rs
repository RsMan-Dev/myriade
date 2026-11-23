//! The operations of the dedicated server's harness that the parity tools name: one arm per tool, with the cases of its Java oracle.
//! `op_fixture` answers `None` for an unknown name, so `command_ops.rs` keeps the other operations.
use crate::game::Game;
use bevy_ecs::entity::Entity;
use horizon_client::{Result, items::ItemStack};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::Frozen;
use horizon_ecs_proto::fishing_hook::{self, Hook, deflect_hit};
use horizon_ecs_proto::raids::Celebrating;
use horizon_ecs_proto::targeting::Who;
use serde_json::{Value, json};

impl Game<'_> {
    /// `op <player> <json>` for the tools' operations: the record of a known one, None for an unknown name.
    pub(super) fn op_fixture(&mut self, name: &str, op: &Value) -> Result<Option<Value>> {
        Ok(Some(match name {
            "mount_control" => self.mount_control()?,
            "raider_celebration" => self.raider_celebration(op)?,
            "mob_air" => self.mob_air(op)?,
            "breeze_hook" => self.breeze_hook(op)?,
            _ => return Ok(None),
        }))
    }

    /// The 32 cases of `VanillaCommandExecOracle.writeMountControlReference`: a pig or strider, saddled or not, the player first or
    /// second, the stick in no hand, the wrong one, or the right one in either hand. `controlled` is `player_steers`, and
    /// `server_simulates` is its complement (the server does not move a mount its rider's client moves).
    fn mount_control(&mut self) -> Result<Value> {
        let player = self.player.as_ref().map(|p| p.entity_id as u32).ok_or("mount_control: no player")?;
        let mut rows = Vec::new();
        for kind in ["pig", "strider"] {
            for saddled in [false, true] {
                for first in [false, true] {
                    for hand in ["none", "wrong", "main", "off"] {
                        let mount = self.ecs_spawn(&format!("minecraft:{kind}"), [0.0; 3], None).ok_or("mount_control: no mount")?;
                        let other = self.ecs_spawn("minecraft:zombie", [0.0; 3], None).ok_or("mount_control: no other rider")?;
                        self.ecs.mount_store(mount, saddled.then(|| ItemStack::new("minecraft:saddle", 1)), None, Vec::new());
                        let (mount_at, other_at) = (self.ecs.entity_of(mount), self.ecs.entity_of(other));
                        let (Some(mount_at), Some(other_at)) = (mount_at, other_at) else { return Err("mount_control: no entity".into()) };
                        if !first {
                            self.ecs.ride(mount_at, Who::Entity(other_at));
                        }
                        self.ecs.ride(mount_at, Who::Player(player));
                        if matches!(hand, "main" | "off") {
                            self.ecs.ecs_mut().map(|w| w.pilot_input(mount_at, Who::Player(player), 0.0, 0.0, 0.0, 0.0, 0.0, false, false, true));
                        }
                        let controlled = self.ecs.ecs().is_some_and(|w| w.player_steers(mount_at));
                        rows.push(json!({
                            "kind": kind, "saddled": saddled, "first": first, "hand": hand,
                            "controlled": controlled, "server_simulates": !controlled,
                            "forward": 1.0,
                        }));
                        for (id, e) in [(mount, mount_at), (other, other_at)] {
                            if self.ecs.entity_of(id).is_some() {
                                self.ecs.despawn(e);
                            }
                        }
                    }
                }
            }
        }
        Ok(Value::Array(rows))
    }

    /// `VanillaRaiderCelebrationOracle`: each case is a raider kind then its `setCelebrating` bits. The metadata sent is the flag when it
    /// changes from the last value the client holds (none, so not celebrating, before the first bit); the record is `sent:celebrating`.
    fn raider_celebration(&mut self, op: &Value) -> Result<Value> {
        let cases = op["cases"].as_array().ok_or("raider_celebration: missing cases")?;
        let mut out = Vec::new();
        for case in cases {
            let line = case.as_str().ok_or("raider_celebration: a case is a line")?;
            let mut parts = line.split(' ');
            let kind = parts.next().ok_or("raider_celebration: no kind")?;
            let id = self.ecs_spawn(&format!("minecraft:{kind}"), [0.0; 3], None).ok_or("raider_celebration: no raider")?;
            let mut before = false;
            let mut trace = Vec::new();
            for bit in parts {
                let e = self.ecs.entity_of(id).ok_or("raider_celebration: no entity")?;
                let world = self.ecs.ecs_mut().ok_or("raider_celebration: no ECS")?.world_mut();
                if bit == "1" {
                    world.entity_mut(e).insert(Celebrating);
                } else {
                    world.entity_mut(e).remove::<Celebrating>();
                }
                let now = world.get::<Celebrating>(e).is_some();
                let sent = if now == before { "-".to_string() } else { u8::from(now).to_string() };
                trace.push(format!("{sent}:{}", u8::from(now)));
                before = now;
            }
            out.push(json!(trace.join(" ")));
            if let Some(e) = self.ecs.entity_of(id) {
                self.ecs.despawn(e);
            }
        }
        Ok(Value::Array(out))
    }

    /// `VanillaMobAirOracle`: each case is a kind then `air,frozen` steps. A value is sent when it differs from the one the client
    /// holds (the spawn's air, then 0 frozen); the record is the sent entries (`1=air`, `7=frozen`, or `-`) and `air,frozen`.
    fn mob_air(&mut self, op: &Value) -> Result<Value> {
        let cases = op["cases"].as_array().ok_or("mob_air: missing cases")?;
        let mut out = Vec::new();
        for case in cases {
            let line = case.as_str().ok_or("mob_air: a case is a line")?;
            let mut parts = line.split(' ');
            let kind = parts.next().ok_or("mob_air: no kind")?;
            let id = self.ecs_spawn(&format!("minecraft:{kind}"), [0.0; 3], None).ok_or("mob_air: no mob")?;
            let e = self.ecs.entity_of(id).ok_or("mob_air: no entity")?;
            let mut held = (self.ecs.ecs().and_then(|w| w.air(e)).ok_or("mob_air: no air")?, 0);
            let mut trace = Vec::new();
            for step in parts {
                let (air, frozen) = step.split_once(',').ok_or("mob_air: a step is air,frozen")?;
                let (air, frozen) = (air.parse::<i32>()?, frozen.parse::<i32>()?);
                let ecs = self.ecs.ecs_mut().ok_or("mob_air: no ECS")?;
                ecs.set_air(e, air);
                ecs.world_mut().entity_mut(e).insert(Frozen(frozen));
                let now = (ecs.air(e).unwrap_or(0), ecs.world().get::<Frozen>(e).map_or(0, |f| f.0));
                let mut sent = Vec::new();
                if now.0 != held.0 {
                    sent.push(format!("1={}", now.0));
                }
                if now.1 != held.1 {
                    sent.push(format!("7={}", now.1));
                }
                let entries = if sent.is_empty() { "-".to_string() } else { sent.join(",") };
                trace.push(format!("{entries}:{},{}", now.0, now.1));
                held = now;
            }
            out.push(json!(trace.join(" ")));
            if let Some(e) = self.ecs.entity_of(id) {
                self.ecs.despawn(e);
            }
        }
        Ok(Value::Array(out))
    }

    /// `VanillaBreezeHookOracle`: each case is a seed, the hook's velocity, then the breezes (0 or 1) it hits in turn. Per hit: the
    /// velocity bits, the sounds, the lands, `needsSync` (a deflection sets it) and the attachment (none, a breeze never hooks).
    fn breeze_hook(&mut self, op: &Value) -> Result<Value> {
        let cases = op["cases"].as_array().ok_or("breeze_hook: missing cases")?;
        let owner = self.player.as_ref().map(|p| p.entity_id as u32).ok_or("breeze_hook: no player")?;
        let mut ids = Vec::new();
        for x in 0..2 {
            ids.push(self.ecs_spawn("minecraft:breeze", [f64::from(x), 64.0, 0.0], None).ok_or("breeze_hook: no breeze")?);
        }
        let breezes: Vec<Entity> = ids.iter().filter_map(|&id| self.ecs.entity_of(id)).collect();
        let mut out = Vec::new();
        for case in cases {
            let line = case.as_str().ok_or("breeze_hook: a case is a line")?;
            let parts: Vec<&str> = line.split(' ').collect();
            let seed: i64 = parts[0].parse()?;
            let velocity = Vec3::new(parts[1].parse()?, parts[2].parse()?, parts[3].parse()?);
            let w = self.ecs.ecs_mut().ok_or("breeze_hook: no ECS")?.world_mut();
            let hook = fishing_hook::cast(w, owner, Vec3::ZERO, velocity, 0, 0, seed);
            let mut h = w.entity_mut(hook).take::<Hook>().ok_or("breeze_hook: no hook")?;
            let mut vel = velocity;
            let mut trace = Vec::new();
            for step in &parts[4..] {
                let target = *breezes.get(step.parse::<usize>()?).ok_or("breeze_hook: no such breeze")?;
                let (turned, play) = deflect_hit(&mut h, target, vel);
                vel = turned;
                trace.push(format!("{:016x},{:016x},{:016x}:{}:0:1:0", vel.x.to_bits(), vel.y.to_bits(), vel.z.to_bits(), u8::from(play)));
            }
            w.despawn(hook);
            out.push(json!(trace.join(" ")));
        }
        for id in ids {
            if let Some(e) = self.ecs.entity_of(id) {
                self.ecs.despawn(e);
            }
        }
        Ok(Value::Array(out))
    }
}
