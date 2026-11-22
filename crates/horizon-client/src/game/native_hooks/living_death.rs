use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// `LivingEntity.die`: the complete accepted killing source and its weapon,
    /// before any loot, experience or death message. Cancelling leaves health
    /// exactly where the listener left it; an unhealed body ticks away without loot.
    pub(in crate::game) fn native_living_death(
        &mut self,
        entity: u32,
        kind: &str,
        source: &HurtSource<'_>,
        weapon: Option<&mod_api::ItemStack>,
        cancel: &mut bool,
    ) {
        self.fire_living_death(entity, kind, source, weapon, cancel);
    }
}
/// `native_living_incoming_damage`'s entity, entity type and source for a hit.
pub(in crate::game) fn hurt_server_args<'a>(
    hurting: Hurting<'a>,
    attack: &Attack,
) -> (i32, &'a str, HurtSource<'a>) {
    match hurting {
        Hurting::Player(entity, source) => (entity, "minecraft:player", source),
        Hurting::Mob(mob, damage) => (
            mob.id as i32,
            mob.kind.id.as_str(),
            mob_source(damage, attack),
        ),
        Hurting::Dragon(entity, source) => (entity, "minecraft:ender_dragon", source),
        Hurting::ArmorStand(entity, source) => (entity, "minecraft:armor_stand", source),
        Hurting::Entity(entity, kind, source) => (entity, kind, source),
    }
}

pub(super) fn capture_hit(
    hurting: Hurting<'_>,
    attack: &Attack,
) -> Option<horizon_client::health::AcceptedHit> {
    let death = mod_api::__private::listened(mod_api::HookId::LivingDeath);
    let loot = mod_api::__private::listened(mod_api::HookId::LootDrops);
    if !death && !loot {
        return None;
    }
    let (committed, dying) = match &hurting {
        Hurting::Mob(mob, _) => (mob.last_damage, mob.health <= 0.0),
        _ => (None, true),
    };
    let (_, _, source) = hurt_server_args(hurting, attack);
    if !dying && !(loot && source.direct != 0 && attack.projectile == Some(source.direct)) {
        return None;
    }
    Some(horizon_client::health::AcceptedHit::capture(
        source, attack, committed,
    ))
}

