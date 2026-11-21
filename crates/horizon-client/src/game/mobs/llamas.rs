//! Llamas on the game side: `AbstractChestedHorse.mobInteract` and
//! `Llama.handleEating` (wheat and hay), chests, carpets, shearing a carpet
//! off and riding.
use super::Game;
use horizon_client::mobs::{equine, llama};
use horizon_core::random::RandomSource;

impl Game<'_> {
    fn llama_sound(&mut self, index: usize, sound: &str, volume: f32) {
        let at = self.remote_entities[index].center();
        let pitch = (self.world.random.next_float() - self.world.random.next_float()) * 0.2 + 1.0;
        self.audio
            .play(&format!("minecraft:{sound}"), Some(at), volume, pitch);
    }
    /// `Llama.handleEating`: wheat and hay bales heal, age up a baby, raise
    /// the temper and, from a hay bale, put a tame adult in love.
    fn feed_llama(&mut self, index: usize, item: &str) -> bool {
        let (age_up, temper, heal) = match item {
            "minecraft:wheat" => (10, 3, 2.0),
            "minecraft:hay_block" => (90, 6, 10.0),
            _ => return false,
        };
        let m = &mut self.remote_entities[index];
        let mut used = false;
        let mut loved = false;
        if item == "minecraft:hay_block" && m.extra.tame && m.extra.age == 0 && m.extra.in_love <= 0
        {
            m.extra.in_love = 600;
            m.extra.love_player = true;
            used = true;
            loved = true;
        }
        if m.health < m.attrs.max_health {
            m.heal(&self.loop_context, heal);
            used = true;
        }
        if m.is_baby() && !m.extra.age_locked {
            m.extra.age = (m.extra.age + age_up * 20).min(0);
            used = true;
        }
        if (used || !m.extra.tame) && equine::temper(m) < llama::MAX_TEMPER {
            equine::modify_temper(m, temper);
            used = true;
        }
        if !used {
            return false;
        }
        // Animal.setInLove: entity event 18.
        if loved {
            let id = self.remote_entities[index].id;
            self.love_cause.insert(id, self.lan_acting);
            let kind = self.remote_entities[index].kind.id.clone();
            self.native_entity_enter_love_mode(id, &kind, self.lan_acting_uuid);
            self.mob_event_particles(id, 18);
        }
        self.llama_sound(index, "entity.llama.eat", 1.0);
        self.consume_held();
        true
    }
    /// `AbstractChestedHorse.mobInteract` and `AbstractHorse.mobInteract` for
    /// llamas and trader llamas. Returns whether the click was used.
    pub(super) fn interact_llama(&mut self, index: usize, item: &str) -> bool {
        let m = &self.remote_entities[index];
        if !m.alive() || Some(m.id) == self.riding {
            return false;
        }
        let sneaking = self.player.as_ref().is_some_and(|p| p.shift_key_down);
        let (baby, tamed, vehicle) = (m.is_baby(), m.extra.tame, m.rider.is_some());
        let open_inventory = !baby && tamed && sneaking;
        if !vehicle && !open_inventory && !item.is_empty() {
            if self.remote_entities[index].kind.food.iter().any(|f| f == item) {
                return self.feed_llama(index, item);
            }
            if !tamed {
                let mut events = Vec::new();
                equine::make_mad(&mut self.remote_entities[index], &mut events);
                let _ = self.handle_mob_events(events);
                return true;
            }
            if !self.remote_entities[index].extra.chested && item == "minecraft:chest" {
                self.remote_entities[index].extra.chested = true;
                self.llama_sound(index, "entity.llama.chest", 1.0);
                self.consume_held();
                return true;
            }
        }
        // AbstractHorse.mobInteract.
        if vehicle || baby {
            return false;
        }
        if tamed && sneaking {
            // AbstractHorse.openCustomInventoryScreen.
            return true;
        }
        let m = &self.remote_entities[index];
        let carpet = |i: &str| i.ends_with("_carpet") && i.starts_with("minecraft:");
        if !item.is_empty() {
            if item == "minecraft:shears" && m.extra.body_id().is_some_and(carpet) {
                // Equippable.canBeSheared: the carpet comes off.
                let at = m.center();
                let carpet = self.remote_entities[index].take_body_equipment().unwrap();
                self.audio
                    .play("minecraft:item.llama_carpet.unequip", Some(at), 1.0, 1.0);
                self.spawn_item(carpet, at, [0.0, 0.2, 0.0], 10);
                self.damage_held_tool();
                let id = self.remote_entities[index].id;
                let pos = self.remote_entities[index].position;
                self.mob_equip_event(id, [pos.x, pos.y, pos.z], false);
                return true;
            }
            if carpet(item) && m.extra.body_id().is_none() {
                let held = self.inventory[self.selected].clone().map(|mut stack| {
                    stack.count = 1;
                    stack
                });
                self.remote_entities[index].extra.set_body_stack(held);
                self.llama_sound(index, "entity.llama.swag", 1.0);
                self.consume_held();
                let id = self.remote_entities[index].id;
                let pos = self.remote_entities[index].position;
                self.mob_equip_event(id, [pos.x, pos.y, pos.z], true);
                return true;
            }
        }
        // TraderLlama.doPlayerRide: not while a wandering trader leads it.
        if self.is_led_by_trader(index) {
            return true;
        }
        // doPlayerRide; mounting drops a lead.
        let id = self.remote_entities[index].id;
        if !self.native_may_board(id) {
            return true;
        }
        self.break_leash(id, true, false);
        let m = &mut self.remote_entities[index];
        m.extra.flag = false;
        m.extra.counter = 0;
        self.yaw = m.yaw;
        self.mount.reset_jump();
        self.start_riding(id);
        true
    }
    fn is_led_by_trader(&self, index: usize) -> bool {
        let m = &self.remote_entities[index];
        m.species() == horizon_client::mobs::Species::TraderLlama
            && matches!(
                m.extra.leash,
                Some(horizon_client::mobs::leash::Holder::Mob(id))
                    if self.remote_entities.iter().any(|o| o.id == id
                        && o.species() == horizon_client::mobs::Species::WanderingTrader)
            )
    }
}
