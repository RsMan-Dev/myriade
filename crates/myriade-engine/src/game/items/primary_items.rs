//! `ItemPart::is_primary_item_for` (`IItemExtension.isPrimaryItemFor`): the logical server asks
//! the answers once when its parts install and stores them on the enchantment registry snapshot
//! it plays with, which every enchantment selection reads (`horizon_items::primary_items`).
use super::*;

impl Game<'_> {
    /// Ask the `is_primary_item_for` parts of each item once per registered enchantment, with
    /// the item's default stack, and store the answers that differ from vanilla's
    /// `Enchantment.isPrimaryItem` on this server's enchantment snapshot for selection to read:
    /// the table's offers and clues, loot `enchant_with_levels`, trades and spawn gear all
    /// choose inside a roll that holds the level's random, where no part can be asked. Run at
    /// world open and on `/reload`, after the parts install, and only on the logical server: a
    /// client's tag install never asks, writes or clears them, so on an integrated server the
    /// client's refresh leaves the server's answers alone. With no such part the snapshot's
    /// answers are retired and selection is vanilla's.
    pub(in crate::game) fn resolve_primary_items(&self) {
        let mut answers = horizon_items::primary_items::PrimaryAnswers::new();
        for table in self.primary_answer_tables() {
            table.install(answers.clone());
        }
    }
    /// Leave the world's primary-item answers as its world closes. A client holds none.
    pub(in crate::game) fn clear_world_parts(&self) {
        for table in self.primary_answer_tables() {
            table.clear();
        }
    }
    /// The answer tables of the enchantment snapshots this game selects with: its stations'
    /// and its items' (loot), once when they are one registry.
    fn primary_answer_tables(
        &self,
    ) -> impl Iterator<Item = &horizon_items::primary_items::PrimaryAnswerTable> {
        let stations = &self.stations().enchantments;
        let items = &self.items().enchantments;
        let other = (!std::sync::Arc::ptr_eq(stations, items)).then_some(items);
        std::iter::once(stations)
            .chain(other)
            .map(|enchantments| enchantments.primary_answers())
    }
}
