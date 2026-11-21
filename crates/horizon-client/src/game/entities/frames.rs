//! Item frames and glow item frames: `ItemFrameItem` placement on any face,
//! `ItemFrame.survives`, `interact` (put an item in, then rotate it),
//! `hurtServer` (take the item out, then break the frame) and
//! `ItemFrameRenderer` (the frame model and the item at a 45 degree step).
use super::{Game, Hit, InteractTarget};
use horizon_assets::{
    bake::bake_model, item_model::OwnerState, model::CuboidModel, resolve::ModelLibrary,
};
use horizon_client::{entities::ItemFrame, items::ItemStack, mesher::Vertex};
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
        let mut next = self.next_entity;
        let hung = self.ecs_with(|ecs, mw| {
            ecs.hang_frame(mw, pos, face, glow, Some(Who::Player), &mut || {
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
        let used = self.ecs_with(|ecs, mw| ecs.frame_use(mw, id, Who::Player, stack));
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
        let creative = self.creative;
        if !self.ecs_with(|ecs, mw| ecs.frame_hit(mw, id, Who::Player, creative)) && !creative {
            // A refused hit (a fixed frame): Player.attack goes on to onAttack, so the attack cooldown is not spent.
            self.attack_ticks = 0;
        }
        true
    }
    /// `ItemFrame.hurtServer` from a projectile, by `who`: P decides the refusal and the break. False when the frame refused the hit.
    pub(super) fn projectile_hurts_frame(&mut self, id: u32, who: Who, creative: bool) -> bool {
        self.ecs_with(|ecs, mw| ecs.frame_hit(mw, id, who, creative))
    }
    /// `ItemFrame.setItem` / `setRotation`: `updateNeighbourForOutputSignal`, so
    /// the comparators reading the frame take its new signal.
    pub(in crate::game) fn frame_output_changed(&mut self, pos: [i32; 3]) {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.update_neighbour_for_output_signal(level, ticks, at));
    }
    /// ItemFrameRenderer: the frame model, then the item half size on its face.
    pub(super) fn frame_geometry(&mut self, camera: glam::DVec3, out: &mut Vec<Vertex>) {
        let sprites = &self.gui.atlas.sprites;
        // Each with its frame's type: a pack tells a glow frame and what it holds apart by its
        // `entityId` (`glow_item_frame` in entity.properties), the item drawn within its frame.
        let mut maps: Vec<(i32, glam::Mat4, u32, bool, &'static str)> = Vec::new();
        let mut items: Vec<FramedItem> = Vec::new();
        // ItemFrame.shouldRenderAtSqrDistance: 1024 blocks at a view scale
        // of 1.
        let range = 16.0 * 64.0 * self.view_scale();
        for f in &self.item_frames {
            // LevelExtractor: the camera entity is not drawn from its own eyes.
            if self.hides_as_camera(f.id) {
                continue;
            }
            let entity_start = out.len();
            self.mark_entity(entity_start, f.kind());
            let map = f.item.as_ref().and_then(super::maps::map_id);
            // EntityRenderer.shouldRender: its box (a block wide with a map),
            // measured from the box's centre.
            let (lo, hi) = f.bounds();
            let (mut lo, mut hi) = (glam::DVec3::from(lo), glam::DVec3::from(hi));
            if map.is_some() {
                let s = f.step();
                for i in 0..3 {
                    if s[i] == 0 {
                        let c = (lo[i] + hi[i]) / 2.0;
                        (lo[i], hi[i]) = (c - 0.5, c + 0.5);
                    }
                }
            }
            let Some(sight) = self.entity_sight(camera, (lo + hi) / 2.0, range, lo, hi) else {
                continue;
            };
            let culling = self.sighted_decision(sight, || self.entity_cull_decision(f, 1.0));
            if culling.hides_entity() {
                continue;
            }
            let entity_start = out.len();
            self.mark_motion(entity_start, Self::MOTION_OBJECT | u64::from(f.id));
            let s = f.step();
            // Pose: to the block centre, then X/Y rotation by facing.
            let (x_rot, y_rot) = if s[1] == 0 {
                let to_y = match f.facing {
                    3 => 0.0,
                    4 => 90.0,
                    2 => 180.0,
                    _ => 270.0,
                };
                (0.0f64, 180.0f64 - to_y)
            } else {
                (-90.0 * f64::from(s[1]), 180.0)
            };
            let pose = glam::DMat4::from_translation(
                glam::DVec3::new(
                    f64::from(f.pos[0]) + 0.5,
                    f64::from(f.pos[1]) + 0.5,
                    f64::from(f.pos[2]) + 0.5,
                ) - camera,
            ) * glam::DMat4::from_rotation_x(x_rot.to_radians())
                * glam::DMat4::from_rotation_y(y_rot.to_radians());
            let light = self.world.light(f.pos[0], f.pos[1], f.pos[2]);
            let packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
            // ItemFrameRenderer.getBlockLightLevel: a glow frame's model is at least 5 bright.
            let frame_light = if f.glow {
                (light & 0xF0) | (light & 15).max(5)
            } else {
                light
            };
            let frame_packed =
                (u32::from(frame_light >> 4) * 16) | ((u32::from(frame_light & 15) * 16) << 8);
            let frame_start = out.len();
            let mut quad =
                |m: &glam::DMat4, corners: [[f64; 3]; 4], uv: [[f32; 2]; 4], light: u32| {
                    // The frame model is drawn with the entity shader, lit by its face normal.
                    let at = corners.map(|c| m.transform_point3(glam::DVec3::from_array(c)));
                    let n = (at[1] - at[0]).cross(at[2] - at[0]).normalize_or_zero();
                    let normal = [0, 1, 2].map(|k| (n[k].clamp(-1.0, 1.0) * 127.0) as i8);
                    for (p, uv) in at.into_iter().zip(uv) {
                        out.push(Vertex {
                            normal: [normal[0], normal[1], normal[2], -127],
                            pos: p.as_vec3().to_array(),
                            uv,
                            layer: 0,
                            color: 0xFFFF_FFFF,
                            light,

                            glint_uv: [0.0, 0.0],
                        });
                    }
                };
            let map_uv = |r: &horizon_client::hud::Rect, uv: [[f32; 2]; 4]| {
                uv.map(|[u, v]| [r.u0 + u * (r.u1 - r.u0), r.v0 + v * (r.v1 - r.v0)])
            };
            if !f.invisible {
                let model = match (f.glow, map.is_some()) {
                    (true, true) => &self.glow_map_frame_model,
                    (false, true) => &self.map_frame_model,
                    (true, false) => &self.glow_frame_model,
                    (false, false) => &self.frame_model,
                };
                for (corners, uv, sprite) in model {
                    let Some(r) = sprites.get(sprite) else {
                        continue;
                    };
                    quad(
                        &pose,
                        corners.map(|c| {
                            [
                                f64::from(c[0]) - 0.5,
                                f64::from(c[1]) - 0.5,
                                f64::from(c[2]) - 0.5,
                            ]
                        }),
                        map_uv(r, *uv),
                        frame_packed,
                    );
                }
            }
            // HDR output: a glow frame shines softly (entity.wgsl hdr_entity_mark); the item in it
            // is only lit, as vanilla draws it.
            if f.glow {
                for v in &mut out[frame_start..] {
                    v.layer |= u32::from(horizon_client::mesher::INCANDESCENT) << 16;
                }
            }
            self.finish_lit_entity(culling, f, out, entity_start);
            // The frame; what it shows is an item or a map, Horizon's to light: not the layer's.
            self.lit_entity(entity_start..out.len());
            if culling.rays_only() {
                continue;
            }
            let cull_contents = culling.checks_vertices();
            if let Some(stack) = &f.item {
                let mut canceled = false;
                self.native_render_item_in_frame(f.id, f.pos, stack, &mut canceled);
                if canceled {
                    continue;
                }
            }
            if let Some(id) = map {
                // ItemFrameRenderer: the map fills the face, in 90-degree steps.
                let depth = if f.invisible { 0.5 } else { 0.4375 };
                // A glow frame's map is 15728850 = 0xF000D2 (sky coordinate 240,
                // block coordinate 210), not full bright.
                let item_light = if f.glow { 240 | 210 << 8 } else { packed };
                let m = pose
                    * glam::DMat4::from_translation(glam::DVec3::new(0.0, 0.0, depth))
                    * glam::DMat4::from_rotation_z(
                        (f64::from(f.rotation % 4 * 2) * 45.0).to_radians(),
                    )
                    * glam::DMat4::from_rotation_z(std::f64::consts::PI)
                    * glam::DMat4::from_scale(glam::DVec3::splat(0.0078125))
                    * glam::DMat4::from_translation(glam::DVec3::new(-64.0, -64.0, -1.0));
                maps.push((id, m.as_mat4(), item_light, cull_contents, f.kind()));
                continue;
            }
            let Some(stack) = f.item.clone() else {
                continue;
            };
            // `GlowItemFrame` items render full bright (15728880).
            let item_light = if f.glow { 0xFF } else { light };
            let depth = if f.invisible { 0.5 } else { 0.4375 };
            let m = pose
                * glam::DMat4::from_translation(glam::DVec3::new(0.0, 0.0, depth))
                * glam::DMat4::from_rotation_z((f64::from(f.rotation) * 45.0).to_radians())
                * glam::DMat4::from_scale(glam::DVec3::splat(0.5));
            items.push(FramedItem {
                stack,
                id: f.id,
                matrix: m.as_mat4(),
                light: item_light,
                cull: cull_contents,
                kind: f.kind(),
                // `ItemFrameRenderer.extractRenderState` hands the frame entity
                // itself to the item model (`updateForNonLiving`), so a framed
                // compass's needle reads the frame's position and yaw.
                owner: frame_owner(f),
            });
        }
        // `extractRenderState`: FIXED, seeded by the frame entity.
        let skinning = self.skinning();
        let lights = horizon_client::block_entity_render::diffuse_lights(self.nether_lighting());
        for item in items {
            let entity_start = out.len();
            self.mark_entity(entity_start, item.kind);
            self.mark_motion(entity_start, Self::MOTION_FRAME_ITEM | u64::from(item.id));
            let state = self.resolve_item(
                &item.stack,
                horizon_assets::item_model::DisplayContext::Fixed,
                Some(&item.owner),
                item.id as i32,
            );
            let light = horizon_client::block_entity_render::vertex_light(item.light);
            // Posed on the GPU when every layer can be.
            let skinned = skinning
                .then(|| self.submit_item_skinned(&state, &[item.matrix], light, lights))
                .flatten();
            if skinned.is_none() {
                self.submit_item_world(&state, item.matrix, light, out);
            }
            if item.cull {
                self.cull_skinned(out, entity_start, skinned);
            }
        }
        // Maps: keyed by their map id, which several frames can show, so they stay unmarked.
        self.mark_motion(out.len(), 0);
        // Nearest first: past the atlas's map slots, the far ones go without.
        maps.sort_by(|a, b| {
            let d = |m: &glam::Mat4| {
                m.transform_point3(glam::Vec3::new(64.0, 64.0, 0.0))
                    .length_squared()
            };
            d(&a.1).total_cmp(&d(&b.1))
        });
        for (id, m, light, cull, kind) in maps {
            let entity_start = out.len();
            self.mark_entity(entity_start, kind);
            // Its outline first: a map out of sight takes no slot in the atlas, or with
            // more maps around than slots, the ones in view lost theirs to it.
            if cull {
                for c in [[0.0, 0.0], [128.0, 0.0], [128.0, 128.0], [0.0, 128.0]] {
                    out.push(Vertex {
                        pos: m
                            .transform_point3(glam::Vec3::new(c[0], c[1], 0.0))
                            .to_array(),
                        ..bytemuck::Zeroable::zeroed()
                    });
                }
                self.cull_entity_vertices(out, entity_start);
                if out.len() == entity_start {
                    continue;
                }
                out.truncate(entity_start);
            }
            self.map_vertices(id, true, m, light, out);
            if cull {
                self.cull_entity_vertices(out, entity_start);
            }
        }
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
