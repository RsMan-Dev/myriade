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
    pub width: i32,
    pub height: i32,
}
/// Variants by id.
#[derive(Default)]
pub(super) struct PaintingVariants {
    pub all: Vec<Variant>,
    /// The names `#minecraft:placeable` lists, in tag order.
    pub placeable: Vec<Arc<str>>,
}
impl PaintingVariants {
    /// The registry's variants (`data/minecraft/painting_variant`, by file name) and the `#placeable` tag, read from `resources`.
    pub fn load(resources: &std::path::Path) -> Self {
        let data = resources.join("data/minecraft");
        let mut files: Vec<_> = std::fs::read_dir(data.join("painting_variant")).into_iter().flatten().flatten().map(|f| f.path()).collect();
        files.sort();
        let all: Vec<Variant> = files
            .iter()
            .filter_map(|p| {
                let json: serde_json::Value = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
                let size = |k: &str| json.get(k).and_then(|v| v.as_i64()).map(|v| v as i32);
                Some(Variant { id: format!("minecraft:{}", p.file_stem()?.to_str()?).into(), width: size("width")?, height: size("height")? })
            })
            .collect();
        let tag = std::fs::read(data.join("tags/painting_variant/placeable.json")).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        let placeable = tag
            .iter()
            .flat_map(|t| t["values"].as_array().into_iter().flatten())
            .filter_map(|v| v.as_str())
            .filter(|n| all.iter().any(|v| &*v.id == *n))
            .map(Arc::from)
            .collect();
        Self { all, placeable }
    }
    /// The table the ECS hangs paintings from: the placeable variants first, then the others, and how many are placeable.
    pub fn motives(&self) -> (Vec<(Box<str>, horizon_ecs_proto::hanging::Size)>, usize) {
        let size = |v: &Variant| horizon_ecs_proto::hanging::Size { width: v.width as u8, height: v.height as u8 };
        let first = self.placeable.iter().filter_map(|n| self.get(n));
        let rest = self.all.iter().filter(|v| !self.placeable.contains(&v.id));
        let table: Vec<_> = first.chain(rest).map(|v| (Box::from(&*v.id), size(v))).collect();
        (table, self.placeable.len())
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
        }
        let me = Who::Player(self.own_entity_id() as u32);
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
        // BlockAttachedEntity.skipAttackInteraction: a player may not hit it where `mayInteract` refuses.
        if let Some([x, y, z]) = self.ecs_hanging_pos(id) && !self.may_interact_at(horizon_core::position::BlockPos::new(x, y, z)) {
            return true;
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
