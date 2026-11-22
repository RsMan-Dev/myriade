//! Pick Block on an entity (`Minecraft.pickBlockOrEntity`'s entity case and
//! `ServerGamePacketListenerImpl.handlePickItemFromEntity`): the client aims,
//! the server answers with `Entity.getPickResult`.
use super::Game;
use horizon_client::items::ItemStack;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CrosshairEntityHit {
    id: i32,
    distance: f64,
    pickable: bool,
    suggest_uuid: bool,
}

fn nearest_entity(
    hits: [Option<CrosshairEntityHit>; 3],
    block_distance: Option<f64>,
) -> Option<CrosshairEntityHit> {
    hits.into_iter().flatten().fold(None, |best, hit| {
        if block_distance.is_none_or(|distance| hit.distance < distance)
            && best.is_none_or(|best| hit.distance < best.distance)
        {
            Some(hit)
        } else {
            best
        }
    })
}

impl Game<'_> {
    fn crosshair_entity_hit(&self) -> Option<CrosshairEntityHit> {
        let hit = |id: u32, distance, pickable| CrosshairEntityHit {
            id: id as i32,
            distance,
            pickable,
            suggest_uuid: true,
        };
        let hits = [
            self.mob_hit()
                .map(|(index, distance, _)| hit(self.remote_entities[index].id, distance, true)),
            self.redirectable_hit()
                .map(|(id, distance)| hit(id, distance, false)),
            self.dragon_hit().map(|(id, distance)| CrosshairEntityHit {
                id: id as i32,
                distance,
                pickable: false,
                suggest_uuid: false,
            }),
        ];
        if hits.iter().all(Option::is_none) {
            return None;
        }
        nearest_entity(hits, self.block_distance())
    }

    /// `Entity.getPickResult` of the entity `id`: a mob's spawn egg, the item
    /// of a boat, minecart, armor stand, cushion, painting, item frame (the
    /// framed item when it holds one) or end crystal.
    pub(super) fn entity_pick_result(&self, id: i32) -> Option<ItemStack> {
        let id = u32::try_from(id).ok()?;
        let mob_kind = if self.ecs.is_active() {
            self.ecs.kind(id).filter(|_| self.creature_species(id).is_some())
        } else {
            self.remote_entities.iter().find(|m| m.id == id).map(|m| &m.kind.id[..])
        };
        let item = if let Some(kind) = mob_kind {
            // Mob.getPickResult: SpawnEggItem.byId.
            let egg = format!("{kind}_spawn_egg");
            return self.items().get(&egg).map(|_| ItemStack::new(&egg, 1));
        } else if let Some(name) = self.boat_entity_type(id) {
            name
        } else if let Some(k) = self.cart_genre(id) {
            k.item().to_owned()
        } else if self.armor_stand_list().iter().any(|a| a.id == id) {
            "minecraft:armor_stand".to_owned()
        } else if let Some(c) = self.cushion_saved(id) {
            c.item()
        } else if let Some(f) = self.ecs_frames().into_iter().find(|f| f.id == id) {
            // ItemFrame.getPickResult: what it frames, else the frame itself.
            return Some(
                f.item
                    .clone()
                    .unwrap_or_else(|| ItemStack::new(f.kind(), 1)),
            );
        } else if self.ecs_paintings().into_iter().any(|p| p.id == id) {
            "minecraft:painting".to_owned()
        } else if self.crystal(id).is_some() {
            "minecraft:end_crystal".to_owned()
        } else {
            return None;
        };
        Some(ItemStack::new(&item, 1))
    }

    /// The entity the crosshair hits before any block, as `Entity.getId`; an
    /// entity with nothing to pick (a projectile, a player, the dragon) is
    /// `Some(None)`. `None` when the view hits a block or nothing.
    pub(super) fn pick_entity_hit_at(&self) -> Option<(f64, Option<i32>)> {
        self.crosshair_entity_hit()
            .map(|hit| (hit.distance, hit.pickable.then_some(hit.id)))
    }

    /// `Player.isWithinEntityInteractionRange(entity, 3.0)`: the entity's box
    /// within the interaction range plus 3 of the eyes.
    pub(super) fn within_entity_range(&self, id: i32) -> bool {
        let Some(eye) = self.eye_position() else {
            return false;
        };
        let mob = u32::try_from(id).ok().and_then(|id| self.mob_box(id));
        let Some(b) = self
            .object_target(id)
            .map(|(lo, hi, _)| (lo, hi))
            .or_else(|| {
                mob.or_else(|| self.tracked_special_box(id)).map(|b| {
                    let (lo, hi) = (b.min_position(), b.max_position());
                    ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
                })
            })
        else {
            return false;
        };
        let d2: f64 = (0..3)
            .map(|a| (eye[a] - eye[a].clamp(b.0[a], b.1[a])).powi(2))
            .sum();
        let range = self.entity_reach + 3.0;
        d2 < range * range
    }
}

#[cfg(test)]
mod tests {
    use super::{CrosshairEntityHit, nearest_entity};

    #[test]
    fn nearest_crosshair_entity_preserves_occlusion_ties_and_pick_metadata() {
        let object = CrosshairEntityHit {
            id: 17,
            distance: 2.0,
            pickable: true,
            suggest_uuid: true,
        };
        let projectile = CrosshairEntityHit {
            id: 23,
            distance: 1.0,
            pickable: false,
            suggest_uuid: true,
        };
        let dragon = CrosshairEntityHit {
            id: 29,
            distance: 0.5,
            pickable: false,
            suggest_uuid: false,
        };
        assert_eq!(nearest_entity([None; 3], None), None);
        assert_eq!(
            nearest_entity([Some(object), None, None], None),
            Some(object)
        );
        for block_distance in [1.0, 2.0] {
            assert_eq!(
                nearest_entity([Some(object), None, None], Some(block_distance)),
                None
            );
        }
        assert_eq!(
            nearest_entity([Some(object), None, Some(projectile)], Some(3.0)),
            Some(projectile)
        );
        assert_eq!(
            nearest_entity([Some(object), Some(projectile), Some(dragon)], None),
            Some(dragon)
        );
        assert_eq!(
            nearest_entity(
                [
                    Some(object),
                    Some(CrosshairEntityHit {
                        distance: 2.0,
                        ..projectile
                    }),
                    None
                ],
                None
            ),
            Some(object)
        );
    }
}
