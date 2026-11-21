//! The server's `MobEvent`s while the ECS runs the level (`apply_mob_events`, called by `handle_mob_events_reusable`).
//! The effects that need no mob list act here, on the bridge (`EcsLevel::set_velocity`, `entity_of`); every other variant goes
//! back to the creature code (`handle_mob_events_legacy`) unchanged, so none loses its effect.
use super::super::{Game, Result};
use crate::mobs::{MobEvent, Who};
use horizon_client::mobs::entity_sounds::full_id;
use horizon_client::particles::{ParticleOptions, ParticleType};
use horizon_core::geometry::Vec3;
use std::collections::VecDeque;

impl Game<'_> {
    /// Applies `events` in order; a nested `ForPlayer` event (no guest) runs before the events after it.
    pub(in crate::game) fn apply_mob_events(&mut self, events: &mut Vec<MobEvent>) -> Result<()> {
        let mut queue: VecDeque<MobEvent> = std::mem::take(events).into();
        while let Some(e) = queue.pop_front() {
            match e {
                MobEvent::ForPlayer { conn: None, event } => self.apply_mob_events(&mut vec![*event])?,
                MobEvent::SquidCycle { mob, at } => self.lan_entity_event(mob as i32, 19, at),
                MobEvent::RabbitHop { mob, at } => self.lan_entity_event(mob as i32, 1, at),
                MobEvent::PowderSnowParticle { at, velocity } => self.snowflake_particle(at, velocity),
                MobEvent::LandingDust { state, at, count, speed } => {
                    self.landing_dust(state, Vec3::new(at[0], at[1], at[2]), count, speed)
                }
                MobEvent::Sound(name, at, volume, pitch) => self.audio.play(&full_id(name), Some(at), volume, pitch),
                MobEvent::ProfessionWorkSound(name, at, volume, pitch) => self.audio.play(&name, Some(at), volume, pitch),
                MobEvent::LocalSound(name, at, volume, pitch) => {
                    self.audio.play_unshared(&full_id(name), Some(at), volume, pitch)
                }
                MobEvent::SoundId(name, at, volume, pitch) => self.audio.play(&name, Some(at), volume, pitch),
                MobEvent::LevelParticles { kind, at, spread, speed, count } => {
                    if let Some(ty) = ParticleType::from_name(kind) {
                        let options = ParticleOptions::simple(ty);
                        self.particles.send_particles(&options, at, spread, speed, count);
                        self.lan_level_particles(&options, at, spread, speed, count, None);
                    }
                }
                MobEvent::SetBlock { pos, state } => self.level_set(pos, state)?,
                MobEvent::LevelEvent { id, pos, data } => self.level_event_at(id, pos, data),
                MobEvent::Projectile(p) => self.launch_mob_projectile(p),
                MobEvent::SetVelocity { target, velocity } => {
                    let v = Vec3::new(velocity[0], velocity[1], velocity[2]);
                    match target {
                        Who::Player => {
                            if let Some(p) = self.player.as_mut() {
                                p.velocity = v;
                            }
                        }
                        Who::Mob(id) => {
                            if let Some(e) = self.ecs.entity_of(id) {
                                self.ecs.set_velocity(e, v);
                            }
                        }
                    }
                }
                MobEvent::EntityEvent { mob, event, at } => self.ecs_mob_entity_event(mob, event, at),
                MobEvent::CreakingHurt { home, by_player, id } => self.creaking_hurt(home, id, by_player),
                MobEvent::CreakingTearDown { center, spread } => self.creaking_tear_down(center, spread),
                MobEvent::Push { target, velocity } => match target {
                    Who::Player => {
                        if let Some(p) = self.player.as_mut() {
                            p.velocity = p.velocity.add_vector(Vec3::new(velocity[0], velocity[1], velocity[2]));
                        }
                    }
                    Who::Mob(id) => self.ecs_mob_push(id, velocity),
                },
                MobEvent::SetTarget { mob, target } => self.ecs_mob_target(mob, target),
                // The ECS's `reinforcements` module decides at the hit (`Zombie.hurtServer`); the legacy request has no list to act on.
                MobEvent::Reinforcements { .. } => {}
                other @ (
                MobEvent::ForPlayer{ .. }
                | MobEvent::Drown{ .. }
                | MobEvent::SunProtectionWear{ .. }
                | MobEvent::ItemParticles{ .. }
                | MobEvent::BoundSound{ .. }
                | MobEvent::BubbleColumnParticles{ .. }
                | MobEvent::Bonemeal{ .. }
                | MobEvent::RingBell{ .. }
                | MobEvent::ChestOpen{ .. }
                | MobEvent::EnterHive{ .. }
                | MobEvent::ParrotToShoulder{ .. }
                | MobEvent::BecomeStatue{ .. }
                | MobEvent::ChestSlot{ .. }
                | MobEvent::Infested{ .. }
                | MobEvent::ArmorHurt{ .. }
                | MobEvent::GliderWear{ .. }
                | MobEvent::Gift{ .. }
                | MobEvent::BreedTarget{ .. }
                | MobEvent::LockGaze{ .. }
                | MobEvent::StartFollowing{ .. }
                | MobEvent::SchoolSize{ .. }
                | MobEvent::LevelEventSound{ .. }
                | MobEvent::GameEvent{ .. }
                | MobEvent::LandingGameEvent{ .. }
                | MobEvent::AttackPlayer(..)
                | MobEvent::Sting{ .. }
                | MobEvent::AttackPlayerWithEffect{ .. }
                | MobEvent::DamagePlayer{ .. }
                | MobEvent::ElderCurse{ .. }
                | MobEvent::RemoveKilled{ .. }
                | MobEvent::SonicBoom{ .. }
                | MobEvent::LidPush{ .. }
                | MobEvent::Wololo{ .. }
                | MobEvent::Fangs{ .. }
                | MobEvent::Shoot(..)
                | MobEvent::ExplosionSource{ .. }
                | MobEvent::Explode(..)
                | MobEvent::LingeringCloud{ .. }
                | MobEvent::DamageMob{ .. }
                | MobEvent::AttackMob{ .. }
                | MobEvent::AttackMobWithEffect{ .. }
                | MobEvent::CubeHurtPlayer{ .. }
                | MobEvent::CubeHurtMob{ .. }
                | MobEvent::PufferfishSting{ .. }
                | MobEvent::StabDismount{ .. }
                | MobEvent::Knockback{ .. }
                | MobEvent::PiglinAlertTarget(..)
                | MobEvent::HoglinAttackTarget{ .. }
                | MobEvent::HuntHoglin{ .. }
                | MobEvent::Retreat{ .. }
                | MobEvent::StartRiding{ .. }
                | MobEvent::BowShot{ .. }
                | MobEvent::CrossbowLoad{ .. }
                | MobEvent::CrossbowShot{ .. }
                | MobEvent::Spawn(..)
                | MobEvent::SpawnKind{ .. }
                | MobEvent::Convert{ .. }
                | MobEvent::DropItem{ .. }
                | MobEvent::DropStack{ .. }
                | MobEvent::ThrowItem{ .. }
                | MobEvent::DropGoatHorn{ .. }
                | MobEvent::DropGift{ .. }
                | MobEvent::EndermanPlaceBlock{ .. }
                | MobEvent::PlaceBlock{ .. }
                | MobEvent::CrackTurtleEgg{ .. }
                | MobEvent::BedOccupied{ .. }
                | MobEvent::SetDoorOpen{ .. }
                | MobEvent::BreakDoor{ .. }
                | MobEvent::DestroyProgress{ .. }
                | MobEvent::CropRaided{ .. }
                | MobEvent::Infest{ .. }
                | MobEvent::BreakBlock{ .. }
                | MobEvent::TakeBlock{ .. }
                | MobEvent::IfBroke{ .. }
                | MobEvent::Jump{ .. }
                | MobEvent::Particles{ .. }
                | MobEvent::AroundSelf{ .. }
                | MobEvent::ParticleAt{ .. }
                | MobEvent::ParticleColored{ .. }
                | MobEvent::Experience{ .. }
                | MobEvent::AxolotlSupportPlayer{ .. }
                | MobEvent::DolphinTreasure{ .. }
                | MobEvent::EffectPlayer{ .. }
                | MobEvent::EffectMob{ .. }
                | MobEvent::Ignite(..)
                | MobEvent::ZombieIgnite(..)
                | MobEvent::Breed{ .. }
                | MobEvent::SetBlockName{ .. }
                | MobEvent::Hiss{ .. }
                | MobEvent::Tamed{ .. }
                | MobEvent::Buck{ .. }
                | MobEvent::EjectPassengers{ .. }
                | MobEvent::FinishAdmiring{ .. }
                | MobEvent::IronGolemHit{ .. }
                | MobEvent::HurtSelf{ .. }
                | MobEvent::VillagerMeet{ .. }
                | MobEvent::VillagerTrade{ .. }
                | MobEvent::VillagerOffers{ .. }
                | MobEvent::VillagerRestocked{ .. }
                | MobEvent::VillagerBirth{ .. }
                | MobEvent::WorkAtComposter{ .. }
                | MobEvent::GolemCheck{ .. }
                | MobEvent::Firework{ .. }
                | MobEvent::ThrowGift{ .. }
                | MobEvent::PatrolTarget{ .. }
                | MobEvent::RaiderTarget{ .. }
                | MobEvent::GiftLoot{ .. }
                | MobEvent::LayEgg{ .. }
                | MobEvent::ShedScute{ .. }
                | MobEvent::LeashBreak{ .. }
                | MobEvent::NotifyLeashHolder{ .. }
                | MobEvent::CureZombieVillager{ .. }
                | MobEvent::Vibration{ .. }
                | MobEvent::DigParticles{ .. }
                | MobEvent::BlockBreakEffect{ .. }
                | MobEvent::Darkness{ .. }
                | MobEvent::PandaSneezed{ .. }
                | MobEvent::FallToPassengers{ .. }
                ) => self.handle_mob_events_legacy(other)?,
            }
        }
        Ok(())
    }
}
