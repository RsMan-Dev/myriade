//! Item frames and glow item frames: `ItemFrameItem` placement on any face,
//! `ItemFrame.survives`, `interact` (put an item in, then rotate it),
//! `hurtServer` (take the item out, then break the frame) and
//! `ItemFrameRenderer` (the frame model and the item at a 45 degree step).
use super::{Game, Hit, InteractTarget};
use horizon_client::items::ItemStack;
use horizon_core::position::BlockPos;
use horizon_ecs_proto::targeting::Who;
pub(in crate::game) fn facing_of(face: [i32; 3]) -> u8 {
    match face {
        [0, -1, 0] => 0,
        [0, 1, 0] => 1,
        [0, 0, -1] => 2,
        [0, 0, 1] => 3,
        [-1, 0, 0] => 4,
        _ => 5,
    }
}
/// `HangingEntity.spawnAtLocation`: drops sit 0.15 out from the wall.
pub(super) fn hanging_drop_position(center: [f64; 3], step: [i32; 3]) -> [f64; 3] {
    [
        center[0] + f64::from(step[0]) * 0.15,
        center[1],
        center[2] + f64::from(step[2]) * 0.15,
    ]
}
impl Game<'_> {
    /// Asks the mods whether the acting guest may place a hanging entity
    /// of type `kind` at `pos` (`native_hanging_place`): true when one
    /// cancelled it. A game's own player is not asked.
    pub(super) fn hanging_place_cancelled(&mut self, kind: &str, pos: [i32; 3]) -> bool {
        let (Some(uuid), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) else {
            return false;
        };
        let mut cancel = false;
        self.native_hanging_place(uuid, &name, kind, pos, &mut cancel);
        cancel
    }
    pub(super) fn block_distance(&self) -> Option<f64> {
        if false && self.targeting().hit_through_walls {
            return None;
        }
        self.pick_unfiltered().map(|h| {
            let eye = self.aim().map(|(eye, _)| eye).unwrap_or_default();
            (0..3)
                .map(|a| (h.point[a] - eye[a]).powi(2))
                .sum::<f64>()
                .sqrt()
        })
    }
    /// Nearer than the targeted block?
    pub(super) fn before_block(&self, distance: f64) -> bool {
        self.block_distance().is_none_or(|block| block > distance)
    }
    /// `ItemFrameItem.useOn`: P hangs the frame and its `Placed` event follows (`game/ecs/frames.rs`); CONSUME when the wall does not hold.
    pub(super) fn place_item_frame(&mut self, hit: &Hit, stack: &ItemStack) -> bool {
        let glow = match &*stack.item {
            "minecraft:item_frame" => false,
            "minecraft:glow_item_frame" => true,
            _ => return false,
        };
        let pos: [i32; 3] = std::array::from_fn(|i| hit.pos[i] + hit.face[i]);
        // `ItemFrameItem.mayPlace`: `Level.isInsideBuildHeight(blockPos)`, so a frame is not
        // placed on the top face of the highest block or under the lowest one.
        if !(horizon_client::chunk::MIN_Y..horizon_client::chunk::MIN_Y + horizon_client::chunk::HEIGHT).contains(&pos[1]) {
            self.use_failed = true;
            return true;
        }
        let kind = if glow { "minecraft:glow_item_frame" } else { "minecraft:item_frame" };
        if self.hanging_place_cancelled(kind, pos) {
            return true;
        }
        let face = facing_of(hit.face);
        let me = Who::Player(self.own_entity_id() as u32);
        let mut next = self.next_entity;
        let hung = self.ecs_with(|ecs, mw| {
            ecs.hang_frame(mw, pos, face, glow, Some(me), &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        if hung.is_none() {
            // HangingEntity.survives false: CONSUME, which ItemStack.useOn still counts as a use.
            self.award_stat("used", &stack.item, 1);
            return true;
        }
        if !self.creative {
            self.consume_held();
        }
        self.award_stat("used", &stack.item, 1);
        true
    }
    /// The frame the player aims at within reach: its id and distance (P's ray).
    pub(super) fn aimed_frame(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        self.ecs.aimed_frame(eye, dir, self.entity_reach)
    }
    /// `ItemFrame.interact` for the frame under the crosshair, or the one a guest's interact packet named: P puts the held item in an
    /// empty frame (`Put`) or turns a filled one (`Rotated`).
    pub(super) fn interact_frame(&mut self, target: Option<InteractTarget>) -> bool {
        let Some((id, d)) = (match target {
            Some(t) => Some((t.id as u32, 0.0)),
            None => self.aimed_frame(),
        }) else {
            return false;
        };
        // Only a client's own pick is stopped by a block: the range check passed.
        if target.is_none() && !self.before_block(d) {
            return false;
        }
        let stack = self.held().cloned();
        // Put of a map that tracks more than 256 markers, into an empty frame: refused before the bridge.
        let empty = self.ecs.entity_of(id).and_then(|e| self.ecs.entity_nbt(e)).is_some_and(|c| c.get(&horizon_core::nbt::JavaString::from("Item")).is_none());
        let over = stack.as_ref().and_then(|s| s.data()).and_then(|d| d.map_id).is_some_and(|m| self.with_map(m, |md| md.tracked_over_limit(256)).unwrap_or(false));
        if empty && over {
            return false;
        }
        let me = Who::Player(self.own_entity_id() as u32);
        let used = self.ecs_with(|ecs, mw| ecs.frame_use(mw, id, me, stack));
        if used {
            self.swing_arm();
        }
        used
    }
    /// `ItemFrame.hurtServer` from the player: P takes the item out first, then breaks the frame.
    pub(super) fn attack_frame(&mut self) -> bool {
        let Some((id, d)) = self.aimed_frame() else {
            return false;
        };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        // BlockAttachedEntity.skipAttackInteraction: a player may not hit it where `mayInteract` refuses.
        if let Some([x, y, z]) = self.ecs_hanging_pos(id) && !self.may_interact_at(BlockPos::new(x, y, z)) {
            return true;
        }
        let me = Who::Player(self.own_entity_id() as u32);
        let creative = self.creative;
        if !self.ecs_with(|ecs, mw| ecs.frame_hit(mw, id, me, creative)) && !creative {
            // A refused hit (a fixed frame): Player.attack goes on to onAttack, so the attack cooldown is not spent.
            self.attack_ticks = 0;
        }
        true
    }
    /// `ItemFrame.setItem` / `setRotation`: `updateNeighbourForOutputSignal`, so
    /// the comparators reading the frame take its new signal.
    pub(in crate::game) fn frame_output_changed(&mut self, pos: [i32; 3]) {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.update_neighbour_for_output_signal(level, ticks, at));
    }
}

#[cfg(test)]
#[path = "tests/lodestone_compass_tests.rs"]
mod lodestone_compass_tests;
