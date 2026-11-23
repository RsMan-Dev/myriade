//! The rule `HAND_OVER`: a player puts an item into the hand of an entity with an empty one (one item), and takes it back with an empty hand.
//! The allay (`Allay.mobInteract`: sounds, the liked player, the item goes to the player's inventory) and the copper golem (`CopperGolem.mobInteract`: no give, the held item is thrown at the player) share it by parameters.
//! Not here: an allay's duplication by amethyst while it dances, the swing of the allay on the take.
use crate::cow::*;
use crate::goals::*;
use crate::rules::{no_attrs, params, Rule};
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct HandOverParams {
    /// The entity takes an item from the player's hand; its sound (volume 2).
    pub accepts: bool,
    pub give_sound: Option<&'static str>,
    pub take_sound: Option<&'static str>,
    /// The taken item goes into the player's inventory (else it is thrown at the player).
    pub to_inventory: bool,
    /// The giver becomes the entity's owner (`LIKED_PLAYER`), forgotten on the take.
    pub liked: bool,
    /// The held item lives in the `Pocket` (the allay's) rather than in the main hand.
    pub pocket: bool,
}
impl HandOverParams {
    pub const ALLAY: Self = Self { accepts: true, give_sound: Some("minecraft:entity.allay.item_given"), take_sound: Some("minecraft:entity.allay.item_taken"), to_inventory: true, liked: true, pocket: true };
    pub const THROWS_BACK: Self = Self { accepts: false, give_sound: None, take_sound: None, to_inventory: false, liked: false, pocket: false };
}
pub static HAND_OVER: Rule = Rule { name: "hand_over", equip: |_, _, _| {}, attrs: no_attrs, interact };

fn interact(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let p = *params::<HandOverParams>(p);
    let held = if p.pocket { w.get::<crate::pocket::Pocket>(e).and_then(|k| k.held().cloned()) } else { main_hand(w, e).cloned() };
    let at = w.get::<Pos>(e).map_or(Vec3::new(0.0, 0.0, 0.0), |q| q.0);
    if held.is_none() && i.item != 0 && p.accepts {
        let Some(name) = crate::items::item_name(i.item) else { return false };
        put(w, e, p.pocket, Some(ItemStack::new(name, 1)));
        crate::give_item::take_from_hand(w, i);
        play(w, p.give_sound, at);
        if p.liked {
            if !w.get::<crate::tame::Owner>(e).is_some_and(|o| o.0 == Some(crate::targeting::Who::Player(i.player))) {
                crate::tame::set_owner(w, e, crate::targeting::Who::Player(i.player));
            }
            w.entity_mut(e).insert(Liked { ttl: None });
        }
        return true;
    }
    let Some(stack) = held else { return false };
    if i.item != 0 {
        return false;
    }
    put(w, e, p.pocket, None);
    play(w, p.take_sound, at);
    let to = w.resource::<Online>().get(i.player).map_or(at, |pl| Vec3::new(pl.x, pl.y, pl.z));
    if p.to_inventory {
        if p.liked {
            forget_liked(w, e);
        }
        w.resource_mut::<Events>().host.held_given(i.player, stack);
    } else {
        toss(w, e, stack, at, to);
    }
    true
}
/// The liked player's memory (`minecraft:liked_player`), present while the memory is: `ttl` is the ticks it has left under AI, `None` when it
/// never expires. The liked player is the entity's `Owner`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Liked {
    pub ttl: Option<i64>,
}
/// `Brain.eraseMemory` of the liked player: no memory, no owner, and the player's pets no longer hold the entity.
pub fn forget_liked(w: &mut World, e: Entity) {
    let was = w.get::<crate::tame::Owner>(e).and_then(|o| o.0);
    w.entity_mut(e).remove::<Liked>().insert(crate::tame::Owner(None));
    if let Some(crate::targeting::Who::Player(id)) = was
        && let Some(v) = w.resource_mut::<crate::tame::Pets>().0.get_mut(&id)
    {
        v.retain(|x| *x != e);
    }
}
/// `MemorySlot.tick` of the liked player: under AI a finite memory counts down, and the tick after 0 it is forgotten; `NoAi` freezes it.
pub fn liked_step(w: &mut World) {
    let mut gone = Vec::new();
    let mut q = w.query_filtered::<(Entity, &mut Liked), Without<crate::dragon::flight::NoAi>>();
    for (e, mut l) in q.iter_mut(w) {
        match l.ttl {
            Some(t) if t <= 0 => gone.push(e),
            Some(t) => l.ttl = Some(t - 1),
            None => {}
        }
    }
    for e in gone {
        forget_liked(w, e);
    }
}
/// Registers the systems of this file.
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(liked_step);
}
fn put(w: &mut World, e: Entity, pocket: bool, s: Option<ItemStack>) {
    if pocket {
        w.entity_mut(e).insert(crate::pocket::Pocket(s));
    } else {
        set_main_hand(w, e, s);
    }
}
fn play(w: &mut World, s: Option<&'static str>, at: Vec3) {
    if let Some(s) = s {
        w.resource_mut::<Events>().host.sound(s, [at.x, at.y, at.z], 2.0, 1.0);
    }
}
/// `BehaviorUtils.throwItem`: from 0.3 below the eyes, at 0.3 a tick toward the player.
fn toss(w: &mut World, e: Entity, s: ItemStack, at: Vec3, to: Vec3) {
    let eye = w.get::<Body>(e).map_or(0.0, |b| f64::from(b.height) * 0.85);
    let from = Vec3::new(at.x, at.y + eye - f64::from(0.3_f32), at.z);
    crate::item::drop_item(w, s, from, crate::rules::trade_with_villager::toss_velocity(from, to, f64::from(0.3_f32)), 10);
}
