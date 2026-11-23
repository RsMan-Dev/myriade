//! The loot rolls of `/loot ... kill`.

use super::Game;
use super::commands::Target;
use myriade_engine::items::ItemStack;

/// `/loot ... kill`'s roll (`LootCommand.dropKillLoot`).
pub(in crate::game) struct KillRoll<'a> {
    /// The target's type and its table.
    pub(in crate::game) kind: &'a str,
    pub(in crate::game) table: &'a str,
    pub(in crate::game) target: Target,
    /// `LootContextParams.LAST_DAMAGE_PLAYER` is set.
    pub(in crate::game) by_player: bool,
    pub(in crate::game) held: Option<&'a ItemStack>,
    pub(in crate::game) keep_empty: bool,
}

impl Game<'_> {
    /// The drops of `/loot ... kill`, from the table's random sequence, else
    /// the level random.
    pub(in crate::game) fn roll_kill_loot(&mut self, kill: KillRoll<'_>) -> Vec<ItemStack> {
        let KillRoll {
            kind,
            table,
            target,
            by_player,
            held,
            keep_empty,
            ..
        } = kill;
        // LootCommand.dropKillLoot supplies the selected live mob as THIS_ENTITY.
        let live_target = match target {
            Target::Mob(id) => self.ecs.loot_entity(id),
            _ => None,
        };
        let loot = live_target.as_ref().and_then(|_| {
            myriade_engine::structure_blocks::loot_data(
                &self.options.resources,
                &self.pack_files(),
                self.res.item_defaults(),
            )
        });
        let matcher = |predicate: &serde_json::Value, entity: &horizon_world::loot::LootEntity| {
            loot.as_ref()
                .is_some_and(|data| data.entity_matches(predicate, Some(entity)))
        };
        let live_target = live_target.as_ref().filter(|_| loot.is_some());
        let sequence = world_items!(self).loot_sequence(table);
        let items = world_items!(self);
        match sequence {
                Some(id) => {
                    let random = self.rand_sequences.get(&id, self.level.seed);
                    if let Some(target) = live_target {
                        items.entity_drops_with_target(
                            kind, by_player, held, target, &matcher, random, keep_empty,
                        )
                    } else {
                        items.entity_drops_for_command(kind, by_player, held, random, keep_empty)
                    }
                }
                None => {
                    if let Some(target) = live_target {
                        items.entity_drops_with_target(
                            kind,
                            by_player,
                            held,
                            target,
                            &matcher,
                            &mut self.world.random,
                            keep_empty,
                        )
                    } else {
                        items.entity_drops_for_command(
                            kind,
                            by_player,
                            held,
                            &mut self.world.random,
                            keep_empty,
                        )
                    }
                }
        }
    }
}
