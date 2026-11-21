//! What the dragon's death builds (`activate_exit_portal`, `place_podium`, `place_gateway` of Horizon's `world/end.rs`; the egg of
//! `dragon_killed`, D:3035): block writes asked through `blocks::set_state`, one batch at the act, nothing per tick.
use crate::blocks::{set_state, world_of};
use crate::dragon::fight::EndFight;
use crate::rules::dragon_portal as rule;
use bevy_ecs::prelude::*;

/// `EndPodiumFeature` at `origin` (the portal level); `active` fills the rim with the portal.
pub fn place_podium(w: &mut World, origin: [i32; 3], active: bool) {
    for y in origin[1] - 1..=origin[1] + 32 {
        for z in origin[2] - 4..=origin[2] + 4 {
            for x in origin[0] - 4..=origin[0] + 4 {
                let Some(name) = rule::podium_block(origin, [x, y, z], active) else { continue };
                set_state(w, [x, y, z], name, &[]);
            }
        }
    }
    for dy in 0..4 {
        set_state(w, [origin[0], origin[1] + dy, origin[2]], "minecraft:bedrock", &[]);
    }
    let torches: [([i32; 2], &'static [(&'static str, &'static str)]); 4] = [
        ([0, -1], &[("facing", "north")]),
        ([0, 1], &[("facing", "south")]),
        ([-1, 0], &[("facing", "west")]),
        ([1, 0], &[("facing", "east")]),
    ];
    for (d, props) in torches {
        set_state(w, [origin[0] + d[0], origin[1] + 2, origin[2] + d[1]], "minecraft:wall_torch", props);
    }
}

/// `spawnExitPortal(true)` after the kill: at the known origin, else at the default portal level.
pub fn activate_exit_portal(w: &mut World) {
    let origin = w.resource::<EndFight>().exit_portal.unwrap_or([0, 61, 0]);
    place_podium(w, origin, true);
}

/// `EndGatewayFeature.place`: the gateway block in a bedrock cross.
pub fn place_gateway(w: &mut World, origin: [i32; 3]) {
    for y in -2i32..=2 {
        for z in -1..=1 {
            for x in -1..=1 {
                let name = rule::gateway_block([x, y, z]);
                set_state(w, [origin[0] + x, origin[1] + y, origin[2] + z], name, &[]);
            }
        }
    }
}

/// The egg on the first kill: on the highest solid block of the column (0, 0), plus one. The podium's bedrock pillar (four high from
/// the portal's floor) is queued, not yet in the world: it counts as the top (Horizon reads the heightmap after placing it).
pub fn place_egg(w: &mut World) {
    let pillar = w.resource::<EndFight>().exit_portal.unwrap_or([0, 61, 0])[1] + 4;
    let mw = world_of(w);
    let y = (0..256).rev().find(|&y| {
        let f = mw.facts.get(mw.world.block(0, y, 0));
        f.full_collision || f.legacy_solid
    });
    set_state(w, [0, y.map_or(64, |y| y + 1).max(pillar), 0], "minecraft:dragon_egg", &[]);
}
