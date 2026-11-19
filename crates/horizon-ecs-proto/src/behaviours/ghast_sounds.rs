//! The happy ghast's sounds (`happy_ghast.rs` 626-641): the ambient, hurt and death sounds of the adult or of the baby (`ghastling`),
//! and the goggles when the harness's goggles go down or up. Each sound is sent through `HostOut::sound` at the ghast's position; the
//! volume is 4 for the adult and 1 for the baby, as in vanilla's `playSound` calls.
//! DOUTE : les noms d'événements sont ceux de la ressource (`entity.happy_ghast.*`, `entity.ghastling.*`) ; leur présence dans le
//! cadre sonore n'a pas été vérifiée hors compilation.
use crate::cow::*;
use crate::host_out::HostOut;
use bevy_ecs::prelude::*;

/// Which sound: the three moods, and the goggles.
#[derive(Clone, Copy, Debug)]
pub enum Cue {
    Ambient,
    Hurt,
    Death,
    GogglesDown,
    GogglesUp,
}

/// The sound's name for an adult or a baby (`GrowAt` marks the baby).
pub fn name(cue: Cue, baby: bool) -> &'static str {
    match (cue, baby) {
        (Cue::Ambient, false) => "entity.happy_ghast.ambient",
        (Cue::Hurt, false) => "entity.happy_ghast.hurt",
        (Cue::Death, false) => "entity.happy_ghast.death",
        (Cue::Ambient, true) => "entity.ghastling.ambient",
        (Cue::Hurt, true) => "entity.ghastling.hurt",
        (Cue::Death, true) => "entity.ghastling.death",
        (Cue::GogglesDown, _) => "entity.happy_ghast.harness_goggles_down",
        (Cue::GogglesUp, _) => "entity.happy_ghast.harness_goggles_up",
    }
}

/// Sends the cue at the ghast's position: volume 4 for an adult, 1 for a baby.
pub fn play(w: &mut World, e: Entity, cue: Cue) {
    let baby = w.get::<GrowAt>(e).is_some();
    let Some(at) = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]) else { return };
    let volume = if baby { 1.0 } else { 4.0 };
    w.resource_mut::<HostOut>().sound(name(cue, baby), at, volume, 1.0);
}
