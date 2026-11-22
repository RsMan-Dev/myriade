//! Evoker fangs (`EvokerFangs`): rising out of the floor after a delay,
//! biting what stands on them for 6 damage, sinking back after 22 ticks.
use super::Game;
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;

#[derive(Clone, Debug)]
pub(super) struct Fangs {
    /// Entity-owned registration metadata; content clones are detached.
    pub(super) attachment_lifetime: horizon_mod_runtime::attachments::AttachmentLifetime,
    pub(in crate::game) join: horizon_client::join_seq::JoinSeq,
    pub id: u32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub warmup: i32,
    pub owner: u32,
    /// `Entity.deltaMovement`, once a tick has read the loaded `Motion`.
    pub velocity: Option<Vec3>,
}

horizon_client::impl_joined!(Fangs);
horizon_mod_runtime::impl_attachment_lifetime!(Fangs);

/// `EvokerFangs.handleEntityEvent`: `nextFloat() * 0.2 + 0.85`.
pub(super) fn fang_attack_pitch(random: &mut impl RandomSource) -> f32 {
    random.next_float() * 0.2 + 0.85
}
#[derive(Default)]
pub(super) struct FangState {
    pub list: horizon_client::join_seq::ResidentList<Fangs>,
}
impl FangState {
    pub(in crate::game) fn perf_count(&self) -> usize {
        self.list.len()
    }
}
impl Game<'_> {

    pub(super) fn insert_fangs(&mut self, id: u32, at: [f64; 3], yaw: f32, delay: i32, owner: u32) {
        self.fangs.list.push(Fangs {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id,
            pos: at,
            yaw,
            warmup: delay,
            owner,
            velocity: None,
        });
    }
    /// `EvokerFangs` of an evoker's spell (`owner`: its id, 0 for none): the ECS spawns them and bites; the host shows the bite.
    pub(in crate::game) fn add_fangs(&mut self, at: [f64; 3], yaw: f32, delay: i32, owner: u32) {
        let owner = self.ecs.entity_of(owner).map(horizon_ecs_proto::targeting::Who::Entity);
        let mut next = self.next_entity;
        self.ecs_with(|ecs, mw| ecs.spawn_fangs(mw, at, yaw, delay, owner, &mut || {
            next += 1;
            next
        }));
        self.next_entity = next;
    }
    /// The bite's attack sound at `at` (`EvokerFangs.attack`: entity event 4, which each player's client
    /// answers with the sound), its pitch from the host's random. Broadcast to the players.
    pub(in crate::game) fn play_fang_bite(&mut self, at: [f64; 3]) {
        let pitch = fang_attack_pitch(&mut self.world.random);
        self.audio.play("minecraft:entity.evoker_fangs.attack", Some(at), 1.0, pitch);
    }
    pub(super) fn fangs_to_nbt(&self, f: &Fangs, uuid: [i32; 4]) -> horizon_core::nbt::Tag {
        horizon_client::entity_nbt::evoker_fangs_to_nbt(
            Vec3::new(f.pos[0], f.pos[1], f.pos[2]),
            f.yaw,
            f.warmup,
            self.persist.uuids.get(&f.owner).copied(),
            uuid,
            self.persist.raw.get(&f.id),
            f.velocity,
        )
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
