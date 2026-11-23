//! The carrying of the ECS entities on the host: the move of an entity with its riders from one level's `EcsLevel` to another's
//! (`ecs_carry`, for the teleports). The portals' own crossing is `send_through_portal` (blocks/nether_portal.rs).
use crate::game::{Dim, Game};
use horizon_core::nbt::Tag;

impl Game<'_> {
    /// The ECS entity `id` and the creatures riding it, all the way down, by id: the root first, then each rider with its own riders,
    /// in the order `Passengers` is written (seats and mounts both).
    pub(in crate::game) fn ecs_tree(&self, root: u32) -> Vec<u32> {
        self.ecs.ride_tree(root)
    }

    /// Writes into `tag` and the riders in its `Passengers` the portal cooldown `cooldown` gives for the ids `ids` (the tree's, in NBT
    /// order), so that the delay travels in the NBT (`PortalCooldown`) and `ecs_load_tree` gives it back.
    pub(in crate::game) fn stamp_cooldowns(
        tag: &mut Tag,
        ids: &mut dyn Iterator<Item = u32>,
        cooldown: &mut dyn FnMut(u32) -> i32,
    ) {
        let Some(id) = ids.next() else { return };
        myriade_engine::nether_portal::put_portal_cooldown(tag, cooldown(id));
        if let Some(Tag::List(riders)) = tag.get_mut("Passengers") {
            for rider in riders {
                Self::stamp_cooldowns(rider, ids, cooldown);
            }
        }
    }

    /// The ECS entity `id` and its riders leave this level for `destination`: their saved form is read here (the root's `Passengers`
    /// carry the riders), the tree is removed here, and the root is loaded there as `arrive` sets it. The portal cooldowns and the mod
    /// attachments of the tree go with it. Returns the root's new id; None when `id` is not an ECS entity (nothing moved).
    pub(in crate::game) fn ecs_carry(
        &mut self,
        id: u32,
        destination: Dim,
        arrive: impl FnOnce(&mut Tag),
    ) -> myriade_engine::Result<Option<u32>> {
        let Some(e) = self.ecs.entity_of(id) else { return Ok(None) };
        let Some(c) = self.ecs.entity_nbt(e) else { return Ok(None) };
        let tree = self.ecs_tree(id);
        let mut tag = Tag::Compound(c);
        Self::stamp_cooldowns(&mut tag, &mut tree.iter().copied(), &mut |i| {
            self.portals.entities.remove(&i).map_or(0, |s| s.cooldown)
        });
        arrive(&mut tag);
        self.ecs_with(|ecs, _| {
            for g in &tree {
                if let Some(x) = ecs.entity_of(*g) {
                    ecs.despawn(x);
                }
            }
        });
        let pairs = self.ecs_place_saved(destination, tag, &tree)?;
        Ok(pairs.iter().find(|(old, _)| *old == id).map(|&(_, new)| new))
    }

    /// Loads the saved form `tag` in `destination` (`old`: the ids it was saved from, in NBT order) and links the ids there; the level
    /// in place is the same after the call. The pairs (old id, new id) of the entities made.
    fn ecs_place_saved(&mut self, destination: Dim, tag: Tag, old: &[u32]) -> myriade_engine::Result<Vec<(u32, u32)>> {
        let source = self.dim;
        self.enter_level(destination)?;
        let pairs = match &tag {
            Tag::Compound(c) => self.ecs_load_tree(c, old),
            _ => Vec::new(),
        };
        self.enter_level(source)?;
        Ok(pairs)
    }
}
