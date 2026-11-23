//! The fishing hook of the ECS (`HookEvent`, `ecs_bridge/hook.rs`): the acting player's cast and reel, the bite and the hooking the
//! events announce, and the catch of a reel (the loot, the rod's damage, the experience, the statistics) with the pull of what it held.
use super::super::Game;
use myriade_engine::{advancements::EntityView, items::ItemStack};
use horizon_core::{geometry::Vec3, random::LevelRandom};
use horizon_ecs_proto::{host_out_hook::HookEvent, hook_extract::Retrieve, targeting::Who as EcsWho};
use std::sync::Arc;

/// The table a catch rolls where no mod's fishing medium names one.
const FISHING_TABLE: &str = "minecraft:gameplay/fishing";

/// `FishingHook.retrieve`: the table's named random sequence, or the level
/// random when a datapack table does not name one.
#[allow(clippy::too_many_arguments)]
fn fishing_loot(
    loot: &horizon_world::loot::LootData,
    table: &str,
    sequences: &mut myriade_engine::random_sequences::RandomSequences,
    level_random: &mut LevelRandom,
    world_seed: i64,
    luck: f32,
    biome: Option<&str>,
    open_water: bool,
) -> Vec<horizon_world::loot::LootStack> {
    match loot.random_sequence(table) {
        Some(sequence) => loot.roll(table, sequences.get(sequence, world_seed), luck, biome, Some(open_water)),
        None => loot.roll(table, level_random, luck, biome, Some(open_water)),
    }
}

/// `FishingHook.pullEntity`: add a tenth of the vector from the hook to its owner.
fn pull_hooked(velocity: Vec3, hook: Vec3, owner: Vec3) -> Vec3 {
    Vec3::new(
        velocity.x + (owner.x - hook.x) * 0.1,
        velocity.y + (owner.y - hook.y) * 0.1,
        velocity.z + (owner.z - hook.z) * 0.1,
    )
}

impl Game<'_> {
    /// The acting player's entity id, which names their hook's owner in the ECS: the host's, or the guest's.
    fn ecs_fisher(&self) -> Option<u32> {
        match self.lan_acting {
            None => self.player.as_ref().map(|p| p.entity_id as u32),
            Some(conn) => self.lan.as_ref()?.hub.guests.iter().find(|g| g.conn == conn).map(|g| g.entity_id as u32),
        }
    }

    /// The acting player's hook out, by network id: the ECS's owner index is the one state of it.
    pub(in crate::game) fn ecs_own_hook(&self) -> Option<u32> {
        self.ecs.hook_of(EcsWho::Player(self.ecs_fisher()?))
    }

    /// Casts the acting player's hook from `at` with `velocity`, its Luck and Lure levels; the id it gets is the one hook out.
    pub(in crate::game) fn ecs_cast_hook(&mut self, at: [f64; 3], velocity: [f64; 3], luck: u8, lure: u8) {
        let Some(owner) = self.ecs_fisher() else {
            return;
        };
        let mut next = self.next_entity;
        let id = self.ecs_with(|ecs, mw| {
            ecs.cast_hook(mw, owner, at, velocity, luck, lure, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        let _ = id;
    }

    /// Reels the acting player's hook in; returns the rod's damage (0 when no hook is out).
    pub(in crate::game) fn ecs_reel_hook(&mut self, rod: &ItemStack) -> u32 {
        let (Some(fisher), Some(owner)) = (self.ecs_fisher(), self.player.as_ref().map(|p| p.position)) else {
            return 0;
        };
        let Some(r) = self.ecs_with(|ecs, mw| ecs.reel_hook(mw, EcsWho::Player(fisher))) else {
            return 0;
        };
        let at = [r.at.x, r.at.y, r.at.z];
        let mut rod_damage = r.damage_item as i32;
        let pulled = |v: Vec3| pull_hooked(v, r.at, owner);
        match r.pulled {
            Some(EcsWho::Entity(e)) => {
                let id = self.ecs_id_of(e);
                // CriteriaTriggers.FISHING_ROD_HOOKED with the creature hooked.
                if let Some(view) = self.ecs_host_view(id) {
                    self.trigger_fishing_rod_hooked(rod, &view, &[]);
                }
                self.ecs_pull(id, pulled);
            }
            Some(EcsWho::Player(id)) => {
                self.with_player(id, |g| {
                    if let Some(p) = g.player.as_mut() {
                        p.velocity = pulled(p.velocity);
                    }
                });
            }
            None => {}
        }
        if r.caught {
            rod_damage = self.ecs_catch(rod, &r, owner, at, rod_damage);
        }
        rod_damage.max(0) as u32
    }

    /// The catch of a reel: the mods hear it first, then the loot falls at the hook with its experience and statistics.
    /// Returns the rod's damage, which a mod may change or cancel the rewards with.
    fn ecs_catch(&mut self, rod: &ItemStack, r: &Retrieve, owner: Vec3, at: [f64; 3], rod_damage: i32) -> i32 {
        let biome = self.biome_name_at(r.at);
        let Some(loot) = self.server_loot.get() else {
            return rod_damage;
        };
        // `withLuck(this.luck + owner.getLuck())`: the rod's Luck of the Sea plus the owner's luck attribute.
        let owner_luck = self
            .player
            .as_mut()
            .and_then(|p| p.attribute("minecraft:luck").ok())
            .unwrap_or(0.0) as f32;
        let items = fishing_loot(
            &loot,
            FISHING_TABLE,
            &mut self.rand_sequences,
            &mut self.world.random,
            self.level.seed,
            f32::from(r.luck) + owner_luck,
            biome.as_deref(),
            r.open_water,
        );
        let caught: Vec<ItemStack> = items
            .into_iter()
            .map(|stack| {
                let mut item = ItemStack {
                    item: Arc::from(stack.item.as_str()),
                    count: stack.count.max(1) as u32,
                    damage: stack.damage.max(0) as u32,
                    extra: None,
                };
                for (id, level) in &stack.enchantments {
                    if &*item.item == "minecraft:enchanted_book" {
                        myriade_engine::item_data::set_level(&mut item.data_mut().stored_enchantments, id, (*level).max(0) as u32);
                    } else {
                        item.enchant(id, (*level).max(0) as u32);
                    }
                }
                if let Some(potion) = &stack.potion {
                    item = item.with_potion(potion);
                }
                item
            })
            .collect();
        // CriteriaTriggers.FISHING_ROD_HOOKED: the hook itself is the entity.
        let bobber = EntityView {
            kind: "minecraft:fishing_bobber".into(),
            position: at,
            fishing_open_water: Some(r.open_water),
            ..Default::default()
        };
        self.trigger_fishing_rod_hooked(rod, &bobber, &caught);
        for item in caught.iter().cloned() {
            let fish = self
                .res
                .items()
                .item_tags
                .get("minecraft:fishes")
                .is_some_and(|tags| tags.contains(&*item.item));
            let (dx, dy, dz) = (owner.x - r.at.x, owner.y - r.at.y, owner.z - r.at.z);
            let v = [
                dx * 0.1,
                dy * 0.1 + (dx * dx + dy * dy + dz * dz).sqrt().sqrt() * 0.08,
                dz * 0.1,
            ];
            self.spawn_item(item, at, v, 0);
            // One orb per fished object, its value drawn by the hook; vanilla builds it directly, not through `award`.
            let xp = self.ecs.hook_xp();
            self.add_orb([owner.x, owner.y + 0.5, owner.z + 0.5], xp);
            if fish {
                self.award_custom_stat("fish_caught", 1);
            }
        }
        rod_damage
    }

    pub(in crate::game) fn ecs_hook_event(&mut self, ev: HookEvent) {
        match ev {
            HookEvent::Bite { hook, pitch } => {
                let id = self.ecs_id_of(hook);
                if let Some(at) = self.ecs_view(id).map(|v| v.position) {
                    self.hook_bite_effects(at, pitch);
                }
            }
            HookEvent::Attached { hook, by, .. } => {
                let id = self.ecs_id_of(hook);
                let Some(at) = self.ecs_view(id).map(|v| v.position) else {
                    return;
                };
                // Projectile.onHit on an entity: PROJECTILE_LAND.
                let by = self.host_who(by);
                self.projectile_landed_event(id, Some(by), at);
            }
        }
    }

    /// The weather changed: the ECS hooks wake to it.
    pub(in crate::game) fn ecs_weather_changed(&mut self) {
        self.ecs_with(|ecs, mw| ecs.weather_changed(mw));
    }

}
