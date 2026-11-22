//! Scoped Player.attack follow-up probe; invoked by the dispenser's headless world.

#[cfg(test)]
mod tests {
    /// Invoke the actual attack methods in an empty scratch Game, without
    /// starting a server, opening a world or entering the game loop.
    #[test]
    #[ignore = "needs prepared resources and HORIZON_SPECIAL_ATTACK_CASES=INPUT,OUTPUT"]
    fn special_attack_cases_without_world_startup() {
        let spec = std::env::var("HORIZON_SPECIAL_ATTACK_CASES").expect("case paths required");
        let mut resources = None;
        let mut game = crate::game::game_tests::tests::server_game(&mut resources);
        game.build_registries();
        game.dispense_fixture(&spec).unwrap();
    }
}
