//! Paintings: `HangingEntityItem.useOn` placement with `Painting.create`'s
//! variant choice (or the stack's `painting/variant`), `HangingEntity.survives`
//! checks every 100 ticks, breaking by attack (`dropItem`), saving, and
//! `PaintingRenderer` quads.
use horizon_core::random::RandomSource as _;
use super::{Game, Hit, entity_culling::default_range};
use horizon_client::{items::ItemStack, mesher::Vertex, random_source};
use horizon_ecs_proto::targeting::Who;
use std::sync::Arc;

/// A `painting_variant` and whether `#placeable` lists it (in tag order).
#[derive(Clone, Debug)]
pub(super) struct Variant {
    pub id: Arc<str>,
    pub asset: String,
    pub width: i32,
    pub height: i32,
    /// `PaintingVariant.title` and `author`: text components.
    pub title: Option<serde_json::Value>,
    pub author: Option<serde_json::Value>,
}
/// Variants by id plus the `#minecraft:placeable` list.
#[derive(Default)]
pub(super) struct PaintingVariants {
    pub all: Vec<Variant>,
    pub placeable: Vec<Arc<str>>,
}
impl PaintingVariants {
    pub fn load(root: &std::path::Path) -> Self {
        let data = root.join("data/minecraft");
        let mut all = Vec::new();
        if let Ok(folder) = horizon_world::json_folder::read(&data.join("painting_variant")) {
            for file in &folder.files {
                let name = file.name.as_str();
                let Ok(v) =
                    serde_json::from_slice::<serde_json::Value>(file.bytes().unwrap_or_default())
                else {
                    continue;
                };
                all.push(Variant {
                    id: Arc::from(format!("minecraft:{name}")),
                    asset: v["asset_id"]
                        .as_str()
                        .unwrap_or(name)
                        .trim_start_matches("minecraft:")
                        .to_owned(),
                    width: v["width"].as_i64().unwrap_or(1) as i32,
                    height: v["height"].as_i64().unwrap_or(1) as i32,
                    title: v.get("title").cloned(),
                    author: v.get("author").cloned(),
                });
            }
        }
        let placeable =
            horizon_core::prepared_files::read(&data.join("tags/painting_variant/placeable.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .and_then(|v| v["values"].as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(Arc::from))
                .collect();
        Self { all, placeable }
    }
    pub fn get(&self, id: &str) -> Option<&Variant> {
        self.all.iter().find(|v| &*v.id == id)
    }
    /// The variant a stack's `painting/variant` component names.
    pub fn of_stack(&self, stack: &ItemStack) -> Option<&Variant> {
        let id = horizon_client::save::text(stack.data()?.other("minecraft:painting/variant"))?;
        self.get(&horizon_world::datafix::dynamic::namespaced(&id))
    }
    /// `VariantUtils.readVariant` then `Painting.defineSynchedData`'s
    /// `VariantUtils.getAny`: a saved id that does not resolve, or none at all,
    /// leaves the registry's first variant in place. `Identifier.CODEC` reads a
    /// bare id in the `minecraft` namespace.
    pub fn saved_or_default(&self, id: Option<&str>) -> Option<Variant> {
        id.map(horizon_world::datafix::dynamic::namespaced)
            .and_then(|id| self.get(&id))
            .or_else(|| self.all.first())
            .cloned()
    }
}

/// `HangingEntityItem.mayPlace` for a painting: not a floor or ceiling.
/// Direction.get2DDataValue: south 0, west 1, north 2, east 3.
fn painting_facing(face: [i32; 3]) -> Option<u8> {
    if face[1] != 0 {
        return None;
    }
    Some(match face {
        [0, 0, 1] => 0,
        [-1, 0, 0] => 1,
        [0, 0, -1] => 2,
        _ => 3,
    })
}
/// Whether the camera, at the origin of `corners`' space, is on the side of the quad
/// that `normal` points to.
fn faces_camera(corners: &[glam::DVec3; 4], normal: glam::DVec3) -> bool {
    let centre = (corners[0] + corners[1] + corners[2] + corners[3]) / 4.0;
    normal.dot(-centre) > 0.0
}

impl Game<'_> {
    /// `HangingEntityItem.useOn` for a painting against `hit`'s face: P draws the variant (the stack's `painting/variant` when it
    /// names one) and hangs it; its `PaintingPlaced` event follows. CONSUME when nothing hangs.
    pub(super) fn place_painting(&mut self, hit: &Hit, stack: &ItemStack) -> bool {
        if &*stack.item != "minecraft:painting" {
            return false;
        }
        // HangingEntityItem.mayPlace: never on a top or bottom face (FAIL).
        let Some(facing) = painting_facing(hit.face) else {
            self.use_failed = true;
            return true;
        };
        let at: [i32; 3] = std::array::from_fn(|i| hit.pos[i] + hit.face[i]);
        let variant = self.painting_variants.of_stack(stack).map(|v| v.id.to_string());
        if self.hanging_place_cancelled("minecraft:painting", at) {
            return true;
        let me = Who::Player(self.own_entity_id() as u32);
        }
        let mut next = self.next_entity;
        let hung = self.ecs_with(|ecs, mw| {
            ecs.hang_painting(
                mw,
                at,
                facing,
                variant.as_deref(),
                Some(me),
                &mut |n| random_source::entity().next_int_bound(n as i32).unwrap_or(0) as usize,
                &mut || {
                    next += 1;
                    next
                },
            )
        });
        self.next_entity = next;
        if hung.is_none() {
            // Nothing fits: CONSUME, which ItemStack.useOn still counts as a use.
            self.award_stat("used", &stack.item, 1);
            return true;
        }
        if !self.creative {
            self.consume_held();
        }
        self.award_stat("used", &stack.item, 1);
        // HangingEntityItem.useOn returns SUCCESS: the hand swings.
        self.swing_arm();
        true
    }
    /// The painting the player aims at within reach: its id and distance (P's ray).
    pub(super) fn aimed_painting(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        self.ecs.aimed_painting(eye, dir, self.entity_reach)
    }
    /// `HangingEntity.hurtServer` from the player: P breaks the painting and its `PaintingBroke` event follows.
    pub(super) fn attack_painting(&mut self) -> bool {
        let Some((id, d)) = self.aimed_painting() else {
            return false;
        };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        let me = Who::Player(self.own_entity_id() as u32);
        let creative = self.creative;
        self.ecs_with(|ecs, mw| ecs.painting_hit(mw, id, me, creative))
    }
    /// PaintingRenderer: the variant on the front, the back texture behind and
    /// on the edges.
    pub(super) fn painting_geometry(&self, camera: glam::DVec3, out: &mut Vec<Vertex>) {
        let sprites = &self.gui.atlas.sprites;
        let Some(back) = sprites.get("painting/back").copied() else {
            return;
        };
        for p in &self.paintings {
            // EntityRenderer.shouldRender: the default range and its box,
            // measured from the box's centre (HangingEntity's position).
            let (lo, hi) = p.bounds();
            let (lo, hi) = (glam::DVec3::from(lo), glam::DVec3::from(hi));
            let range = default_range(hi - lo, self.view_scale());
            let Some(sight) = self.entity_sight(camera, (lo + hi) / 2.0, range, lo, hi) else {
                continue;
            };
            let culling = self.sighted_decision(sight, || self.entity_cull_decision(p, 1.0));
            if culling.hides_entity() {
                continue;
            }
            let Some(v) = self.painting_variants.get(&p.variant) else {
                continue;
            };
            let Some(front) = sprites.get(&format!("painting/{}", v.asset)).copied() else {
                continue;
            };
            let (lo, hi) = p.bounds();
            let c = glam::DVec3::new(
                (lo[0] + hi[0]) / 2.0,
                (lo[1] + hi[1]) / 2.0,
                (lo[2] + hi[2]) / 2.0,
            ) - camera;
            let s = p.step().map(f64::from);
            let l = p.left().map(f64::from);
            let out_dir = glam::DVec3::new(s[0], 0.0, s[2]);
            // The viewer's right is the painting's left.
            let right = glam::DVec3::new(l[0], 0.0, l[2]);
            let up = glam::DVec3::Y;
            let (hw, hh, d) = (
                f64::from(p.width) / 2.0,
                f64::from(p.height) / 2.0,
                1.0 / 32.0,
            );
            let entity_start = out.len();
            self.mark_motion(entity_start, Self::MOTION_OBJECT | u64::from(p.id));
            let light = self.world.light(p.pos[0], p.pos[1], p.pos[2]);
            let packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
            let mut quad =
                |corners: [glam::DVec3; 4], normal: glam::DVec3, uv: [[f32; 2]; 4], shade: f32| {
                    // Back faces are culled, as the entity pipeline culls them: inside the
                    // painting's box nothing of it shows. `corners` are relative to the camera.
                    if !faces_camera(&corners, normal) {
                        return;
                    }
                    let k = (255.0 * shade) as u32;
                    let color = 0xFF00_0000 | k << 16 | k << 8 | k;
                    for (p, uv) in corners.iter().zip(uv) {
                        out.push(Vertex {
                            normal: [0; 4],
                            pos: p.as_vec3().to_array(),
                            uv,
                            layer: 0,
                            color,
                            light: packed,

                            glint_uv: [0.0, 0.0],
                        });
                    }
                };
            let full = |r: horizon_client::hud::Rect| {
                [[r.u0, r.v1], [r.u1, r.v1], [r.u1, r.v0], [r.u0, r.v0]]
            };
            let f = c + out_dir * d;
            quad(
                [
                    f - right * hw - up * hh,
                    f + right * hw - up * hh,
                    f + right * hw + up * hh,
                    f - right * hw + up * hh,
                ],
                out_dir,
                full(front),
                1.0,
            );
            let b = c - out_dir * d;
            quad(
                [
                    b + right * hw - up * hh,
                    b - right * hw - up * hh,
                    b - right * hw + up * hh,
                    b + right * hw + up * hh,
                ],
                -out_dir,
                full(back),
                0.8,
            );
            // Edges: a one-pixel strip of the back texture.
            let strip = [
                [back.u0, back.v0 + (back.v1 - back.v0) / 16.0],
                [back.u1, back.v0 + (back.v1 - back.v0) / 16.0],
                [back.u1, back.v0],
                [back.u0, back.v0],
            ];
            let (t, bt) = (up * hh, -up * hh);
            quad(
                [
                    b + t + right * hw,
                    b + t - right * hw,
                    f + t - right * hw,
                    f + t + right * hw,
                ],
                up,
                strip,
                1.0,
            );
            quad(
                [
                    f + bt + right * hw,
                    f + bt - right * hw,
                    b + bt - right * hw,
                    b + bt + right * hw,
                ],
                -up,
                strip,
                0.5,
            );
            quad(
                [
                    b - right * hw - up * hh,
                    f - right * hw - up * hh,
                    f - right * hw + up * hh,
                    b - right * hw + up * hh,
                ],
                -right,
                strip,
                0.6,
            );
            quad(
                [
                    f + right * hw - up * hh,
                    b + right * hw - up * hh,
                    b + right * hw + up * hh,
                    f + right * hw + up * hh,
                ],
                right,
                strip,
                0.6,
            );
            self.finish_lit_entity(culling, p, out, entity_start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// `HangingEntityItem.useOn` returns SUCCESS (the hand swings) when a
    /// painting is created on a wall. Floor, ceiling, or a failed create is
    /// CONSUME: no swing.
    fn painting_use_on_swings(face: [i32; 3], created: bool) -> bool {
        painting_facing(face).is_some() && created
    }

    #[test]
    fn placing_a_painting_on_a_wall_swings() {
        assert!(painting_use_on_swings([0, 0, 1], true));
        assert!(painting_use_on_swings([-1, 0, 0], true));
        assert!(painting_use_on_swings([0, 0, -1], true));
        assert!(painting_use_on_swings([1, 0, 0], true));
        assert!(!painting_use_on_swings([0, 1, 0], true));
        assert!(!painting_use_on_swings([0, -1, 0], true));
        assert!(!painting_use_on_swings([0, 0, 1], false));
    }
}

#[cfg(test)]
mod face_tests {
    use super::faces_camera;
    use glam::DVec3;

    /// A quad at z = 2 facing -z (towards a camera at the origin) shows; seen from behind
    /// (the camera on the other side, the quad now at z = -2) it does not.
    #[test]
    fn a_quad_shows_only_to_the_side_it_faces() {
        let at = |z: f64| {
            [
                DVec3::new(-1.0, -1.0, z),
                DVec3::new(1.0, -1.0, z),
                DVec3::new(1.0, 1.0, z),
                DVec3::new(-1.0, 1.0, z),
            ]
        };
        let normal = DVec3::new(0.0, 0.0, -1.0);
        assert!(faces_camera(&at(2.0), normal));
        assert!(!faces_camera(&at(-2.0), normal));
        // Edge on: nothing to draw.
        assert!(!faces_camera(&at(0.0), normal));
    }
}
