//! The joins of the ECS entities, in the order the mods are told: the next one after a mark, not after `through`, read from
//! the `JoinedAt` each entity carries from its making (`cow::TypeName`, or the hanging it is hung by).
use super::level::EcsLevel;
use horizon_client_lib::join_seq::JoinSeq;
use horizon_ecs_proto::hanging::JoinedAt;

impl EcsLevel {
    /// The ECS entity whose join is the least after `mark` (and not after `through`) among those `refused` does not name: its network
    /// id and its join. None with no level.
    pub fn next_joined(&self, mark: JoinSeq, through: Option<JoinSeq>, refused: impl Fn(u32) -> bool) -> Option<(u32, JoinSeq)> {
        self.world()?
            .iter_entities()
            .filter_map(|e| {
                let seq = e.get::<JoinedAt>()?.0;
                (seq > mark && through.is_none_or(|t| seq <= t)).then_some((e.id(), seq))
            })
            .filter_map(|(e, seq)| Some((self.id_of(e)?, seq)))
            .filter(|&(id, _)| !refused(id))
            .min_by_key(|&(_, seq)| seq)
    }
    /// The ids of the ECS entities that joined at `mark` or later.
    pub fn ids_joined_since(&self, mark: JoinSeq) -> Vec<u32> {
        let Some(w) = self.world() else { return Vec::new() };
        w.iter_entities()
            .filter(|e| e.get::<JoinedAt>().is_some_and(|j| j.0 >= mark))
            .filter_map(|e| self.id_of(e.id()))
            .collect()
    }
}
