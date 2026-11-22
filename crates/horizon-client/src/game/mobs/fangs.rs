//! Evoker fangs (`EvokerFangs`): rising out of the floor after a delay,
//! biting what stands on them for 6 damage, sinking back after 22 ticks.
//! The fangs are ECS entities; the host keeps no list of them.
use super::Game;
use horizon_core::random::RandomSource;

const FANGS: &str = "minecraft:evoker_fangs";

/// `EvokerFangs.handleEntityEvent`: `nextFloat() * 0.2 + 0.85`.
pub(super) fn fang_attack_pitch(random: &mut impl RandomSource) -> f32 {
    random.next_float() * 0.2 + 0.85
}

impl Game<'_> {
    /// `EvokerFangs` of an evoker's spell (`owner`: its id, 0 for none): the ECS spawns them and bites; the host shows the bite.
    /// Returns the network id of the fang.
    pub(in crate::game) fn add_fangs(&mut self, at: [f64; 3], yaw: f32, delay: i32, owner: u32) -> u32 {
        let owner = self.ecs.entity_of(owner).map(horizon_ecs_proto::targeting::Who::Entity);
        let mut next = self.next_entity;
        let id = self.ecs_with(|ecs, mw| ecs.spawn_fangs(mw, at, yaw, delay, owner, &mut || {
            next += 1;
            next
        }));
        self.next_entity = next;
        id
    }
    /// The bite's attack sound at `at` (`EvokerFangs.attack`: entity event 4, which each player's client
    /// answers with the sound), its pitch from the host's random. Broadcast to the players.
    pub(in crate::game) fn play_fang_bite(&mut self, at: [f64; 3]) {
        let pitch = fang_attack_pitch(&mut self.world.random);
        self.audio.play("minecraft:entity.evoker_fangs.attack", Some(at), 1.0, pitch);
    }
    /// The network ids of the evoker fangs the ECS holds.
    pub(in crate::game) fn fang_ids(&self) -> Vec<u32> {
        self.ecs.species_ids(FANGS)
    }
    /// Removes the fang `id` from the ECS.
    pub(in crate::game) fn kill_fang(&mut self, id: u32) {
        if let Some(e) = self.ecs.entity_of(id) {
            self.ecs.despawn(e);
        }
    }
    /// Moves the fang `id` to `at`, and turns it to `yaw` when one is given.
    pub(in crate::game) fn move_fang(&mut self, id: u32, at: [f64; 3], yaw: Option<f32>) {
        let Some(e) = self.ecs.entity_of(id) else {
            return;
        };
        self.ecs_with(|ecs, mw| ecs.teleport(mw, e, at));
        if let Some(yaw) = yaw {
            let pitch = self.ecs.rotation(id).map_or(0.0, |r| r[1]);
            self.ecs.set_rotation(e, yaw, pitch);
        }
    }
    pub(super) fn loaded_fangs_owner(&self, tag: &horizon_core::nbt::Tag) -> u32 {
        match tag.get("Owner") {
            Some(horizon_core::nbt::Tag::IntArray(v)) => v.as_slice().try_into().ok(),
            _ => None,
        }
        .and_then(|u| self.mob_by_uuid(u))
        .unwrap_or(0)
    }
}
#[cfg(test)]
mod tests {
    use super::fang_attack_pitch;
    use horizon_core::random::LegacyRandom;

    #[test]
    fn fang_attack_pitch_matches_the_vanilla_span() {
        // Java Random seed 1, first nextFloat() * 0.2 + 0.85, in f32.
        let mut random = LegacyRandom::new(1);
        let pitch = fang_attack_pitch(&mut random);
        assert!((pitch - 0.996_175_65).abs() < 1.0e-6, "{pitch}");
        assert!((0.85..1.05).contains(&pitch));
    }
}
