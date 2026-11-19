//! Jeu de poursuite entre bébés (`PlayTag`, `villager_brain.rs:2592-2692`) : un bébé choisit un camarade à portée et le
//! poursuit à vitesse 0,6, distance d'arrivée 1 (`villager_brain.rs:2632-2640`). Le camarade est un paramètre (`friends`,
//! mémoire de liste d'entités : les bébés proches, interface supposée) ; la cible est l'interaction (`INTERACTION_TARGET`).
use crate::brain::*;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_play_tag",
    mems: &[&WALK_TARGET, &LOOK_TARGET, &INTERACTION_TARGET],
    one_shot: true,
    can_start,
    start,
    stop,
    ..Behaviour::DEFAULT
};

/// The current play partner (`interaction_target` of the `Mob`).
pub static INTERACTION_TARGET: MemKey = MemKey::new("minecraft:interaction_target", Shape::Who);

pub struct Params {
    /// Memory of the nearby babies (other than the entity itself).
    pub friends: &'static MemKey,
}

fn partner(w: &World, e: Entity, c: &Ctx) -> Option<Who> {
    match get(w, e, c.get::<Params>().friends) {
        Some(MemValue::Whos(v)) => v.iter().copied().find(|x| matches!(x, Who::Entity(k) if *k != e)),
        _ => None,
    }
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    !present(w, e, &WALK_TARGET) && partner(w, e, c).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some(Who::Entity(kid)) = partner(w, e, c) else { return };
    set(w, e, &INTERACTION_TARGET, MemValue::Who(Who::Entity(kid)), None);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Entity(Who::Entity(kid), true)), None);
    set(
        w,
        e,
        &WALK_TARGET,
        MemValue::Walk(WalkTarget { target: Tracker::Entity(Who::Entity(kid), false), speed: 0.6, close: 1 }),
        None,
    );
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
    erase(w, e, &INTERACTION_TARGET);
}
