//! Brushing suspicious sand and gravel: `BrushItem.onUseTick` and
//! `BrushableBlockEntity`. The block's loot table stays in its saved block
//! entity (like a decorated pot's) until the first counted stroke rolls it.
use super::{Game, Hit, decorated_pot, view_vector};
use horizon_client::{
    containers::BlockEntity,
    particles::{ParticleOptions, ParticleType},
    save,
};
use horizon_core::random::RandomSource;
use std::collections::HashMap;

/// `BrushItem.USE_DURATION`.
const USE_DURATION: u32 = 200;

/// A brushable block's counters. Vanilla never saves them: the block's
/// `dusted` state is all that is left after a reload.
#[derive(Default)]
pub(super) struct BrushState {
    count: i32,
    resets_at: i64,
    cooldown_ends_at: i64,
    hit_face: Option<[i32; 3]>,
}

/// `BrushableBlockEntity.getCompletionState`: the `dusted` property.
fn completion_state(count: i32) -> i32 {
    match count {
        0 => 0,
        1..=2 => 1,
        3..=5 => 2,
        _ => 3,
    }
}

pub(super) type Brushing = HashMap<[i32; 3], BrushState>;

impl Game<'_> {
    /// `BrushItem.onUseTick` for a brush used `ticks` ticks so far, the
    /// hand of `slot` acting.
    pub(super) fn tick_brush(
        &mut self,
        stack: &horizon_client::items::ItemStack,
        slot: usize,
        ticks: u32,
    ) {
        // No block under the crosshair: vanilla calls releaseUsingItem, which
        // stops the use and causes the interaction vibration.
        let Some(hit) = self.pick() else {
            self.stop_using_item(stack);
            return;
        };
        // `BrushItem.onUseTick` aims with `getHitResultOnViewVector` out to the
        // block interaction range, for mobs too: a mob between the player and
        // the block is nearer, so the hit is not a block and the use releases
        // the same way. A guest acts on its client's targeted block, whose view
        // we cannot trace here, so only the local aim runs the entity test.
        if self.forced_hit.is_none()
            && let Some((eye, _)) = self.aim()
            && let Some((_, mob, _)) = self.mob_hit_within(self.block_reach)
        {
            let block = (
                hit.point[0] - eye[0],
                hit.point[1] - eye[1],
                hit.point[2] - eye[2],
            );
            if mob <= (block.0 * block.0 + block.1 * block.1 + block.2 * block.2).sqrt() {
                self.stop_using_item(stack);
                return;
            }
        }
        let elapsed = ticks + 1;
        // The last tick before the backswing.
        if elapsed % 10 == 5 {
            self.brush_stroke(&hit, slot);
        }
        // The use ends after USE_DURATION; stopUsingItem causes the vibration
        // (vanilla's tickUsingItem), the held button starts it again.
        if elapsed < USE_DURATION {
            self.using = Some((slot, elapsed));
        } else {
            self.stop_using_item(stack);
        }
    }

    /// One stroke on the block aimed at: the brush sound, and the block
    /// entity's count. The brush wears when the block changed state.
    fn brush_stroke(&mut self, hit: &Hit, slot: usize) {
        let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        let name = self.res.facts().get(state).name.clone();
        let (sound, brushable) = match &*name {
            "minecraft:suspicious_sand" => ("minecraft:item.brush.brushing.sand", true),
            "minecraft:suspicious_gravel" => ("minecraft:item.brush.brushing.gravel", true),
            _ => ("minecraft:item.brush.brushing.generic", false),
        };
        let at = hit.pos.map(|v| f64::from(v) + 0.5);
        self.audio.play(sound, Some(at), 1.0, 1.0);
        if brushable && self.brush_block(hit.pos, hit.face, slot) {
            self.damage_held(1);
        }
    }

    /// `BrushableBlockEntity.brush`: whether the block changed (the brush wears).
    fn brush_block(&mut self, pos: [i32; 3], face: [i32; 3], slot: usize) -> bool {
        let time = self.world.game_time;
        let mut brush = self.brushing.remove(&pos).unwrap_or_default();
        brush.hit_face.get_or_insert(face);
        let direction = match brush.hit_face.unwrap() {
            [0, -1, 0] => 0,
            [0, 1, 0] => 1,
            [0, 0, -1] => 2,
            [0, 0, 1] => 3,
            [-1, 0, 0] => 4,
            [1, 0, 0] => 5,
            _ => return false,
        };
        if let Some(BlockEntity::Brushable(b)) =
            self.world.block_entities.get_mut(&(pos[0], pos[1], pos[2]))
        {
            b.hit_direction = Some(direction);
        }
        brush.resets_at = time + 40;
        if time < brush.cooldown_ends_at {
            self.brushing.insert(pos, brush);
            return false;
        }
        brush.cooldown_ends_at = time + 10;
        self.unpack_brush_loot(pos);
        let before = completion_state(brush.count);
        brush.count += 1;
        if brush.count >= 10 {
            self.brushing_completed(pos, brush.hit_face, slot);
            return true;
        }
        self.schedule_brushable_tick(pos);
        let after = completion_state(brush.count);
        self.brushing.insert(pos, brush);
        if before != after {
            self.set_dusted(pos, after);
        }
        false
    }

    /// `BrushableBlockEntity.unpackLootTable`: the table's roll becomes the
    /// block entity's item.
    fn unpack_brush_loot(&mut self, pos: [i32; 3]) {
        let key = (pos[0], pos[1], pos[2]);
        let Some(BlockEntity::Brushable(b)) = self.world.block_entities.get(&key) else {
            return;
        };
        let Some(horizon_core::nbt::Tag::String(table)) = &b.loot_table else {
            return;
        };
        let table = table.to_string_lossy();
        let mut tag = save::block_entity_encode::encode(
            key,
            &BlockEntity::Brushable(b.clone()),
            "minecraft:brushable_block",
        );
        self.trigger_loot_table(&table);
        let loot = self.server_loot.get();
        decorated_pot::unpack_loot(
            &mut tag,
            loot.as_deref(),
            &mut self.world.random,
            &mut self.rand_sequences,
            self.level.seed,
        );
        if let Some(BlockEntity::Brushable(b)) = self.world.block_entities.get_mut(&key) {
            b.loot_table = tag.get("LootTable").cloned();
            b.table_seed_tag = tag.get("LootTableSeed").cloned();
            b.item = tag.get("item").and_then(save::item_from_nbt);
        }
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }

    /// `BrushableBlockEntity.brushingCompleted`: the item drops out of the
    /// brushed face and the block turns into its sand or gravel.
    fn brushing_completed(&mut self, pos: [i32; 3], face: Option<[i32; 3]>, slot: usize) {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let turns_into = match &*self.res.facts().get(state).name {
            "minecraft:suspicious_sand" => "minecraft:sand",
            "minecraft:suspicious_gravel" => "minecraft:gravel",
            _ => "minecraft:air",
        };
        // dropContent: the item at the middle of the block beside that face.
        let item = match self.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
            Some(BlockEntity::Brushable(b)) => b.item.clone(),
            _ => None,
        };
        if let Some(mut item) = item {
            let face = face.unwrap_or([0, 1, 0]);
            let take = 10 + self.world.random.next_int_bound(21).unwrap_or(0);
            item.count = item.count.min(take as u32);
            let at = [
                f64::from(pos[0] + face[0]) + 0.5,
                f64::from(pos[1] + face[1]) + 0.625,
                f64::from(pos[2] + face[2]) + 0.5,
            ];
            let mut drops = vec![item];
            let tool = self.stack_in(slot).cloned();
            let origin = pos.map(|c| f64::from(c) + 0.5);
            let player = Some(self.native_acting_id());
            self.native_use_drops(
                "",
                origin,
                Some((pos, state)),
                None,
                player,
                tool.as_ref(),
                &mut drops,
            );
            for item in drops {
                self.spawn_item(item, at, [0.0; 3], 0);
            }
        }
        // Level event 3008 for every player, then the block change.
        self.level_event_at(3008, pos, state.0 as i32);
        if let Some(base) = self.pack.assets.registry.default_state(turns_into) {
            let _ = self.level_set(pos, base);
        }
    }

    fn set_dusted(&mut self, pos: [i32; 3], dusted: i32) {
        let registry = self.pack.assets.registry;
        let Some(mut state) = registry
            .state(self.world.block(pos[0], pos[1], pos[2]))
            .cloned()
        else {
            return;
        };
        state.properties.insert("dusted".into(), dusted.to_string());
        if let Ok(state) = registry.resolve(&state) {
            let _ = self.level_set(pos, state);
        }
    }

    fn schedule_brushable_tick(&mut self, pos: [i32; 3]) {
        let at = horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.schedule_tick(level, ticks, at, 2));
    }

    /// `BrushableBlockEntity.checkReset`, from `BrushableBlock.tick`: a block
    /// left alone loses two counts, again every four ticks.
    pub(super) fn brushable_tick(&mut self, pos: [i32; 3]) {
        let time = self.world.game_time;
        let Some(mut brush) = self.brushing.remove(&pos) else {
            return;
        };
        let before = completion_state(brush.count);
        if brush.count != 0 && time >= brush.resets_at {
            brush.count = (brush.count - 2).max(0);
            brush.resets_at = time + 4;
        }
        let after = completion_state(brush.count);
        if brush.count != 0 {
            self.brushing.insert(pos, brush);
            self.schedule_brushable_tick(pos);
        } else if let Some(BlockEntity::Brushable(b)) =
            self.world.block_entities.get_mut(&(pos[0], pos[1], pos[2]))
        {
            b.hit_direction = None;
        }
        if before != after {
            self.set_dusted(pos, after);
        }
    }

}

impl Game<'_> {
    pub(super) fn brush_complete_event(&mut self, pos: [i32; 3], data: i32) {
        let state = horizon_world::registry::BlockStateId(data as u32);
        let sound = match &*self.res.facts().get(state).name {
            "minecraft:suspicious_sand" => "minecraft:item.brush.brushing.sand.complete",
            "minecraft:suspicious_gravel" => "minecraft:item.brush.brushing.gravel.complete",
            _ => "",
        };
        if !sound.is_empty() {
            let at = pos.map(|v| f64::from(v) + 0.5);
            // Each client plays it from the event itself.
            self.audio.play(sound, Some(at), 1.0, 1.0);
        }
    }
}
