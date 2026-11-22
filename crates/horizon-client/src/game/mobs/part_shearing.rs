//! Shears and entity parts (NeoForge's `IShearable`, arismoko/horizon#3351): a player's shears and a dispenser's
//! ask the mob's entity parts first (`EntityPart::is_shearable`), and a part that says yes
//! shears it (`EntityPart::on_sheared`) in place of the species' own shearing.
use super::api_values::{TryIntoApi, TryIntoEngine};
use super::Game;
use mod_api::client::first_person::Hand;
use horizon_client::{
    items::ItemStack,
    mobs::{Species, Who},
};
use horizon_core::{geometry::Vec3, random::RandomSource};
use mod_api::{
    PlayerRef,
    content::crash::{ModCall, enter_native_mod},
};

impl Game<'_> {
    /// `IShearable.isShearable` as live mob `index`'s entity parts answer it: the answering part's
    /// position in dispatch order and its answer, the first part to answer winning; none when no
    /// part answers. A type with no part costs one table lookup. Server-side.
    pub(super) fn part_shearable(
        &self,
        index: usize,
        player: Option<PlayerRef>,
        shears: &ItemStack,
    ) -> Option<(usize, bool)> {
        if !self.remote_entities[index].alive() {
            return None;
        }
        let mob = &self.remote_entities[index];
        self.part_shearable_of(mob.id, &mob.kind.id, player, shears)
    }

    /// `part_shearable` for the creature `id` of type `kind`, wherever it lives (`remote_entities` or the ECS level): the parts are read
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

    /// `IShearable.onSheared` on mob `index`'s entity part `part` (as [`Self::part_shearable`]
    /// named it), then `spawnShearedDrop` for each drop it returns: one block above where the mob
    /// stands afterwards, nudged by the mob's own random as `Sheep.shear` nudges its wool.
    pub(super) fn part_shear(
        &mut self,
        index: usize,
        part: usize,
        player: Option<PlayerRef>,
        shears: &ItemStack,
    ) {
        let (id, at) = (self.remote_entities[index].id, self.remote_entities[index].position);
        let kind = self.remote_entities[index].kind.id.clone();
        let drops = self.part_drops(id, &kind, part, player, shears);
        // The part may have moved or removed the mob: its drops fall where it is now.
        let index = self.remote_entities.iter().position(|m| m.id == id && !m.removed);
        let at = index.map_or(at, |i| self.remote_entities[i].position);
        for drop in drops {
            let Ok(stack) = drop.try_into_engine(self.items()) else {
                continue;
            };
            self.spawn_at_location(at, 1.0, stack);
            let Some(i) = index else { continue };
            let r = &mut self.remote_entities[i].random;
            let push = [
                f64::from((r.next_float() - r.next_float()) * 0.1),
                f64::from(r.next_float() * 0.05),
                f64::from((r.next_float() - r.next_float()) * 0.1),
            ];
            if let Some(item) = self.just_spawned_item() {
                let v = item.velocity;
                item.velocity = Vec3::new(v.x + push[0], v.y + push[1], v.z + push[2]);
            }
        }
    }

    /// `IShearable.onSheared` of part `part` of the creature `id` (type `kind`): what the part returns, empty when it is gone.
    fn part_drops(
        &mut self,
        id: u32,
        kind: &str,
        part: usize,
        player: Option<PlayerRef>,
        shears: &ItemStack,
    ) -> Vec<mod_api::ItemStack> {
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
    /// own throw, as `EcsWorld::shear` drops its loot). Does not touch `remote_entities`.
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

    /// Shears in the acting player's hand on mob `index` (`ShearsItem.interactLivingEntity`):
    /// the entity parts' answer, else the species' own (`Sheep`, `MushroomCow`, `SnowGolem`,
    /// `Bogged`), and a shearable mob fires `player_shear_entity`. `Some(consumed)` ends the
    /// interaction: a part's refusal (false), the hook's cancel or a part's shearing (true);
    /// none leaves the shearing to the species' own interaction.
    pub(super) fn player_shear(&mut self, index_slot: &mut usize) -> Option<bool> {
        let mut index = *index_slot;
        let shears = self.held().cloned()?;
        let player = self.native_acting_player();
        let part = self.part_shearable(index, player, &shears);
        if part.is_some_and(|(_, shearable)| !shearable) {
            return Some(false);
        }
        let mob = &self.remote_entities[index];
        let (id, at) = (mob.id, mob.position);
        let shearable = part.is_some()
            || match mob.species() {
                Species::Sheep => !mob.extra.sheared && !mob.is_baby(),
                Species::Mooshroom => !mob.is_baby(),
                Species::SnowGolem | Species::Bogged => mob.alive() && !mob.extra.sheared,
                _ => false,
            };
        if shearable
            && let (Some(uuid), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone())
        {
            let mut cancel = false;
            let off_hand = self.hand == Hand::Off;
            self.native_player_shear_entity(uuid, &name, id as i32, off_hand, &mut cancel);
            if cancel {
                return Some(true);
            }
            // The hook had `&mut self`: find the mob again by id; a mob a mod
            // removed, or shears it swapped, ends the interaction.
            let Some(now) = self.remote_entities.iter().position(|m| m.id == id && !m.removed) else {
                return Some(true);
            };
            index = now;
            *index_slot = now;
            let same = self
                .held()
                .is_some_and(|h| h.item == shears.item && h.count == shears.count);
            if !same {
                return Some(true);
            }
        }
        let (part, _) = part?;
        self.part_shear(index, part, player, &shears);
        let ctx = self.who_context(Some(Who::Player), None);
        self.post_game_event("minecraft:shear", [at.x, at.y, at.z], ctx);
        self.damage_held_tool();
        self.swing_arm();
        Some(true)
    }
}
