//! Paintings: `HangingEntityItem.useOn` placement with `Painting.create`'s
//! variant choice (or the stack's `painting/variant`), `HangingEntity.survives`
//! checks every 100 ticks, breaking by attack (`dropItem`), saving, and
//! `PaintingRenderer` quads.
use horizon_core::random::RandomSource as _;
use super::{Game, Hit};
use horizon_client::{items::ItemStack, random_source};
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
