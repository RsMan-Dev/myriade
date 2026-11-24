use crate::game::Game;
use horizon_core::random::LevelRandom;
use horizon_gameplay::commands::weather::WeatherHost;

impl WeatherHost for Game<'_> {
    fn command_random(&mut self) -> &mut LevelRandom {
        &mut self.world.random
    }
    fn set_weather_parameters(&mut self, clear: i32, time: i32, raining: bool, thundering: bool) {
        // `MinecraftServer.setWeatherParameters`.
        let before = (self.world.weather.raining, self.world.weather.thundering);
        let w = &mut self.world.weather;
        w.clear_time = clear;
        w.rain_time = time;
        w.thunder_time = time;
        w.raining = raining;
        w.thundering = thundering;
        if before != (raining, thundering) {
            self.ecs_weather_changed();
        }
    }
}
