//! Sulfur cubes on the game side (`SulfurCube.mobInteract`, `pickUpItem`):
//! swallowing a held block, shearing it back out, lighting an explosive one,
//! swallowing blocks from the ground. Read and written through the ECS.
use super::Game;
use horizon_client::{
    items::ItemStack,
    mobs::Who,
    text::Text,
};
use horizon_core::{geometry::Vec3, position::BlockPos};
use horizon_items::equipment::BODY;

const SULFUR_CUBE: &str = "minecraft:sulfur_cube";

impl Game<'_> {
    /// The centre of the entity `id` (`Mob.center`).
    fn cube_center(&self, id: u32) -> Option<[f64; 3]> {
        let [x, y, z] = self.ecs.position(id)?;
        Some([x, y + f64::from(self.ecs.height(id)?) / 2.0, z])
    }
    fn sulfur_sound(&mut self, id: u32, sound: &str) {
        let Some(at) = self.cube_center(id) else {
            return;
        };
        self.audio
            .play(&format!("minecraft:{sound}"), Some(at), 1.0, 1.0);
    }
    /// Spit the swallowed block out on top (`EntityAttachment.PASSENGER`).
    fn eject_sulfur_block(&mut self, id: u32) {
        let Some(item) = self.ecs.equipment_stack(id, BODY) else {
            return;
        };
        let (Some([x, y, z]), Some(h)) = (self.ecs.position(id), self.ecs.height(id)) else {
            return;
        };
        self.ecs.put_equipment_slot(id, BODY, None);
        self.spawn_item(item, [x, y + f64::from(h), z], [0.0, 0.2, 0.0], 10);
    }
    /// `SulfurCube.mobInteract`. Returns whether the click was used.
    pub(super) fn interact_sulfur_cube(&mut self, id: u32, item: &str) -> bool {
        if self.ecs.species(id) != Some(SULFUR_CUBE) || !self.ecs.alive(id) {
            return false;
        }
        // RESTE-SANS-ECS: feeding a baby cube (`AgeableMob.ageUp`) needs an age write.
        if self.ecs.age(id).is_some_and(|age| age < 0) {
            return false;
        }
        if matches!(item, "minecraft:flint_and_steel" | "minecraft:fire_charge") {
            if !self
                .session
                .rules
                .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES)
            {
                let message = Text::translate("block.minecraft.tnt.disabled", &[]);
                self.send_overlay_message(message);
                return false;
            }
            if !self.ecs.prime(id, None) {
                return false;
            }
            if item == "minecraft:flint_and_steel" {
                self.damage_held_tool();
            } else {
                self.consume_held();
            }
            self.award_stat("used", item, 1);
            return true;
        }
        if item == "minecraft:shears" && self.ecs.equipment_stack(id, BODY).is_some() {
            // shear: out comes the block, and nothing is swallowed for 5 s.
            // RESTE-SANS-ECS: the 5 s pickup timer (`ints[3]`) has no ECS write.
            let sheared = self.ecs.equipment_stack(id, BODY).map(|s| s.item.clone());
            self.eject_sulfur_block(id);
            self.sulfur_sound(id, "entity.sulfur_cube.eject");
            self.damage_held_tool();
            // gameEvent(SHEAR, player).
            let at = self.ecs.position(id).unwrap_or_default();
            let ctx = self.who_context(Some(Who::Player), None);
            self.post_game_event("minecraft:shear", at, ctx);
            // CriteriaTriggers.PLAYER_SHEARED_EQUIPMENT with the body block.
            if let Some(body) = sheared {
                let target = self.ecs_predicate_view(id);
                self.trigger_item_entity(
                    "minecraft:player_sheared_equipment",
                    &ItemStack::new(&body, 1),
                    &target,
                );
            }
            return true;
        }
        if self.ecs.swallows(id, item) {
            // equipItem: not the same block again; the old one comes out.
            if self.ecs.equipment_stack(id, BODY).is_some_and(|s| &*s.item == item) {
                return false;
            }
            self.eject_sulfur_block(id);
            let held = self.inventory[self.selected]
                .clone()
                .map(|s| ItemStack { count: 1, ..s });
            self.ecs.put_equipment_slot(id, BODY, held);
            self.consume_held();
            self.sulfur_sound(id, "entity.sulfur_cube.absorb");
            // Entity.gameEvent(ENTITY_INTERACT): the cube is the source.
            let at = self.ecs.position(id).unwrap_or_default();
            let ctx = self.who_context(Some(Who::Mob(id)), None);
            self.post_game_event("minecraft:entity_interact", at, ctx);
            return true;
        }
        // RESTE-SANS-ECS: `Bucketable` pickup of a cube (`bucket_components` reads the Mob).
        false
    }
    /// `SulfurCube.primeWhenOnPoweredPosition`: an explosive cube standing in
    /// or next to redstone power lights its fuse (`getBestOwnOrNeighbourSignal`;
    /// no block has an own signal, so the neighbour signal is all there is).
    /// Only the cubes `primable` names can be lit.
    /// RESTE-SANS-ECS: the fuse of `primable` is not passed on; the ECS prime takes its own.
    pub(super) fn sulfur_cube_power_priming(&mut self, primable: &[(u32, i32)]) {
        if !self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES)
        {
            return;
        }
        // Vanilla calls primeWhenOnPoweredPosition from SulfurCube.tick,
        // not from a block-change callback. Frozen descendants never poll power.
        let blocked = self.mobs_below_off_level();
        for id in self.ecs.species_ids(SULFUR_CUBE) {
            if blocked.contains(&id)
                || !self.ecs.alive(id)
                || !primable.iter().any(|(p, _)| *p == id)
            {
                continue;
            }
            let Some([x, y, z]) = self.ecs.position(id) else {
                continue;
            };
            let at = BlockPos::new(x.floor() as i32, y.floor() as i32, z.floor() as i32);
            let signal =
                self.with_level(|b, level, ticks| b.best_neighbor_signal(level, ticks, at));
            if signal != 0 {
                self.ecs.prime(id, None);
            }
        }
    }
    /// `SulfurCubeSearchForItemsGoal` and `pickUpItem`: an adult with nothing
    /// inside hops to a block it can swallow within 8 and takes it in.
    /// RESTE-SANS-ECS: the seek target (`point`) and the pickup timer are not written.
    pub(super) fn sulfur_cube_pickups(&mut self) {
        let griefing = self
            .session
            .rules
            .bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING);
        for id in self.ecs.species_ids(SULFUR_CUBE) {
            if !self.creatures.ticked.contains(&id)
                || !self.ecs.alive(id)
                || self.ecs.age(id).is_some_and(|age| age < 0)
                || self.ecs.equipment_stack(id, BODY).is_some()
            {
                continue;
            }
            let (Some(bounds), Some([x, y, z])) = (self.ecs.bounds(id), self.ecs.position(id))
            else {
                continue;
            };
            let reach = bounds.inflate(Vec3::new(8.0, 8.0, 8.0));
            let pos = Vec3::new(x, y, z);
            let best = self
                .ecs
                .item_views()
                .into_iter()
                .filter(|e| {
                    e.pickup_delay == 0
                        && reach.contains(e.position)
                        && self.ecs.swallows(id, &e.stack.item)
                })
                .map(|e| (e.id, e.position.distance_squared(pos)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((j, _)) = best else {
                continue;
            };
            let Some(e) = self.ecs.item(j) else { continue };
            // Mob.aiStep: items within the box grown by (1, 0, 1).
            let touch = bounds.inflate(Vec3::new(1.0, 0.0, 1.0));
            let close = touch.contains(e.position);
            if !griefing || !close {
                continue;
            }
            let thrown_by_player = e
                .thrower
                .is_some_and(|uuid| Some(uuid) == self.level.player_uuid);
            let mut swallowed = e.stack.clone();
            swallowed.count = 1;
            let (count, at) = (e.stack.count as i32, [e.position.x, e.position.y, e.position.z]);
            self.add_pickup(
                super::pickups::Taken::Ecs { id: j, count, at, kind: "item" },
                super::pickups::Collector::Mob(id),
            );
            let mut rest = e.stack.clone();
            rest.count -= 1;
            if rest.count == 0 {
                self.ecs_remove_item(j);
            } else {
                self.ecs.set_item_stack(j, rest);
            }
            self.ecs.put_equipment_slot(id, BODY, Some(swallowed.clone()));
            self.sulfur_sound(id, "entity.sulfur_cube.absorb");
            if thrown_by_player {
                let target = self.ecs_predicate_view(id);
                self.trigger_item_entity(
                    "minecraft:thrown_item_picked_up_by_entity",
                    &swallowed,
                    &target,
                );
            }
        }
    }
}
