//! `OminousItemSpawner`: the item an ominous trial spawner drops on its
//! fight. It floats for 60 to 120 ticks, then falls as an item or is fired
//! straight down as a projectile (`spawnItem`). Saved with its chunk's
//! entities (persist.rs).
use super::Game;
use horizon_client::items::ItemStack;
use horizon_core::geometry::Vec3;

/// `TICKS_BEFORE_ABOUT_TO_SPAWN_SOUND`.
const ABOUT_TO_SPAWN: i64 = 36;

pub(super) struct OminousItemSpawner {
    /// Entity-owned registration metadata; content clones are detached.
    pub(super) attachment_lifetime: horizon_mod_runtime::attachments::AttachmentLifetime,
    pub(in crate::game) join: horizon_client::join_seq::JoinSeq,
    /// Its entity id: from the entity counter on the server, the server's on
    /// a client.
    pub(super) id: u32,
    /// `DATA_ITEM`; `minecraft:air` stands for `ItemStack.EMPTY`.
    pub(super) item: ItemStack,
    pub(super) position: [f64; 3],
    pub(super) spawn_item_after: i64,
    tick_count: i64,
    /// Its UUID, and the compound it was loaded from (fields Horizon doesn't
    /// model survive a save).
    pub(super) uuid: [i32; 4],
    pub(super) raw: Option<Box<horizon_core::nbt::Tag>>,
}

horizon_client::impl_joined!(OminousItemSpawner);
horizon_mod_runtime::impl_attachment_lifetime!(OminousItemSpawner);

/// `ProjectileItem.DispenseConfig` of the items fired down: power,
/// uncertainty and `overrideDispenseEvent`; None for items that just fall.
fn projectile(item: &str) -> Option<(f32, f32, Option<i32>)> {
    Some(match item {
        "minecraft:fire_charge" => (1.0, 6.666_666_5, Some(1018)),
        "minecraft:wind_charge" => (1.0, 6.666_666_5, Some(1051)),
        "minecraft:firework_rocket" => (0.5, 1.0, Some(1004)),
        "minecraft:experience_bottle"
        | "minecraft:splash_potion"
        | "minecraft:lingering_potion" => (1.375, 3.0, None),
        "minecraft:arrow"
        | "minecraft:spectral_arrow"
        | "minecraft:tipped_arrow"
        | "minecraft:snowball"
        | "minecraft:egg"
        | "minecraft:blue_egg"
        | "minecraft:brown_egg" => (1.1, 6.0, None),
        _ => return None,
    })
}

fn block(p: [f64; 3]) -> [i32; 3] {
    p.map(|v| v.floor() as i32)
}

impl Game<'_> {
    /// `OminousItemSpawner.create` added at `position`.
    pub(super) fn add_ominous_item_spawner(
        &mut self,
        item: ItemStack,
        position: [f64; 3],
        delay: i32,
    ) {
        self.next_entity += 1;
        self.ominous_spawners.push(OminousItemSpawner {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id: self.next_entity,
            item,
            position,
            spawn_item_after: i64::from(delay),
            tick_count: 0,
            uuid: horizon_client::save::random_uuid(),
            raw: None,
        });
    }
    /// A saved spawner, as `readAdditionalSaveData` leaves it: its tick
    /// count starts again from 0.
    pub(super) fn load_ominous_item_spawner(&mut self, tag: horizon_core::nbt::Tag) {
        self.load_ominous_item_spawner_transferred(tag, None);
    }
    pub(super) fn load_ominous_item_spawner_transferred(
        &mut self,
        mut tag: horizon_core::nbt::Tag,
        transferred: Option<super::entity_initialization::TransferredEntityState>,
    ) {
        let Some((item, at, spawn_item_after)) =
            horizon_client::entity_nbt::ominous_item_spawner_from_nbt(&tag)
        else {
            return;
        };
        self.next_entity += 1;
        let id = self.next_entity;
        let saved = horizon_client::entity_nbt::uuid(&tag);
        // Its saved UUID and the attachments saved under it (#7398).
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        self.remember_uuid(id, saved, &tag);
        // Entity.load's `setXRot` keeps the pitch in -90..90 and its `setRot`
        // takes both `% 360`; the save writes the rotation back from this tag.
        let [yaw, pitch] = horizon_client::entity_nbt::rotation(&tag);
        let (yaw, pitch) = (yaw % 360.0, (pitch % 360.0).clamp(-90.0, 90.0));
        let floats = [yaw, pitch].map(|v| horizon_core::nbt::Tag::Float(v.to_bits()));
        if let horizon_core::nbt::Tag::Compound(map) = &mut tag {
            map.insert(
                "Rotation".into(),
                horizon_core::nbt::Tag::List(floats.into()),
            );
        }
        let uuid = self.uuid_for(id);
        self.ominous_spawners.push(OminousItemSpawner {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id,
            item,
            position: [at.x, at.y, at.z],
            spawn_item_after,
            tick_count: 0,
            uuid,
            raw: Some(Box::new(tag)),
        });
    }
    /// Every item spawner's tick: `commonTick`'s count, `tickServer` (the
    /// warning sound, then the item) and `tickClient` (particles every 5
    /// game ticks).
    pub(super) fn tick_ominous_item_spawners(&mut self) {
        let mut spawners = self.ominous_spawners.lift();
        #[cfg(test)]
        super::entity::taken_pass_tests::after_take(self);
        // ServerLevel.tick: only entities in entity-ticking chunks tick.
        let ticking = self.entity_ticking_area();
        spawners.retain_mut(|s| {
            if !ticking(s.position[0].floor() as i32, s.position[2].floor() as i32) {
                return true;
            }
            s.tick_count += 1;
            let at = block(s.position);
            let center = at.map(|v| f64::from(v) + 0.5);
            if s.tick_count == s.spawn_item_after - ABOUT_TO_SPAWN {
                self.audio.play_on(
                    6, // SoundSource.NEUTRAL, as OminousItemSpawner.tickServer selects.
                    "minecraft:block.trial_spawner.about_to_spawn_item",
                    Some(center),
                    1.0,
                    1.0,
                );
            }
            if s.tick_count >= s.spawn_item_after {
                self.spawn_ominous_item(s);
                return self.forget_finished_spawner(s.id);
            }
            true
        });
        self.ominous_spawners.restore(
            spawners,
            horizon_client::join_seq::RestoreOrder::SurvivorsFirst,
        );
    }
    /// `handleAddEntity` for a spawner a server sent: its item comes with the
    /// entity data (`ItemStack.EMPTY` until then, which draws nothing).
    pub(super) fn add_remote_ominous_item_spawner(&mut self, id: u32, position: [f64; 3]) {
        self.ominous_spawners.push(OminousItemSpawner {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id,
            item: ItemStack::new("minecraft:air", 1),
            position,
            spawn_item_after: 0,
            tick_count: 0,
            uuid: [0; 4],
            raw: None,
        });
    }
    /// `spawnItem`: a projectile shot down, or an item entity; level event
    /// 3021 and the entity_place game event.
    fn spawn_ominous_item(&mut self, s: &OminousItemSpawner) {
        // An empty item spawns nothing, and no event marks it.
        if &*s.item.item == "minecraft:air" {
            return;
        }
        let at = block(s.position);
        let p = s.position;
        match projectile(&s.item.item) {
            Some((power, uncertainty, event)) => {
                if let Some(event) = event {
                    self.level_event_at(event, at, 0);
                }
                let from = Vec3::new(p[0], p[1], p[2]);
                self.shoot_projectile_item(from, [0.0, -1.0, 0.0], &s.item, power, uncertainty);
            }
            None => {
                self.spawn_item_own(s.item.clone(), p, 0);
            }
        }
        self.level_event_at(3021, at, 1);
        self.post_block_game_event("minecraft:entity_place", at, None, None);
    }
}

#[cfg(test)]
mod warning_tests;
