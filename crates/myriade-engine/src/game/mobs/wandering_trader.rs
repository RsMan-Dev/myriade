//! `WanderingTraderSpawner`: every 1200 ticks the spawn delay counts down;
//! when it runs out a trader may appear near the player (or the nearest
//! bell), bringing two llamas on leads, and wanders off after two days.
use super::{Game, creatures::Creatures};
use myriade_engine::save::write_dat;
use horizon_ecs_proto::behaviours::wander_to_position;
use horizon_ecs_proto::despawn_delay;
use horizon_ecs_proto::leash::Holder as EcsHolder;
use horizon_ecs_proto::spots::Home;
use horizon_gameplay::wandering_trader::TraderLevel;

pub(super) use horizon_gameplay::wandering_trader::Spawner;

impl Creatures {
    /// A fresh spawner for a newly opened world.
    pub(super) fn reset_trader(&mut self) {
        self.trader = Spawner::default();
    }
}
impl Game<'_> {
    fn trader_path(&self) -> std::path::PathBuf {
        self.world_root
            .join("data")
            .join("minecraft")
            .join("wandering_trader.dat")
    }
    pub(super) fn save_wandering_trader(&mut self) {
        let Some(tag) = self.creatures.trader.to_save() else {
            return;
        };
        if write_dat(&self.trader_path(), tag).is_ok() {
            self.creatures.trader.saved();
        }
    }
    /// `WanderingTraderSpawner.spawn`.
    fn spawn_trader(&mut self) -> bool {
        horizon_gameplay::wandering_trader::spawn(self)
    }
    /// `tryToSpawnLlamaFor`: a trader llama near the trader, on its lead.
    fn spawn_trader_llama(&mut self, trader: u32, radius: i32) {
        horizon_gameplay::wandering_trader::spawn_llama(self, trader, radius);
    }
    /// `findSpawnPositionNear`: ten tries at a column near the reference
    /// where the trader could stand.
    fn find_trader_spawn_near(&mut self, reference: [i32; 3], radius: i32) -> Option<[i32; 3]> {
        horizon_gameplay::wandering_trader::find_spawn_near(self, reference, radius)
    }
    /// `hasEnoughSpace`: no collision in the 2 by 3 by 2 box.
    fn trader_has_space(&self, pos: [i32; 3]) -> bool {
        horizon_gameplay::wandering_trader::has_space(&self.world, self.res.facts(), pos)
    }
    /// The closest bell (`PoiTypes.MEETING`) within `range` blocks.
    fn nearest_bell(&self, from: [i32; 3], range: i32) -> Option<[i32; 3]> {
        horizon_gameplay::wandering_trader::nearest_bell(
            &self.world,
            self.res.facts(),
            from,
            range,
        )
    }
    /// `setDespawnDelay(48000)` on the trader `trader` the spawner made: the spawner sets it, not the species (a summoned trader has none).
    fn start_trader_delay(&mut self, trader: u32) {
        let Some(e) = self.ecs.entity_of(trader) else {
            return;
        };
        if let Some(w) = self.ecs.world_mut() {
            despawn_delay::start(w, e);
        }
    }
    /// `WanderingTrader.startVisit`: the trader walks to `reference` and keeps within 16 of it.
    fn visit_trader(&mut self, trader: u32, reference: [i32; 3]) {
        let Some(e) = self.ecs.entity_of(trader) else {
            return;
        };
        let point = [f64::from(reference[0]) + 0.5, f64::from(reference[1]), f64::from(reference[2]) + 0.5];
        if let Some(w) = self.ecs.world_mut() {
            wander_to_position::send_to(w, e, point);
            w.entity_mut(e).insert(Home { center: reference, radius: 16 });
        }
    }
    /// `setLeashedTo(trader)` for a llama `tryToSpawnLlamaFor` made; its delay comes with its species at the birth.
    fn leash_trader_llama(&mut self, trader: u32, llama: u32) {
        let Some(t) = self.ecs.entity_of(trader) else {
            return;
        };
        self.ecs_set_leash(llama, Some(EcsHolder::Entity(t)));
    }
}

impl TraderLevel for Game<'_> {
    fn spawn_trader(&mut self) -> bool {
        Game::spawn_trader(self)
    }
    fn spawn_trader_llama(&mut self, trader: u32, radius: i32) {
        Game::spawn_trader_llama(self, trader, radius);
    }
    fn find_trader_spawn_near(&mut self, reference: [i32; 3], radius: i32) -> Option<[i32; 3]> {
        Game::find_trader_spawn_near(self, reference, radius)
    }
    fn trader_has_space(&self, pos: [i32; 3]) -> bool {
        Game::trader_has_space(self, pos)
    }
    fn nearest_bell(&self, from: [i32; 3], range: i32) -> Option<[i32; 3]> {
        Game::nearest_bell(self, from, range)
    }
    fn trader_spawned(&mut self, trader: u32, reference: [i32; 3]) {
        Game::start_trader_delay(self, trader);
        Game::visit_trader(self, trader, reference);
    }
    fn trader_llama_spawned(&mut self, trader: u32, llama: u32) {
        Game::leash_trader_llama(self, trader, llama);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::ecs::test_support::ecs_game_with;
    use horizon_ecs_proto::{behaviours::wander_to_position::WanderPoint, cow::now_of, despawn_delay::DespawnDelay, spots::Home};

    #[test]
    fn a_spawned_trader_visits_its_reference_and_its_llama_carries_the_lead() {
        let (mut game, trader) = ecs_game_with("minecraft:wandering_trader", [0.5, 64.0, 0.5]);
        TraderLevel::trader_spawned(&mut game, trader, [10, 64, 10]);
        let t = game.ecs.entity_of(trader).expect("the trader is in the ECS");
        let w = game.ecs.world().expect("the level is active");
        assert_eq!(w.get::<DespawnDelay>(t).expect("the trader's delay").due - now_of(w, t), 48000);
        assert_eq!(w.get::<WanderPoint>(t).expect("the visit point").0, [10.5, 64.0, 10.5]);
        let home = w.get::<Home>(t).expect("the home of the visit");
        assert_eq!((home.center, home.radius), ([10, 64, 10], 16));
        let llama = game.ecs_spawn("minecraft:trader_llama", [2.5, 64.0, 0.5], None).expect("a trader llama");
        TraderLevel::trader_llama_spawned(&mut game, trader, llama);
        let l = game.ecs.entity_of(llama).expect("the llama is in the ECS");
        let w = game.ecs.world().expect("the level is active");
        assert_eq!(w.get::<DespawnDelay>(l).expect("the llama's delay").due - now_of(w, l), 47999);
        assert!(matches!(game.ecs_leash_holder(llama), Some(EcsHolder::Entity(h)) if h == t));
    }
}
