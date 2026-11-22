use super::Game;
use horizon_core::geometry::Vec3;

pub(in crate::game) fn teleport_player(game: &mut Game<'_>, to: Vec3) {
    game.stop_riding();
    let from = game.player.as_ref().map(|player| player.position);
    if let Some(player) = game.player.as_mut() {
        player.position = to;
        player.velocity = Vec3::ZERO;
        player.fall_distance = 0.0;
        player.impulse.reset();
        player.teleported = true;
        player.teleported_delta = [false; 3];
    }
    game.previous_position = to;
    if let (Some(from), Ok(id)) = (from, u32::try_from(game.own_entity_id())) {
        game.entity_section_step(id, [from.x, from.y, from.z], [to.x, to.y, to.z]);
    }
}
