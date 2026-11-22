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
    /// blast. It captures the item defaults and the damage catalog, not `self`, so P reads it at every creation of an item.
    pub(in crate::game) fn item_resists_fn(&self) -> ItemResistsFn {
        let res = self.res.clone();
        let catalog = self.health_resources.damage.clone();
        Box::new(move |stack| {
            let default = res.item_defaults().entries().get(stack.item.as_ref()).and_then(|components| components.get("minecraft:damage_resistant"));
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
            // The take left what the room did not hold under `id`: the whole stack is what the inventory is offered.
            let left_on_ground = self.ecs.item(id).map_or(0, |v| v.stack.count);
            let mut whole = stack.clone();
            whole.count = stack.count + left_on_ground;
            let rest = self.native_pickup_insert(&whole);
            if rest.as_ref().map_or(0, |r| r.count) == whole.count {
                let back = rest.unwrap_or(whole);
                if left_on_ground > 0 {
                    self.ecs.set_item_stack(id, back);
                } else {
                    self.ecs_drop_after(back, feet, [0.0; 3], DROP_DELAY);
                }
                continue;
            }
            // `item_picked_up_post` for a whole pickup, `item_picked_up_partial` for a part: what a mod leaves lies at the feet.
            let kept = match rest {
                None => self.native_pickup_post(id, &whole),
                Some(rest) => self.native_pickup_partial(&whole, rest),
            };
            // What the listeners leave on the ground is not picked up.
            let kept_count = kept.as_ref().map_or(0, |k| k.count);
            // A listener that kills the item leaves no take animation (the stack was on the ground, now gone).
            let killed = left_on_ground > 0 && self.ecs.item(id).is_none();
            match (kept, left_on_ground > 0) {
                (Some(kept), true) => {
                    self.ecs.set_item_stack(id, kept);
                }
                (Some(kept), false) => {
                    self.ecs_drop_after(kept, feet, [0.0; 3], DROP_DELAY);
                }
                (None, true) => {
                    if let Some(e) = self.ecs.entity_of(id) {
                        self.ecs.despawn(e);
                    }
                }
                (None, false) => {}
            }
            let got = whole.count - kept_count;
            self.award_stat("picked_up", &kind, got as i32);
            if left_on_ground == 0 {
                self.native_entity_left(id);
            }
            if !killed {
                self.add_pickup(Taken::Ecs { id, count: got as i32, at, kind: "item" }, Collector::Player);
            }
            let mut random = horizon_client::random_source::create();
            let pitch = (random.next_float() - random.next_float()) * 1.4 + 2.0;
            self.audio.play("minecraft:entity.item.pickup", Some(feet), 0.2, pitch);
        }
    }
    /// One orb of `feet`'s reach every 2 ticks (`ExperienceOrb.playerTouch`): P gives its value (`collect_orb`), the host the points.
    pub(in crate::game) fn ecs_player_orbs(&mut self, feet: Option<[f64; 3]>) {
        let Some(feet) = feet.filter(|_| self.xp_delay == 0) else { return };
        let mut next = self.next_entity;
        let touching = self.ecs_with(|ecs, mw| ecs.pickup_orbs(mw, feet, REACH, &mut || { next += 1; next }));
        self.next_entity = next;
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
            self.ecs.extend_item_life(e, extra.min(32766) as u32);
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
            let id = game.ecs_drop_after(ItemStack::new("minecraft:stone".into(), 3), at, [0.0; 3], 0).unwrap();
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
            let a = game.ecs_orb_moving(3, at, [0.0; 3]).unwrap();
            // `ExperienceOrb.canMerge`: the same value and ids a multiple of 40 apart; 39 others stand between the two.
            for _ in 0..39 {
                game.ecs_orb_moving(9, [100.5, 64.0, 100.5], [0.0; 3]).unwrap();
            }
            let b = game.ecs_orb_moving(3, at, [0.0; 3]).unwrap();
            assert_eq!((b - a) % 40, 0, "ids {a} and {b}");
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
            if game.world.chunks.get(&(0, 0)).is_none() {
                game.world.chunks.insert((0, 0), std::sync::Arc::new(horizon_client::chunk::Chunk::empty(0, 0)));
            }
            let stone = game.pack.assets.registry.default_state("minecraft:stone").unwrap();
            game.world.set(0, 60, 0, stone).unwrap();
            // The state in the world's own NBT form (`BlockState.to_nbt`).
            let reg = &game.pack.assets.registry;
            let state = reg.state(reg.default_state("minecraft:sand").unwrap()).unwrap().to_nbt();
            let mut c = Compound::new();
            c.insert("BlockState".into(), state);
            game.ecs_with(|ecs, mw| ecs.spawn(mw, "minecraft:falling_block", [0.5, 80.0, 0.5], Some(&c))).unwrap();
            let events: Vec<HostEvent> = (0..800).flat_map(|_| game.ecs_with(|ecs, mw| ecs.step(mw))).collect();
            assert!(events.iter().any(|e| matches!(e, HostEvent::SetBlock { .. })));
        });
    }

    fn compound(entries: Vec<(&str, Tag)>) -> Tag {
        Tag::compound(entries)
    }

    fn stack_of(id: &str, count: i32) -> Tag {
        compound(vec![("id", Tag::String(id.into())), ("count", Tag::Int(count))])
    }

    /// A shulker box holding two stacks, named: the components the old `save_item` wrote beside the stack's own fields.
    fn full_shulker_box() -> ItemStack {
        let slots = Tag::List(vec![
            compound(vec![("slot", Tag::Int(0)), ("item", stack_of("minecraft:diamond", 3))]),
            compound(vec![("slot", Tag::Int(5)), ("item", stack_of("minecraft:stone", 64))]),
        ]);
        let components = compound(vec![("minecraft:container", slots), ("minecraft:custom_name", Tag::String("Boxy".into()))]);
        let tag = compound(vec![("id", Tag::String("minecraft:shulker_box".into())), ("count", Tag::Int(1)), ("components", components)]);
        horizon_items::item_nbt::item_from_nbt(&tag).expect("a shulker box")
    }

    /// Far from the player at (0.5, 64, 0.5) (no pickup, no merge), in chunk (0, 0): an item, an orb and a cow are saved and
    /// loaded again with their content.
    #[test]
    fn a_dropped_shulker_box_keeps_its_contents_through_the_chunk_save_and_load() {
        with_game(|game| {
            game.ecs = level(game);
            let before = full_shulker_box();
            let wrote = format!("{:?}", horizon_items::item_nbt::item_to_nbt(&before));
            assert!(wrote.len() > 300, "the stack encodes its contents: {wrote}");
            game.ecs_drop(before.clone(), [14.5, 64.0, 14.5], [0.0; 3]).unwrap();
            game.ecs_orb_moving(7, [12.5, 64.0, 14.5], [0.0; 3]).unwrap();
            game.ecs_with(|ecs, mw| ecs.spawn(mw, "minecraft:cow", [10.5, 64.0, 14.5], None)).expect("a cow");
            game.ecs_with(|ecs, mw| ecs.step(mw));
            let saved = game.ecs_save_chunk(0, 0, Vec::new());
            let ids: Vec<_> = saved.iter().map(|c| format!("{:?}", c.get(&"id".into()))).collect();
            assert_eq!(saved.len(), 3, "the chunk saves the item, the orb and the cow: {ids:?}");
            game.ecs = level(game);
            for c in &saved {
                game.ecs_load(c).expect("the entity loads again");
            }
            let views = game.ecs.item_views();
            assert_eq!(views.len(), 1);
            let after = format!("{:?}", horizon_items::item_nbt::item_to_nbt(&views[0].stack));
            assert_eq!(after, wrote, "the reloaded stack keeps the box's contents");
            assert_eq!(game.ecs_save_chunk(0, 0, Vec::new()).len(), 3, "the three are back and saved again");
        });
    }
}
