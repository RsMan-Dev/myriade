//! Shears and entity parts (NeoForge's `IShearable`, arismoko/horizon#3351): a player's shears and a dispenser's
//! ask the mob's entity parts first (`EntityPart::is_shearable`), and a part that says yes
//! shears it (`EntityPart::on_sheared`) in place of the species' own shearing.
use super::api_values::{TryIntoApi, TryIntoEngine};
use super::Game;
use myriade_engine::{items::ItemStack, mobs::Who};
use mod_api::PlayerRef;
use mod_api::content::crash::ModCall;
use mod_api::content::crash::enter_native_mod;

impl Game<'_> {
    /// `part_shearable` for the creature `id` of type `kind` on the ECS level: the parts are read
    /// from the mod table at the shearing, nothing is kept or polled.
    pub(in crate::game) fn part_shearable_of(
        &self,
        id: u32,
        kind: &str,
        player: Option<PlayerRef>,
        shears: &ItemStack,
    ) -> Option<(usize, bool)> {
        if !mod_api::__private::has_entity_parts() {
            return None;
        }
        let parts = mod_api::__private::entity_parts();
        let list = parts.entity_parts(kind);
        if list.is_empty() {
            return None;
        }
        let entity = self.entity(id)?;
        let shears = shears.try_into_api().ok()?;
        list.iter().enumerate().find_map(|(at, attached)| {
            let _guard = enter_native_mod(ModCall {
                id: attached.mod_id(),
                hook: "is_shearable",
                part: Some(attached.type_name()),
            });
            let answer = attached
                .part()
                .is_shearable(self, entity, player, &shears)?;
            Some((at, answer))
        })
    }

    /// `IShearable.onSheared` of part `part` of the creature `id` (type `kind`): what the part returns, empty when it is gone.
    fn part_drops(
        &mut self,
        id: u32,
        kind: &str,
        part: usize,
        player: Option<PlayerRef>,
        shears: &ItemStack,
    ) -> Vec<myriade_types::ItemStack> {
        let parts = mod_api::__private::entity_parts();
        let (Some(attached), Some(entity), Ok(shears)) = (
            parts.entity_parts(kind).get(part),
            self.entity(id),
            shears.try_into_api(),
        ) else {
            return Vec::new();
        };
        let _guard = enter_native_mod(ModCall {
            id: attached.mod_id(),
            hook: "on_sheared",
            part: Some(attached.type_name()),
        });
        attached.part().on_sheared(self, entity, player, &shears)
    }

    /// `part_shear` for an ECS creature: the drops of part `part` fall where the creature stands afterwards, one block above (the level's
    /// own throw, as `EcsWorld::shear` drops its loot). Does not touch the host's mob list; the mob's random nudge is not kept (RESTE-SANS-ECS).
    pub(in crate::game) fn part_shear_ecs(
        &mut self,
        id: u32,
        kind: &str,
        part: usize,
        player: Option<PlayerRef>,
        shears: &ItemStack,
    ) {
        let Some(before) = self.ecs_mob(id).map(|m| m.at) else { return };
        let drops = self.part_drops(id, kind, part, player, shears);
        let at = self.ecs_mob(id).map_or(before, |m| m.at);
        for drop in drops {
            if let Ok(stack) = drop.try_into_engine(self.items()) {
                self.spawn_at_location(at, 1.0, stack);
            }
        }
    }

    /// `player_shear` for an ECS creature `uid` (entity `e`): a mod part's answer, the `player_shear_entity` hook (a mod may cancel; the
    /// creature and the shears are found again after it), then a part's own shearing. None leaves the shearing to P's species rule
    /// (`behaviours::shear`); Some(consumed) ends the interaction.
    pub(in crate::game) fn player_shear_ecs(
        &mut self,
        uid: u32,
        e: bevy_ecs::entity::Entity,
        off_hand: bool,
        held: Option<&ItemStack>,
    ) -> Option<bool> {
        let shears = held?.clone();
        let (kind, at) = self.ecs_mob(uid).map(|m| (m.kind, m.at))?;
        let player = self.native_acting_player();
        let part = self.part_shearable_of(uid, kind, player, &shears);
        if part.is_some_and(|(_, shearable)| !shearable) {
            return Some(false);
        }
        if part.is_none() && !self.ecs.can_shear(uid) {
            return None;
        }
        if let (Some(uuid), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            let mut cancel = false;
            self.native_player_shear_entity(uuid, &name, uid as i32, off_hand, &mut cancel);
            let same = self.held().is_some_and(|h| h.item == shears.item && h.count == shears.count);
            if cancel || !same || self.ecs.entity_of(uid) != Some(e) {
                return Some(true);
            }
        }
        let (part, _) = part?;
        self.part_shear_ecs(uid, kind, part, player, &shears);
        let ctx = self.who_context(Some(Who::Player), None);
        self.post_game_event("minecraft:shear", [at.x, at.y, at.z], ctx);
        self.damage_held_tool();
        self.swing_arm();
        Some(true)
    }
}
