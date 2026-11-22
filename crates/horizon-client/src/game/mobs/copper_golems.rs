//! Copper golems on the game side: what their brain (`mobs::transport_items`)
//! does to chests (opener counts, the lid block events, slots), and the
//! statue spawn from an axe. The antenna, the carried item, the waxing and the
//! statue's turning have no ECS hook yet (RESTE-SANS-ECS).
use super::Game;
use horizon_client::items::ItemStack;

impl Game<'_> {
    /// `OfferFlowerGoal.stop` (`MobEvent::Gift`). RESTE-SANS-ECS: the antenna
    /// has no ECS setter yet.
    pub(super) fn receive_gift(&mut self, _target: u32, _item: &'static str) {}
    /// A copper golem opened or closed a chest half: the counter changes, the
    /// first opener posts CONTAINER_OPEN and the last closer CONTAINER_CLOSE
    /// (`ContainerOpenersCounter.incrementOpeners` / `decrementOpeners`), and
    /// the clients see the block event (`openerCountChanged`).
    pub(in crate::game) fn golem_chest_open(&mut self, chest: [i32; 3], open: bool, mob: u32) {
        let key = (chest[0], chest[1], chest[2]);
        let entry = self.world.chest_openers.entry(key).or_insert((0, 0.0));
        let before = entry.0;
        if open {
            entry.0 += 1;
            entry.1 = entry.1.max(3.0);
        } else {
            entry.0 -= 1;
            if entry.0 == 0 {
                entry.1 = 0.0;
            }
        }
        let count = entry.0;
        if count <= 0 {
            self.world.chest_openers.remove(&key);
        }
        let who = Some(horizon_client::mobs::Who::Mob(mob));
        if open && before == 0 {
            self.post_block_game_event("minecraft:container_open", chest, who, None);
        } else if !open && count == 0 {
            self.post_block_game_event("minecraft:container_close", chest, who, None);
        }
        self.block_entity_event(chest, 1, count.max(0));
        // The guests get the same `ClientboundBlockEventPacket` (the lid).
        self.lan_block_event(chest, 1, count.max(0));
    }
    /// A copper golem took from or filled a chest slot.
    pub(in crate::game) fn golem_chest_slot(
        &mut self,
        chest: [i32; 3],
        slot: usize,
        stack: Option<ItemStack>,
    ) {
        let key = (chest[0], chest[1], chest[2]);
        if let Some(e) = self.world.block_entities.get_mut(&key)
            && let Some(s) = e.slots_mut().get_mut(slot)
        {
            *s = stack;
            // TransportItemsBetweenContainers calls Container.setChanged.
            self.container_changed(key);
        }
    }
    /// `CopperGolem.turnToStatue`, which its weathering asked for. RESTE-SANS-ECS:
    /// the statue, its drops and the leash break have no ECS hook yet.
    pub(super) fn golem_become_statue(&mut self, _id: u32) {}
    // ---- interactions -----------------------------------------------------
    /// `WeatheringCopperGolemStatueBlock.useItemOn`: an axe on an unoxidized
    /// statue turns it back into a golem (`CopperGolemStatueBlockEntity.removeStatue`),
    /// wearing the axe. Weathered statues are scraped by the axe's own
    /// transform instead. RESTE-SANS-ECS: the statue's custom name is not carried over.
    pub(super) fn use_golem_statue(&mut self, pos: [i32; 3], held: Option<&ItemStack>) -> bool {
        if !held.is_some_and(|s| s.item.ends_with("_axe")) {
            return false;
        }
        let registry = self.pack.assets.registry;
        let Some(state) = registry.state(self.world.block(pos[0], pos[1], pos[2])) else {
            return false;
        };
        if state.name != "minecraft:copper_golem_statue" {
            return false;
        }
        // `Direction.toYRot` of the statue's facing.
        let yaw: f32 = match state.properties.get("facing").map(String::as_str) {
            Some("west") => 90.0,
            Some("north") => 180.0,
            Some("east") => 270.0,
            _ => 0.0,
        };
        let waterlogged = state
            .properties
            .get("waterlogged")
            .is_some_and(|w| w == "true");
        // initCopperGolem: at the block's feet, facing as the statue does.
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        ];
        if self
            .ecs_spawn_mob(
                "minecraft:copper_golem",
                at,
                true,
                Some(yaw),
                super::creatures::SpawnSite::Raw,
            )
            .is_none()
        {
            return false;
        }
        self.damage_held(1);
        self.audio
            .play("minecraft:entity.copper_golem.spawn", Some(at), 1.0, 1.0);
        // Level.removeBlock(pos, false): air, or the water the statue stood in.
        let replacement = registry.default_state(if waterlogged {
            "minecraft:water"
        } else {
            "minecraft:air"
        });
        if let Some(replacement) = replacement {
            let _ = self.level_set(pos, replacement);
        }
        self.world.block_entities.remove(&(pos[0], pos[1], pos[2]));
        true
    }
    /// `CopperGolem.mobInteract`; true when handled. RESTE-SANS-ECS: the held
    /// item, the antenna shears, the waxing and the scraping have no ECS hook
    /// yet, so no interaction is handled here.
    pub(super) fn interact_copper_golem(&mut self, _id: u32, _item: &str) -> bool {
        false
    }
    /// `readyForShearing`: the antenna holds a flower shears take. RESTE-SANS-ECS:
    /// the antenna is not in the ECS yet, so no golem is shearable.
    pub(super) fn golem_shearable(&self, _id: u32) -> bool {
        false
    }
}
