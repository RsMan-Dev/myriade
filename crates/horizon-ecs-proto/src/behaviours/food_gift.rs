//! Le villageois donne un cadeau au héros du village (`GiveGiftToHero`, `villager_brain.rs:2790` et suivants). Le héros est
//! un joueur ayant gagné un raid : `Heroes` (ressource, identifiants de joueur) écrite par l'hôte à la victoire.
//! Le cadeau tombe devant le joueur (`item::drop_item`), tiré dans la table `gifts` (paramètre).
use bevy_ecs::prelude::Entity;
use bevy_ecs::prelude::World;
use crate::targeting::Who;
use super::raid;
use crate::brain::*;
use crate::cow::Pos;
use crate::goals::with_rng;
use crate::item::drop_item;
use bevy_ecs::prelude::Resource;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use std::collections::BTreeSet;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_give_gift_to_hero",
    mems: &[&NEAREST_VISIBLE_PLAYER],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

/// Player ids who won a raid as the hero of the village, written by the host.
#[derive(Resource, Default)]
pub struct Heroes(pub BTreeSet<u32>);

pub struct Params {
    /// Gift table: (item, count), uniform choice.
    pub gifts: &'static [(&'static str, i32)],
}

/// The nearest visible player when that player is a hero.
fn hero(w: &World, e: Entity) -> Option<u32> {
    let id = match get(w, e, &NEAREST_VISIBLE_PLAYER) {
        Some(MemValue::Who(Who::Player(id))) => *id,
        _ => return None,
    };
    w.get_resource::<Heroes>().filter(|h| h.0.contains(&id)).map(|_| id)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    raid::status(w, e) == raid::RAID_VICTORY && hero(w, e).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let gifts = c.get::<Params>().gifts;
    if gifts.is_empty() {
        return;
    }
    let i = with_rng(w, e, |r| r.next_int(gifts.len() as i32)) as usize;
    let (name, n) = gifts[i];
    let Some(p) = w.get::<Pos>(e).map(|q| q.0) else { return };
    let at = Vec3::new(p.x, p.y + 1.0, p.z);
    drop_item(w, ItemStack::new(name, n as u32), at, Vec3::new(0.0, 0.3, 0.0), 40);
}
