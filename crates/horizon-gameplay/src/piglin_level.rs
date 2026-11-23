//! What piglin code calls on `ServerLevel` beyond [`MobLevel`]: the content tables it
//! reads and the questions only the game can answer.
use crate::mob_level::MobLevel;
use horizon_core::geometry::Vec3;

pub trait PiglinLevel: MobLevel {
    // The `Game` methods piglin code calls on itself. Each is the game's own method, so a mod
    // that listens to one still hears the moved code; the free functions of `piglins` are
    // what those methods run.
    fn item_in_tag(&self, tag: &str, item: &str) -> bool;
    fn piglin_loved(&self, item: &str) -> bool;
    fn item_blocks_attacks(&self, item: &str) -> bool;
    fn piglin_slot(&self, item: &str) -> usize;
    fn piglin_max_stack(&self, item: &str) -> u32;
    fn admire_seen_item(&mut self, index: usize, nearest: Option<f64>, loved: bool);
    /// Where `PiglinAi.throwItems` throws toward from `at`: one block above the feet of
    /// the nearest player within 16 blocks, the level's own player ahead of the guests,
    /// those alive and not spectators. `None` when no player qualifies.
    fn piglin_throw_target(&self, at: Vec3) -> Option<[f64; 3]>;
}
