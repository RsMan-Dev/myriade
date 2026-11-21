//! Evoker fangs (`EvokerFangs`): rising out of the floor after a delay,
//! biting what stands on them for 6 damage, sinking back after 22 ticks.
use super::Game;
use horizon_core::geometry::Vec3;
use horizon_core::random::{EntityRandom, RandomSource};

#[derive(Clone, Debug)]
pub(super) struct Fangs {
    /// Entity-owned registration metadata; content clones are detached.
    pub(super) attachment_lifetime: horizon_mod_runtime::attachments::AttachmentLifetime,
    pub(in crate::game) join: horizon_client::join_seq::JoinSeq,
    pub id: u32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub warmup: i32,
    pub life: i32,
    pub owner: u32,
    pub started: bool,
    /// `Entity.deltaMovement`, once a tick has read the loaded `Motion`.
    pub velocity: Option<Vec3>,
    /// `Entity.random`. The attack sound's pitch is its first draw.
    random: EntityRandom,
}

horizon_client::impl_joined!(Fangs);
horizon_mod_runtime::impl_attachment_lifetime!(Fangs);

/// `EvokerFangs.handleEntityEvent`: `nextFloat() * 0.2 + 0.85`.
pub(super) fn fang_attack_pitch(random: &mut impl RandomSource) -> f32 {
    random.next_float() * 0.2 + 0.85
}
/// One of the twelve `crit` particles `EvokerFangs.tick` spawns on a client
/// when `lifeTicks` hits 14. The box is 0.5 wide (`getBbWidth() * 0.5`).
pub(super) fn fang_crit(random: &mut impl RandomSource, pos: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let half = 0.5 * 0.5;
    let x = pos[0] + (random.next_double() * 2.0 - 1.0) * half;
    let y = pos[1] + 0.05 + random.next_double();
    let z = pos[2] + (random.next_double() * 2.0 - 1.0) * half;
    let xd = (random.next_double() * 2.0 - 1.0) * 0.3;
    let yd = 0.3 + random.next_double() * 0.3;
    let zd = (random.next_double() * 2.0 - 1.0) * 0.3;
    ([x, y + 1.0, z], [xd, yd, zd])
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
            life: 22,
            owner,
            started: false,
            velocity: None,
            random: horizon_client::mobs::entity_random(id, Vec3::new(at[0], at[1], at[2])),
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
    use super::{fang_attack_pitch, fang_crit};
    use horizon_core::random::LegacyRandom;

    #[test]
    fn fang_attack_pitch_matches_the_vanilla_span() {
        // Java Random seed 1, first nextFloat() * 0.2 + 0.85, in f32.
        let mut random = LegacyRandom::new(1);
        let pitch = fang_attack_pitch(&mut random);
        assert!((pitch - 0.996_175_65).abs() < 1.0e-6, "{pitch}");
        assert!((0.85..1.05).contains(&pitch));
    }

    #[test]
    fn fang_crit_sits_above_the_jaws() {
        // Java Random seed 1, the first crit of the twelve. The y is
        // getY() + 0.05 + nextDouble(), then addParticle adds 1.
        let mut random = LegacyRandom::new(1);
        let (at, velocity) = fang_crit(&mut random, [2.0, 70.0, -4.0]);
        let expected_at = [
            2.115_439_095_351_645_5,
            71.460_080_811_492_2,
            -4.146_142_579_345_142,
        ];
        let expected_velocity = [
            -0.100_369_766_424_293_3,
            0.590_326_772_827_236_1,
            -0.296_329_690_640_543_2,
        ];
        for i in 0..3 {
            assert!((at[i] - expected_at[i]).abs() < 1.0e-12, "{at:?}");
            assert!(
                (velocity[i] - expected_velocity[i]).abs() < 1.0e-12,
                "{velocity:?}"
            );
        }
    }
}
