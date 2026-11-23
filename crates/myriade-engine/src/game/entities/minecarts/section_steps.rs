use super::*;

/// The rider's step on the cart `id`: the player sits at the cart's place in the ECS (`SEAT` above it), without velocity. None when the
/// cart has no view or nobody is in the host's player.
pub(super) fn seat_rider(game: &mut Game<'_>, id: u32) -> Option<(Vec3, Vec3)> {
    let [x, y, z] = game.ecs_view(id)?.position;
    let player = game.player.as_mut()?;
    let player_from = player.position;
    game.previous_position = player.position;
    player.position = Vec3::new(x, y + SEAT, z);
    player.velocity = Vec3::ZERO;
    player.fall_distance = 0.0;
    Some((player_from, player.position))
}

pub(super) fn tell_player(game: &mut Game<'_>, step: Option<(Vec3, Vec3)>) {
    if let Some((from, to)) = step
        && let Ok(id) = u32::try_from(game.own_entity_id())
    {
        game.entity_section_step(id, [from.x, from.y, from.z], [to.x, to.y, to.z]);
    }
}
