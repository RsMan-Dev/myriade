//! A mob whose AI step a mod paused while its physics runs on, for
//! `MobRef::set_ai_paused` (arismoko/horizon#4478). Vanilla's `NoAI` skips `travel` with the
//! AI step, so a `NoAI` mob neither falls nor carries a push; this is
//! vanilla's `isImmobile` instead (a dying mob, a stunned ravager): the AI
//! step is skipped and the inputs are zeroed while `travel` runs on.
//!
//! The flag (`Ai::ai_paused`) is read once, at the AI step (`mobs::ai_pause`).
//! It is saved with the mob as an attachment-style value
//! (`entity_nbt::write_ai_paused`), never as `NoAI`, and read back on load.
//!
//! Server authority: each case the pause cannot apply to is told apart here,
//! where the side, the epoch and the held-out list are known.
use super::{Game, commands::Target};
use mod_api::{EntityRef, MobAiPauseRefusal, MobRef};

impl Game<'_> {
    /// Whether a mod paused the mob's AI step. None for a stale or held-out
    /// handle, anything but a mob, the dragon, or on a client.
    pub(super) fn mob_ai_paused(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        self.ecs_mod_live(id).and(self.ecs.ai_paused(id))
    }

    /// Pauses or resumes the mob's AI step while `aiStep`'s physics runs on.
    /// Refused, with nothing changed, as `MobAiPauseRefusal` names.
    pub(super) fn mob_set_ai_paused(
        &mut self,
        mob: MobRef,
        paused: bool,
    ) -> Result<(), MobAiPauseRefusal> {
        let id = self.ai_pause_target(mob.entity())?;
        // The pause and the inputs it zeroes (LivingEntity.aiStep's isImmobile branch) are the ECS mob's, set by the bridge.
        self.ecs_mod_live(id).ok_or(MobAiPauseRefusal::Stale)?;
        self.ecs.set_ai_paused(id, paused);
        Ok(())
    }

    /// The mob a pause names, or which of the named refusals it is.
    fn ai_pause_target(&self, entity: EntityRef) -> Result<u32, MobAiPauseRefusal> {
        if self.native_lifecycle.epoch() != Some(entity.epoch()) {
            return Err(MobAiPauseRefusal::Stale);
        }
        if self.held_out.contains(&entity.id()) {
            return Err(MobAiPauseRefusal::HeldOut);
        }
        match self.native_entity_target(entity) {
            Some(Target::Mob(id)) => Ok(id),
            Some(Target::Dragon(_) | Target::DragonPart(_)) => Err(MobAiPauseRefusal::NoAiStep),
            Some(_) => Err(MobAiPauseRefusal::NotAMob),
            None if self.level_dragon_part(entity.id()).is_some() => {
                Err(MobAiPauseRefusal::NoAiStep)
            }
            None => Err(MobAiPauseRefusal::Stale),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_guest_turn_tests::with_acting_guest;
    use horizon_client::chunk::Chunk;
    use horizon_core::{geometry::Vec3, nbt::Tag};
    use mod_api::{EntityRef, MobAiPauseRefusal, MobRef, WorldEpoch};
    use std::sync::Arc;

    /// A server level with a stone floor at y 64 over chunk (0, 0), in a turn
    /// of guest 12, the acting player (entity 12).
    fn level(game: &mut super::Game<'_>) {
        for (x, z) in (-1..=1).flat_map(|x| (-1..=1).map(move |z| (x, z))) {
            let mut chunk = Chunk::empty(x, z);
            chunk.lit = true;
            game.world.chunks.insert((x, z), Arc::new(chunk));
        }
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        for x in 0..16 {
            for z in 0..16 {
                game.world.set(x, 64, z, stone).unwrap();
            }
        }
    }

    /// A `kind` mob of the ECS level at `at`, numbered `id`.
    fn add(game: &mut super::Game<'_>, id: u32, kind: &str, at: Vec3) -> MobRef {
        game.ecs = game.test_ecs_level();
        crate::game::ecs::test_support::place_test_mob(game, id, kind, [at.x, at.y, at.z]).unwrap();
        game.mob(game.entity(id).unwrap()).unwrap()
    }

    fn paused_value(tag: &Tag) -> Option<&Tag> {
        tag.get(horizon_mod_runtime::attachments::ATTACHMENTS_KEY)?
            .get("horizon")?
            .get("ai_paused")
    }

    /// The pause is saved with the mob beside the mods' attachments, never as
    /// `NoAI`, and a reload finds the mob still paused; a resumed mob's save
    /// drops it and keeps the mods' own.
    #[test]
    #[ignore = "daily"]
    fn the_pause_round_trips_through_a_save_and_writes_no_no_ai() {
        use horizon_core::nbt::Compound;
        with_acting_guest(Vec3::ZERO, |game| {
            level(game);
            let pig = add(game, 40, "minecraft:pig", Vec3::new(3.5, 65.0, 3.5));
            let mods = Compound::from_iter([(
                "example".into(),
                Tag::Compound(Compound::from_iter([(
                    "count".into(),
                    Tag::Compound(Compound::from_iter([("value".into(), Tag::Int(3))])),
                )])),
            )]);
            game.entity_attachments.insert(
                40,
                horizon_mod_runtime::attachments::Attachments::from_nbt(mods),
            );
            assert_eq!(pig.set_ai_paused(game, true), Ok(()));
            // The saved data is read back onto the same mob, as a load of its file would.
            let reload = |game: &mut super::Game<'_>, mob: MobRef| {
                let saved = Tag::Compound(game.entity_nbt(mob.entity()).unwrap());
                let target = game.native_entity_target(mob.entity()).unwrap();
                game.apply_entity_nbt(target, saved.clone()).unwrap();
                (saved, game.mob(mob.entity()).unwrap())
            };
            let (saved, pig) = reload(game, pig);
            assert_eq!(paused_value(&saved), Some(&Tag::Byte(1)));
            assert!(saved.get("NoAI").is_none(), "no NoAI tag: {saved:?}");
            assert_eq!(
                pig.ai_paused(game),
                Some(true),
                "still paused after the reload"
            );
            assert_eq!(pig.no_ai(game), Some(false));

            assert_eq!(pig.set_ai_paused(game, false), Ok(()));
            let (saved, pig) = reload(game, pig);
            assert_eq!(paused_value(&saved), None);
            assert!(saved.get("NoAI").is_none());
            let attachments = saved
                .get(horizon_mod_runtime::attachments::ATTACHMENTS_KEY)
                .unwrap();
            assert!(attachments.get("horizon").is_none(), "nothing left behind");
            assert!(
                attachments.get("example").is_some(),
                "the mods' attachment is kept"
            );
            assert_eq!(pig.ai_paused(game), Some(false));
        });
    }

    /// Each case the pause cannot apply to has its own refusal, from the
    /// real backend, and none of them changes anything.
    #[test]
    #[ignore = "daily"]
    fn each_refusal_is_named_and_changes_nothing() {
        with_acting_guest(Vec3::ZERO, |game| {
            level(game);
            let pig = add(game, 40, "minecraft:pig", Vec3::new(3.5, 65.0, 3.5));
            let epoch = pig.entity().epoch();
            let refused = |game: &mut super::Game<'_>, handle: MobRef, why| {
                assert_eq!(handle.set_ai_paused(game, true), Err(why), "{why:?}");
                assert_eq!(handle.ai_paused(game), None, "{why:?}");
            };

            let stale = MobRef::new(EntityRef::new(WorldEpoch::new(999), 40));
            refused(game, stale, MobAiPauseRefusal::Stale);
            let gone = MobRef::new(EntityRef::new(epoch, 999));
            refused(game, gone, MobAiPauseRefusal::Stale);

            game.held_out.push(40);
            refused(game, pig, MobAiPauseRefusal::HeldOut);
            game.held_out.pop();

            game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [8.0, 80.0, 8.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(), &mut || 50));
            refused(
                game,
                MobRef::new(EntityRef::new(epoch, 50)),
                MobAiPauseRefusal::NoAiStep,
            );
            refused(
                game,
                MobRef::new(EntityRef::new(epoch, 51)),
                MobAiPauseRefusal::NoAiStep,
            );

            let player = MobRef::new(game.entity(12).unwrap());
            refused(game, player, MobAiPauseRefusal::NotAMob);

            assert_eq!(pig.ai_paused(game), Some(false), "no refusal paused it");
            assert_eq!(pig.no_ai(game), Some(false));
        });
    }
}
