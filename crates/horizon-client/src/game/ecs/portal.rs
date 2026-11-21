//! The carrying of the ECS entities on the host: the move of an entity with its riders from one level's `EcsLevel` to another's
//! (`ecs_carry`, for the teleports). The portals' own crossing is `send_through_portal` (blocks/nether_portal.rs).
use crate::game::{Dim, Game};
use horizon_core::nbt::Tag;

impl Game<'_> {
    /// The ECS entity `id` and the creatures riding it, all the way down, by id: the root first, each vehicle before its riders.
    pub(in crate::game) fn ecs_tree(&self, root: u32) -> Vec<u32> {
        let mut tree = vec![root];
        let mut i = 0;
        while i < tree.len() {
            let riders = self.ecs.riders_of(tree[i]);
            tree.extend(riders);
            i += 1;
        }
        tree
    }

    /// The ECS entity `id` and its riders leave this level for `destination`: their saved form is read here (the root's `Passengers`
    /// carry the riders), the tree is removed here, and the root is loaded there as `arrive` sets it. The host's portal cooldown goes
    /// with it. Returns the root's new id; None when `id` is not an ECS entity (nothing moved).
    pub(in crate::game) fn ecs_carry(
        &mut self,
        id: u32,
        destination: Dim,
        arrive: impl FnOnce(&mut Tag),
    ) -> horizon_client::Result<Option<u32>> {
        let Some(e) = self.ecs.entity_of(id) else { return Ok(None) };
        let Some(mut c) = self.ecs.entity_nbt(e) else { return Ok(None) };
        let cooldown = self.portals.entities.get(&id).map_or(0, |s| s.cooldown);
        c.insert("PortalCooldown".into(), Tag::Int(cooldown));
        let mut tag = Tag::Compound(c);
        arrive(&mut tag);
        let tree = self.ecs_tree(id);
        self.ecs_with(|ecs, _| {
            for g in &tree {
                if let Some(x) = ecs.entity_of(*g) {
                    ecs.despawn(x);
                }
            }
        });
        self.ecs_place_saved(destination, tag)
    }

    /// Loads the saved form `tag` in `destination` and links its id there; the level in place is the same after the call.
    fn ecs_place_saved(&mut self, destination: Dim, tag: Tag) -> horizon_client::Result<Option<u32>> {
        let source = self.dim;
        self.enter_level(destination)?;
        let loaded = match &tag {
            Tag::Compound(c) => self.ecs_with(|ecs, mw| ecs.load_entity(mw, c)),
            _ => None,
        };
        let id = loaded.map(|e| self.ecs_id_of(e));
        self.enter_level(source)?;
        Ok(id)
    }
}
