//! Allays on the game side: the hand-over interaction (`Allay.mobInteract`)
//! and its client prediction.
use super::Game;
use super::level_sounds::LevelSound;
use horizon_client::{items::ItemStack, mobs::{Species, allay}};
use mod_api::client::first_person::Hand;
use mod_api::SoundSource;

impl Game<'_> {
    /// `Allay.mobInteract`'s `level().playSound(player, this, sound, NEUTRAL,
    /// 2, 1)`, bound to allay `allay`. Vanilla leaves the player out because
    /// their client played it; Horizon's own client plays it from the
    /// packet, so every player gets it.
    fn play_allay_sound(&mut self, sound: &str, at: [f64; 3], allay: u32) {
        let sound = LevelSound::new(sound, at, 2.0, 1.0).in_source(SoundSource::Neutral);
        self.play_bound_sound(sound, allay, None);
    }
    /// `Allay.mobInteract`: duplicate when dancing with an amethyst shard,
    /// take the held item, or hand back what it holds.
    pub(super) fn interact_allay(&mut self, index: usize, item: Option<&ItemStack>) -> bool {
        let m = &self.remote_entities[index];
        if m.species() != Species::Allay || !m.alive() {
            return false;
        }
        let at = [m.position.x, m.position.y, m.position.z];
        let shard = item.is_some_and(|s| {
            self.items()
                .item_tags
                .get("minecraft:duplicates_allays")
                .is_some_and(|tag| tag.contains(&*s.item))
        });
        if m.extra.flag && shard && m.extra.egg_time == 0 {
            let pos = at;
            self.remote_entities[index].extra.egg_time = 6000;
            if let Some(id) = self.spawn_mob_raw("minecraft:allay", pos, true)
                && let Some(c) = self.remote_entities.iter_mut().find(|m| m.id == id)
            {
                c.extra.egg_time = 6000;
            }
            // Allay.duplicateAllay: entity event 18, three spawnHeartParticle.
            let id = self.remote_entities[index].id;
            self.mob_event_particles(id, 18);
            self.play_allay_sound("minecraft:block.amethyst_block.chime", at, id);
            self.consume_held();
            return true;
        }
        let acting = self.acting_uuid();
        let holds = m.extra.main_hand_id().is_some();
        if !holds && let Some(mut given) = item.cloned() {
            let m = &mut self.remote_entities[index];
            // `Allay.mobInteract` hands over one item, its components and all.
            given.count = 1;
            m.extra.set_main_hand_stack(Some(given));
            // `Allay.mobInteract` remembers LIKED_PLAYER by UUID.
            allay::like_player(m, acting);
            let id = m.id;
            self.play_allay_sound("minecraft:entity.allay.item_given", at, id);

            self.consume_held();
            return true;
        }
        // Allay.mobInteract takes the item back only through an empty main hand.
        if holds && item.is_none() && self.hand == Hand::Main {
            let m = &mut self.remote_entities[index];
            // The hand's stack keeps the components (a potion) the items came with.
            let held = m
                .extra
                .main_hand_stack()
                .cloned()
                .unwrap_or_else(|| ItemStack::new("minecraft:air", 1));
            m.extra.set_main_hand(None);
            let carried = m.extra.allay.as_mut().and_then(|st| st.inventory.take());
            allay::forget_liked_player(m);
            let id = m.id;
            self.play_allay_sound("minecraft:entity.allay.item_taken", at, id);

            // Allay.mobInteract ends with swing(MAIN_HAND, ...).
            self.remote_entities[index].swing();
            if let Some(stack) = carried {
                self.spawn_item(stack, at, [0.0, 0.2, 0.0], 40);
            }
            if let Some(rest) = self.add_stack_to_inventory(ItemStack { count: 1, ..held })
                && let Some(eye) = self.eye_position()
            {
                self.spawn_item(rest, eye, [0.0; 3], 0);
            }
            return true;
        }
        false
    }
}
