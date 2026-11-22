//! The totem of undying (`LivingEntity.checkTotemDeathProtection`: a hand's
//! item carrying the `death_protection` component, whatever item it is, and
//! entity event 35 on the client).
use super::Game;
use horizon_client::items::ItemStack;

const TOTEM: &str = "minecraft:totem_of_undying";

/// `EntityEvent.TOTEM_OF_UNDYING`.
pub(super) const TOTEM_EVENT: i8 = 35;

impl Game<'_> {
    /// Whether the stack has `DataComponents.DEATH_PROTECTION`, whatever
    /// item it is: the totem of undying is only one item that carries it.
    fn has_death_protection(&self, s: &ItemStack) -> bool {
        self.items().get(&s.item).is_some_and(|i| i.death_protection.is_some())
    }
    /// The `death_effects` of the stack's `minecraft:death_protection`.
    fn death_effects(&self, s: &ItemStack) -> Vec<horizon_client::items::ConsumeEffect> {
        self.items()
            .get(&s.item)
            .and_then(|i| i.death_protection.clone())
            .unwrap_or_default()
    }
    /// No cache of the mobs' hands: the ECS reads their held items when they
    /// are hurt. RESTE-SANS-ECS: the death protection of a mob (`checkTotemDeathProtection`)
    /// has no ECS rule yet; the call in game.rs:3650 goes with it.
    pub(super) fn sync_mob_death_protection(&mut self) {}
    /// Whether either hand holds a death-protection item, for the next hurt:
    /// the window's player once a tick, a server's player for each of its
    /// turns.
    pub(super) fn update_totem_ready(&mut self) {
        let ready = self.held().is_some_and(|s| self.has_death_protection(s))
            || self
                .offhand
                .as_ref()
                .is_some_and(|s| self.has_death_protection(s));
        let off = !self.held().is_some_and(|s| self.has_death_protection(s));
        if let Some(p) = self.player.as_mut() {
            p.health.totem_ready = ready;
            p.health.totem_off_hand = off;
        }
    }
    /// The server's half once a totem fired: use it up (main hand first),
    /// count it, set health to 1, then clear and apply its effects. Returns
    /// the player's body, for entity event 35, which the caller broadcasts,
    /// with the protector copy the activation animation shows.
    pub(super) fn apply_totem(&mut self) -> Option<Body> {
        if !self.player.as_ref()?.health.totem_used {
            return None;
        }
        // The mods hear a server player's totem before it is used up.
        if let (Some(uuid), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            self.native_player_totem(uuid, &name);
        }
        let p = self.player.as_mut()?;
        p.health.totem_used = false;
        let _ = p.set_health(1.0, false);
        let body = Body {
            feet: [p.position.x, p.position.y, p.position.z],
        };
        // `checkTotemDeathProtection`: the selected slot first, then the
        // offhand. The copy of that stack is the protector (`protectionItem`)
        // the use is counted and the activation animation shown with, taken
        // before the use takes one off the stack.
        let main = self.held().is_some_and(|s| self.has_death_protection(s));
        let cell = if main {
            &mut self.inventory[self.selected]
        } else {
            &mut self.offhand
        };
        let protector = cell
            .as_ref()
            .cloned()
            .unwrap_or_else(|| ItemStack::new(TOTEM, 1));
        if let Some(s) = cell.as_mut() {
            s.count -= 1;
            if s.count == 0 {
                *cell = None;
            }
        }
        self.award_stat("used", &protector.item, 1);
        self.trigger_item("minecraft:used_totem", &protector);
        // ItemStack.causeUseVibration: the totem's use effects make one.
        let ctx = self.player_context(None);
        self.post_game_event("minecraft:item_interact_finish", body.feet, ctx);
        // `DeathProtection.applyEffects`: the component's own `death_effects`,
        // read from the protector copy (the totem's clear-then-effects, or a
        // custom list; `death_protection={}` has none).
        for effect in self.death_effects(&protector) {
            self.apply_consume_effect(&protector, &effect, None);
        }
        Some(body)
    }
    /// The window's player's totem: the server's half only; the client draws nothing.
    pub(super) fn tick_totem(&mut self) {
        self.apply_totem();
    }
    /// `GameRenderer.tick`: the activated item counts down.
    pub(super) fn tick_item_activation(&mut self) {
        if self.player.is_some() {
            self.item_activation.tick();
        }
    }
}



/// Where an entity event's entity stands and its size, and, for the event
/// this window's own player fires after the server half used a stack up, the
/// protector copy the activation animation shows.
#[derive(Clone)]
pub(super) struct Body {
    pub feet: [f64; 3],
}
