//! Item frames and glow item frames: `ItemFrameItem` placement on any face,
//! `ItemFrame.survives`, `interact` (put an item in, then rotate it),
//! `hurtServer` (take the item out, then break the frame) and
//! `ItemFrameRenderer` (the frame model and the item at a 45 degree step).
use super::{Game, Hit, InteractTarget};
use horizon_assets::{
    bake::bake_model, item_model::OwnerState, model::CuboidModel, resolve::ModelLibrary,
};
use horizon_client::{entities::ItemFrame, items::ItemStack};
use horizon_core::position::BlockPos;
use horizon_ecs_proto::targeting::Who;
/// A baked frame model: (corners in block units, unit UVs, sprite) per quad.
pub(super) type FrameModel = Vec<([[f32; 3]; 4], [[f32; 2]; 4], String)>;

/// Item-frame variants prepared together for one resource generation.
pub(super) fn bake_frames(
    source: &horizon_assets::resource_source::ResourceSource,
) -> [FrameModel; 4] {
    [
        "item_frame",
        "glow_item_frame",
        "item_frame_map",
        "glow_item_frame_map",
    ]
    .map(|name| bake_frame(source, name))
}

/// Bake `block/<name>` (item_frame / glow_item_frame) from the resources.
pub(super) fn bake_frame(
    root: &horizon_assets::resource_source::ResourceSource,
    name: &str,
) -> FrameModel {
    let mut models = std::collections::BTreeMap::new();
    let mut pending = vec![format!("minecraft:block/{name}")];
    while let Some(model) = pending.pop() {
        let Some((ns, path)) = model.split_once(':') else {
            continue;
        };
        let Ok(text) = root.read_to_string(&format!("assets/{ns}/models/{path}.json")) else {
            continue;
        };
        let Ok(m) = CuboidModel::from_json(&text) else {
            continue;
        };
        if let Some(parent) = &m.parent {
            pending.push(parent.clone());
        }
        models.insert(model, m);
    }
    let Ok(library) = ModelLibrary::new(models) else {
        return Vec::new();
    };
    let Ok(resolved) = library.resolve(&format!("minecraft:block/{name}")) else {
        return Vec::new();
    };
    let Ok(quads) = bake_model(&resolved) else {
        return Vec::new();
    };
    let unit = if quads
        .iter()
        .flat_map(|q| q.positions)
        .any(|p| p.iter().any(|v| *v > 1.5))
    {
        16.0
    } else {
        1.0
    };
    quads
        .into_iter()
        .map(|q| {
            (
                q.positions.map(|p| p.map(|v| v / unit)),
                q.uv,
                q.material
                    .sprite
                    .trim_start_matches("minecraft:")
                    .to_owned(),
            )
        })
        .collect()
}
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
fn center(f: &ItemFrame) -> [f64; 3] {
    let (lo, hi) = f.bounds();
    std::array::from_fn(|i| (lo[i] + hi[i]) / 2.0)
}
/// `HangingEntity.spawnAtLocation`: drops sit 0.15 out from the wall.
pub(super) fn hanging_drop_position(center: [f64; 3], step: [i32; 3]) -> [f64; 3] {
    [
        center[0] + f64::from(step[0]) * 0.15,
        center[1],
        center[2] + f64::from(step[2]) * 0.15,
    ]
}
/// A frame's item, waiting for its model: the frames' borrow of `self` ends
/// before `resolve_item` reads the pack.
struct FramedItem {
    stack: ItemStack,
    id: u32,
    matrix: glam::Mat4,
    light: u8,
    cull: bool,
    kind: &'static str,
    owner: OwnerState,
}
/// `ItemFrameRenderer.extractRenderState`'s `ItemOwner` (`updateForNonLiving`
/// with the frame entity): `ItemFrame.position` and
/// `ItemFrame.getVisualRotationYInDegrees`.
fn frame_owner(f: &ItemFrame) -> OwnerState {
    super::item_draw::owner_at(center(f), visual_rotation_y_degrees(f), f.kind())
}
/// `ItemFrame.getVisualRotationYInDegrees`: the frame's own yaw, as an item
/// model's owner reads it (a framed compass's needle turns with it).
fn visual_rotation_y_degrees(f: &ItemFrame) -> f32 {
    // `Direction.get2DDataValue` of a wall frame, and the axis direction's
    // step for the two flat ones.
    let s = f.step();
    let (two_d, correction) = match (s[0], s[1], s[2]) {
        (0, 0, 1) => (0, 0),
        (-1, 0, 0) => (1, 0),
        (0, 0, -1) => (2, 0),
        (1, 0, 0) => (3, 0),
        (0, y, _) => (-1, 90 * y),
        _ => (0, 0),
    };
    crate::camera::wrap_degrees(
        180.0 + (two_d * 90) as f32 + f32::from(f.rotation) * 45.0 + correction as f32,
    )
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
        if self.native_window() && self.targeting().hit_through_walls {
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
mod source_tests {
    #[test]
    fn frame_model_reads_resolved_resources_without_files() {
        use horizon_assets::resource_source::{Resource, ResourceSource};
        let source = ResourceSource::indexed(std::collections::BTreeMap::from([
            ("assets/minecraft/models/block/item_frame.json".into(), Resource::bytes(br##"{"textures":{"x":"example:block/frame"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"north":{"texture":"#x"}}}]}"##.to_vec())),
        ]));
        let model = super::bake_frame(&source, "item_frame");
        assert_eq!(model.len(), 1);
        assert_eq!(model[0].2, "example:block/frame");
    }
}


#[cfg(test)]
#[path = "tests/lodestone_compass_tests.rs"]
mod lodestone_compass_tests;
