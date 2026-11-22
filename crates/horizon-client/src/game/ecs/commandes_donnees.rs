//! The commands' reads and writes of an entity's NBT on the ECS level, by network id: `/data get entity` reads the saved tree,
//! `/data merge entity` loads a patch onto it, `/tag` writes `Tags`, `/execute on owner|leasher` reads the holder's UUID. Each is
//! one read or one write on the entity of the id; none walks the world and none copies state.
use crate::game::Game;
use horizon_core::nbt::{Compound, JavaString, Tag};

fn int_array4(t: &Tag) -> Option<[i32; 4]> {
    match t {
        Tag::IntArray(v) => v.as_slice().try_into().ok(),
        _ => None,
    }
}

impl Game<'_> {
    /// `Entity.saveWithoutId` of the mob `id` as the ECS level holds it; None when it is not in the level.
    pub(in crate::game) fn ecs_mob_tree(&self, id: u32) -> Option<Compound> {
        self.ecs.entity_nbt(self.ecs.entity_of(id)?)
    }
    /// `Entity.load` of `patch` onto the mob `id`; false when it is not saved.
    pub(in crate::game) fn ecs_merge_mob(&mut self, id: u32, patch: &Compound) -> bool {
        let Some(e) = self.ecs.entity_of(id) else {
            return false;
        };
        self.ecs.merge_nbt(e, patch)
    }
    /// The kind of the entity `id` in the ECS level.
    pub(in crate::game) fn ecs_mob_kind(&self, id: u32) -> Option<&'static str> {
        self.ecs.kind(id)
    }
    /// Whether a player rides the entity `id` (`controller`), by the bridge's rider of its entity.
    pub(in crate::game) fn ecs_mob_has_rider(&self, id: u32) -> bool {
        self.ecs.entity_of(id).and_then(|e| self.ecs.rider_of(e)).is_some()
    }
    /// `Mob.ai.last_hurt_by` (attacker) or `ai.target` of the entity `id`, read by id from the bridge (missing: `last_hurt_by`, `ai_target`).
    pub(in crate::game) fn ecs_mob_ai_target(&self, id: u32, attacker: bool) -> Option<horizon_client::mobs::Who> {
        if attacker { self.ecs.last_hurt_by(id) } else { self.ecs.ai_target(id) }
    }
    /// `Entity.getTags` of the entity `id` in the ECS level: the `Tags` list of its tree; none when it has no tag left.
    pub(in crate::game) fn ecs_mob_tags(&self, id: u32) -> Vec<String> {
        let Some(tree) = self.ecs_mob_tree(id) else {
            return Vec::new();
        };
        match Tag::Compound(tree).get("Tags") {
            Some(Tag::List(l)) => l
                .iter()
                .filter_map(|t| match t {
                    Tag::String(s) => Some(s.to_string_lossy()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
    /// The summoner id of the vex `id` (`counter2` of the host), read by id from the bridge (missing: `vex_owner`).
    pub(in crate::game) fn ecs_vex_owner(&self, id: u32) -> Option<u32> {
        self.ecs.vex_owner(id)
    }
    /// The height of the eyes above the feet of the entity `id`, read by id from the bridge (missing: `eye_offset`).
    pub(in crate::game) fn ecs_mob_eye_offset(&self, id: u32) -> Option<f64> {
        self.ecs.eye_offset(id)
    }
    /// The owner's UUID of the mob `id` as saved (`Owner`) in the ECS level.
    pub(in crate::game) fn ecs_mob_owner(&self, id: u32) -> Option<[i32; 4]> {
        int_array4(Tag::Compound(self.ecs_mob_tree(id)?).get("Owner")?)
    }
    /// The UUID of the holder of the lead of `id` (`leash`'s `UUID`) in the ECS level; None for a knot or no lead.
    pub(in crate::game) fn ecs_leash_uuid(&self, id: u32) -> Option<[i32; 4]> {
        int_array4(Tag::Compound(self.ecs_mob_tree(id)?).get("leash")?.get("UUID")?)
    }
    /// `OwnableEntity.getOwner`'s UUID of the mob `id`: the ECS tree's `Owner`.
    pub(in crate::game) fn mob_owner_uuid(&self, id: u32) -> Option<[i32; 4]> {
        self.ecs_mob_owner(id)
    }
    /// `Entity.setTags`: the `Tags` of the mob `id` in the ECS level, as the patch the save reads back.
    pub(in crate::game) fn ecs_set_mob_tags(&mut self, id: u32, tags: &[String]) -> bool {
        let list = tags.iter().map(|s| Tag::String(s.as_str().into())).collect();
        let mut patch = Compound::new();
        patch.insert(JavaString::from("Tags"), Tag::List(list));
        self.ecs_merge_mob(id, &patch)
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_core::nbt::{Compound, JavaString, Tag};
    use horizon_ecs_proto::bridge::host::HostSetup;

    #[test]
    fn a_merged_patch_reads_back_in_the_entity_tree() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            let id = game.ecs_spawn("minecraft:cow", [0.5, 64.0, 0.5], None).unwrap();
            let tags = Tag::List(vec![Tag::String("merged".into())]);
            let mut patch = Compound::new();
            patch.insert(JavaString::from("Tags"), tags.clone());
            assert!(game.ecs_merge_mob(id, &patch));
            let tree = Tag::Compound(game.ecs_mob_tree(id).unwrap());
            assert_eq!(tree.get("Tags"), Some(&tags));
        });
    }
}
