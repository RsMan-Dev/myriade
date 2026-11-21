//! Dropped items and experience orbs as the ECS holds them: the spawns, the players' pickups by position, the host's effects of
//! the ends P reports. P's `item` and `orb` move, merge, expire and collect them; nothing here scans them.
use horizon_core::random::RandomSource as _;
use crate::game::{Game, pickups::{Collector, Taken}};
use bevy_ecs::entity::Entity;
use horizon_client::items::ItemStack;
use horizon_ecs_proto::item::ItemResistsFn;
use horizon_core::{geometry::Vec3, nbt::{Compound, Tag}};
use horizon_items::item_nbt::item_to_nbt;

const ITEM: &str = "minecraft:item";
const ORB: &str = "minecraft:experience_orb";
/// `Entity.spawnAtLocation`: a stack dropped without a delay waits 10 ticks.
const DROP_DELAY: i16 = 10;
/// The pickup reach around the feet, by position (P's `pickup`).
const REACH: f64 = 1.0;

impl Game<'_> {
    pub(in crate::game) fn ecs_drop_after(&mut self, stack: ItemStack, at: [f64; 3], velocity: [f64; 3], delay: i16) -> Option<u32> {
        let mut c = Compound::new();
        c.insert("Item".into(), item_to_nbt(&stack));
        c.insert("PickupDelay".into(), Tag::Short(delay));
        let e = self.ecs_with(|ecs, mw| ecs.spawn(mw, ITEM, at, Some(&c)))?;
        self.ecs.set_velocity(e, Vec3::new(velocity[0], velocity[1], velocity[2]));
        Some(self.ecs_id_of(e))
    }
    /// The host's table of what a stack on the ground resists (bit 0: fire, bit 1: explosion), `ItemStack.canBeHurtBy` for the fire and the
    /// blast. It captures the item models and the damage catalog, not `self`, so P reads it at every creation of an item.
    pub(in crate::game) fn item_resists_fn(&self) -> ItemResistsFn {
        let models = self.pack.item_renderer.clone();
        let catalog = self.health_resources.damage.clone();
        Box::new(move |stack| {
            let default = models.models.prototypes.get(stack.item.as_ref()).and_then(|components| components.get("minecraft:damage_resistant"));
            let hurt_by = |name: &str| horizon_client::item_damage::stack_can_be_hurt_by(default, &catalog, stack, name) == Some(false);
            u8::from(hurt_by("minecraft:in_fire")) | u8::from(hurt_by("minecraft:explosion")) << 1
        })
    }
    pub(in crate::game) fn ecs_drop(&mut self, stack: ItemStack, at: [f64; 3], velocity: [f64; 3]) -> Option<u32> {
        self.ecs_drop_after(stack, at, velocity, DROP_DELAY)
    }
    /// None when `amount` does not fit the orb's short value.
    pub(in crate::game) fn ecs_orb_moving(&mut self, amount: u32, at: [f64; 3], velocity: [f64; 3]) -> Option<u32> {
        let mut c = Compound::new();
        c.insert("Value".into(), Tag::Short(i16::try_from(amount).ok()?));
        c.insert("Count".into(), Tag::Int(1));
        let e = self.ecs_with(|ecs, mw| ecs.spawn(mw, ORB, at, Some(&c)))?;
        self.ecs.set_velocity(e, Vec3::new(velocity[0], velocity[1], velocity[2]));
        Some(self.ecs_id_of(e))
    }
    pub(in crate::game) fn ecs_orb(&mut self, amount: u32, at: [f64; 3]) -> Option<u32> {
        self.ecs_orb_moving(amount, at, [0.0; 3])
    }
    /// `Player.drop` (`Entity.spawnAtLocation` when `hook` is false: a death's scatter, which `item_toss` does not hear). The thrower
    /// is kept on the stack; a mod's cancel removes it again.
    pub(in crate::game) fn ecs_toss(&mut self, thrown: ItemStack, at: [f64; 3], v: [f64; 3], thrower: Option<[i32; 4]>, hook: bool) -> Option<u32> {
        let id = self.ecs_drop_after(thrown, at, v, 40)?;
        let e = self.ecs.entity_of(id)?;
        if let Some(t) = thrower {
            self.ecs.set_thrower(e, t);
        }
        if hook && mod_api::__private::listened(mod_api::HookId::ItemToss) && let Some(item) = self.native_item_handle(id) {
            let player = self.native_acting_id();
            let mut canceled = false;
            self.native_item_toss(player, item, &mut canceled);
            if canceled {
                self.ecs.despawn(e);
                return None;
            }
        }
        Some(id)
    }
    /// How many of `stack` the player's inventory takes (`Inventory.add`): a creative player takes all of it.
    fn pickup_room(&self, stack: &ItemStack) -> u32 {
        if self.creative {
            return stack.count as u32;
        }
        let max = self.res.items().get(&stack.item).map_or(64, |i| i.max_stack) as u32;
        let room: u32 = self
            .inventory
            .iter()
            .map(|slot| match slot {
                None => max,
                Some(s) if s.same_item_same_components(stack) => max.saturating_sub(s.count as u32),
                Some(_) => 0,
            })
            .sum();
        room.min(stack.count as u32)
    }
    /// P takes the stacks in reach of `feet` (`pickup_items`); the inventory takes what it can, the rest lies at `feet` again.
    pub(in crate::game) fn ecs_player_pickups(&mut self, feet: [f64; 3]) {
        let who = horizon_ecs_proto::targeting::Who::Player(self.own_entity_id() as u32);
        let uuid = self.acting_player_uuid();
        // The level leaves the game for the call, as `ecs_with` does; `room` reads the inventory as the game holds it.
        let mut ecs = std::mem::replace(&mut self.ecs, horizon_client::ecs_bridge::EcsLevel::inert());
        let taken = {
            let mw = self.mob_world_base(&[]);
            let game = &*self;
            ecs.pickup_items(&mw, who, uuid, feet, REACH, &mut |s| game.pickup_room(s))
        };
        self.ecs = ecs;
        for (id, stack, at) in taken {
            let kind = stack.item.clone();
            let count = stack.count;
            let rest = self.native_pickup_insert(&stack);
            let left = rest.as_ref().map_or(0, |r| r.count);
            if let Some(rest) = rest {
                self.ecs_drop_after(rest, feet, [0.0; 3], DROP_DELAY);
            }
            let got = count.saturating_sub(left);
            if got == 0 {
                continue;
            }
            self.award_stat("picked_up", &kind, got as i32);
            self.native_entity_left(id);
            self.add_pickup(Taken::Ecs { id, count: got as i32, at, kind: "item" }, Collector::Player);
            let mut random = horizon_client::random_source::create();
            let pitch = (random.next_float() - random.next_float()) * 1.4 + 2.0;
            self.audio.play("minecraft:entity.item.pickup", Some(feet), 0.2, pitch);
        }
    }
    /// One orb of `feet`'s reach every 2 ticks (`ExperienceOrb.playerTouch`): P gives its value (`collect_orb`), the host the points.
    pub(in crate::game) fn ecs_player_orbs(&mut self, feet: Option<[f64; 3]>) {
        let Some(feet) = feet.filter(|_| self.xp_delay == 0) else { return };
        let touching = self.ecs_with(|ecs, mw| ecs.pickup_orbs(mw, feet, REACH));
        let Some(&orb) = touching.first() else { return };
        let at = self.ecs.position(orb).unwrap_or(feet);
        let Some(points) = self.ecs_with(|ecs, mw| ecs.collect_orb(mw, orb)) else { return };
        self.xp_delay = 2;
        if !self.ecs.exists(orb) {
            self.native_entity_left(orb);
        }
        self.add_pickup(Taken::Ecs { id: orb, count: 1, at, kind: "orb" }, Collector::Player);
        let mut random = horizon_client::random_source::create();
        let pitch = (random.next_float() - random.next_float()) * 0.35 + 0.9;
        self.audio.play("minecraft:entity.experience_orb.pickup", Some(at), 0.1, pitch);
        let gained = self.mending(points);
        if gained > 0 {
            self.give_experience_points(gained as i32);
        }
    }
    /// The end of life of an item of P's (`HostEvent::ItemExpiring`, sent before P removes it): the mods may extend it; else it goes.
    pub(in crate::game) fn ecs_item_expiring(&mut self, e: Entity, stack: &ItemStack, at: [f64; 3]) {
        let id = self.ecs_id_of(e);
        let mut extra = 0;
        self.native_item_expire(id, stack, &mut extra);
        if extra > 0 {
            self.ecs.extend_item_life(e, extra.min(32766));
            return;
        }
        if self.serving.is_some() || self.options.dedicated.is_some() {
            self.native_item_despawn(id, &stack.item, stack.count as i32, at);
        }
        self.ecs.despawn(e);
        self.native_entity_left(id);
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_client::items::ItemStack;
    use horizon_core::nbt::{Compound, Tag};
    use horizon_ecs_proto::bridge::host::HostSetup;
    use horizon_ecs_proto::host_out::HostEvent;

    fn level(game: &mut crate::game::Game<'_>) -> EcsLevel {
        game.test_ecs_level()
    }

    #[test]
    fn a_dropped_stack_is_taken_back_by_a_player_at_its_feet() {
        with_game(|game| {
            game.ecs = level(game);
            let at = [0.5, 64.0, 0.5];
            let id = game.ecs_drop(ItemStack::new("minecraft:stone".into(), 3), at, [0.0; 3]).unwrap();
            assert_eq!(game.ecs.kind(id), Some("minecraft:item"));
            game.ecs_player_pickups(at);
            assert!(!game.ecs.exists(id), "the stack was taken: the level no longer holds it");
        });
    }

    #[test]
    fn two_orbs_at_one_place_merge_into_one() {
        with_game(|game| {
            game.ecs = level(game);
            let at = [0.5, 64.0, 0.5];
            let (a, b) = (game.ecs_orb(3, at).unwrap(), game.ecs_orb(4, at).unwrap());
            for _ in 0..40 {
                game.ecs_with(|ecs, mw| ecs.step(mw));
            }
            assert_eq!(u8::from(game.ecs.exists(a)) + u8::from(game.ecs.exists(b)), 1);
        });
    }

    #[test]
    fn a_falling_block_lands_as_a_set_block() {
        with_game(|game| {
            game.ecs = level(game);
            let mut state = Compound::new();
            state.insert("Name".into(), Tag::String("minecraft:sand".into()));
            let mut c = Compound::new();
            c.insert("BlockState".into(), Tag::Compound(state));
            game.ecs_with(|ecs, mw| ecs.spawn(mw, "minecraft:falling_block", [0.5, 80.0, 0.5], Some(&c))).unwrap();
            let events: Vec<HostEvent> = (0..800).flat_map(|_| game.ecs_with(|ecs, mw| ecs.step(mw))).collect();
            assert!(events.iter().any(|e| matches!(e, HostEvent::SetBlock { .. })));
        });
    }
}
