//! The hurt state of a creature by id (`P bridge/host_combat.rs`): damage window, `invulnerableTime`, attackers, combat tracker,
//! fire, fuse, glowing, enderman teleports, one random seed; and the teams of the board (`P bridge/teams.rs`).
use super::level::EcsLevel;
use crate::mobs::combat::{Entry, Tracker};
use crate::mobs::{Damage, DamageKind, MobWorld};
use horizon_ecs_proto::{
    bridge::{host::EcsWorld, ids::id_of, teams},
    targeting::Who,
};

fn mob_who(w: &EcsWorld, who: Who) -> Option<crate::mobs::Who> {
    match who {
        Who::Player(_) => Some(crate::mobs::Who::Player),
        Who::Entity(e) => id_of(w.world(), e).map(crate::mobs::Who::Mob),
    }
}

/// The damage kind of a damage type name the ECS carries (with or without the namespace); None for a name Horizon has no kind for.
fn kind_of_name(name: &str) -> Option<DamageKind> {
    use DamageKind as K;
    Some(match name.strip_prefix("minecraft:").unwrap_or(name) {
        "player_attack" => K::PlayerAttack,
        "mace_smash" => K::MaceSmash,
        "spear" => K::Spear,
        "mob_attack" => K::MobAttack,
        "mob_attack_no_aggro" => K::MobAttackNoAggro,
        "arrow" => K::Arrow,
        "thrown" => K::Thrown,
        "fireball" => K::Fireball,
        "explosion" | "player_explosion" => K::Explosion,
        "fall" => K::Fall,
        "fly_into_wall" => K::FlyIntoWall,
        "falling_block" => K::FallingBlock,
        "falling_stalactite" => K::FallingStalactite,
        "on_fire" => K::OnFire,
        "in_fire" => K::InFire,
        "campfire" => K::Campfire,
        "lava" => K::Lava,
        "drown" => K::Drown,
        "dry_out" => K::DryOut,
        "magic" => K::Magic,
        "indirect_magic" => K::IndirectMagic,
        "sting" => K::Sting,
        "cactus" => K::Cactus,
        "sweet_berry_bush" => K::SweetBerryBush,
        "generic" => K::Generic,
        "in_wall" => K::InWall,
        "thorns" => K::Thorns,
        "sonic_boom" => K::SonicBoom,
        "freeze" => K::Freeze,
        "out_of_world" => K::OutOfWorld,
        "cramming" => K::Cramming,
        "falling_anvil" => K::FallingAnvil,
        "fireworks" => K::Fireworks,
        "generic_kill" => K::GenericKill,
        "lightning_bolt" => K::LightningBolt,
        "outside_border" => K::OutsideBorder,
        "spit" => K::Spit,
        "stalagmite" => K::Stalagmite,
        "starve" => K::Starve,
        "hot_floor" => K::HotFloor,
        "wind_charge" => K::WindCharge,
        "wither" => K::Wither,
        "wither_skull" => K::WitherSkull,
        _ => return None,
    })
}

/// A hit waiting in the ECS's damage queue, by network id (the target is linked on the way out).
#[derive(Clone, Copy, Debug)]
pub struct PendingHit {
    pub target: u32,
    pub amount: f32,
    pub damage: &'static str,
}

impl EcsLevel {
    /// The hits queued since the last tick (before `apply_damage` takes them), emptied: what the host's attacks and blasts wrote. A target
    /// that has no network id is left out.
    pub fn drain_damage_events(&mut self) -> Vec<PendingHit> {
        let Some(w) = self.ecs_mut() else { return Vec::new() };
        let hits = std::mem::take(&mut w.world_mut().resource_mut::<horizon_ecs_proto::cow::DamageQueue>().0);
        hits.into_iter()
            .filter_map(|h| Some(PendingHit { target: id_of(w.world(), h.target)?, amount: h.amount, damage: h.damage }))
            .collect()
    }
    pub fn damage_cooldown(&self, id: u32) -> Option<i32> {
        self.ecs()?.damage_cooldown(self.entity_of(id)?)
    }
    pub fn set_damage_cooldown(&mut self, id: u32, ticks: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_damage_cooldown(e, ticks))
    }
    pub fn invulnerable_time(&self, id: u32) -> Option<i32> {
        self.ecs()?.invulnerable_time(self.entity_of(id)?)
    }
    pub fn set_invulnerable_time(&mut self, id: u32, ticks: i32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_invulnerable_time(e, ticks))
    }
    /// `getLastHurtByMob` while remembered; None when there is none or the attacker has no id.
    pub fn recent_attacker(&self, id: u32) -> Option<crate::mobs::Who> {
        let w = self.ecs()?;
        mob_who(w, w.recent_attacker(self.entity_of(id)?)?)
    }
    /// The luck and main-hand stack of the player `player` that hurts `e` (what its death reads of the killer); nothing when inert.
    pub fn note_killer(&mut self, e: bevy_ecs::entity::Entity, player: u32, luck: f32, weapon: Option<crate::items::ItemStack>) {
        if let Some(w) = self.ecs_mut() {
            w.note_killer(e, player, luck, weapon);
        }
    }
    pub fn last_hurt_by_player(&self, id: u32) -> i32 {
        let Some(e) = self.entity_of(id) else { return 0 };
        self.ecs().map_or(0, |w| w.last_hurt_by_player(e))
    }
    /// `LivingEntity.setLastHurtByPlayer` on the entity `id` (`credit::record`); nothing when inert.
    pub fn record_credit(&mut self, id: u32, by: Option<horizon_ecs_proto::targeting::Who>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.record_credit(e, by);
        }
    }
    /// The uuid of the player the entity `id` remembers as its last hurter, while remembered.
    pub fn credited_uuid(&self, id: u32) -> Option<u128> {
        self.ecs()?.credited_uuid(self.entity_of(id)?).map(crate::lan::playerdata::ints_uuid)
    }
    /// `Animal.setInLove(player)` on the entity `id`: the player's ECS id, None for no player.
    pub fn set_love_cause(&mut self, id: u32, player: Option<u32>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.set_love_cause(e, player);
        }
    }
    /// The combat tracker of `id` as the server's; None until its first hit. A hit whose type has no Horizon kind is left out.
    pub fn combat_tracker(&self, id: u32) -> Option<Tracker> {
        let w = self.ecs()?;
        let log = w.combat_log(self.entity_of(id)?)?;
        let mut t = Tracker::default();
        for h in &log.entries {
            let Some(kind) = kind_of_name(h.damage_type) else { continue };
            let attacker = h.by.and_then(|b| mob_who(w, b));
            let source = Damage { kind, attacker, from: None, attacker_species: None };
            t.record(Entry { source, damage: h.amount, in_water: h.in_water, fall_distance: h.fall_distance }, 0);
        }
        t.in_combat = log.in_combat;
        Some(t)
    }
    pub fn set_ignited(&mut self, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_ignited(e))
    }
    pub fn extinguish(&mut self, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.extinguish(e))
    }
    pub fn set_glowing(&mut self, id: u32, on: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.set_glowing(e, on))
    }
    pub fn repeated_teleport(&mut self, mw: &MobWorld<'_>, id: u32, tries: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.repeated_teleport(mw, e, tries))
    }
    pub fn draw_seed(&mut self, id: u32) -> Option<i64> {
        let e = self.entity_of(id)?;
        self.ecs_mut()?.draw_seed(e)
    }
    pub fn add_spawn_tag(&mut self, id: u32, name: &str) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|w| w.add_spawn_tag(e, name))
    }
    pub fn set_team(&mut self, name: &str, members: &[Who]) {
        if let Some(w) = self.ecs_mut() {
            teams::set_team(w.world_mut(), name, members);
        }
    }
    pub fn set_team_friendly_fire(&mut self, name: &str, on: bool) {
        if let Some(w) = self.ecs_mut() {
            teams::set_friendly_fire(w.world_mut(), name, on);
        }
    }
    pub fn clear_team(&mut self, name: &str) {
        if let Some(w) = self.ecs_mut() {
            teams::clear_team(w.world_mut(), name);
        }
    }
}
