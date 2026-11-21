//! Minecart handles for native server mods. A cart is an ECS entity named by its network id: reads and writes go through the bridge.
use super::Game;
use horizon_ecs_proto::minecart::Kind;
use horizon_world::registry::BlockStateId;
use mod_api::{EntityRef, MinecartKind, MinecartRef};

impl Game<'_> {
    /// The network id and genre of the cart a handle names: a cart the ECS holds now. None for a stale handle, another entity, or a client.
    fn native_cart(&self, cart: MinecartRef) -> Option<(u32, Kind)> {
        let entity = cart.entity();
        if self.native_lifecycle.epoch() != Some(entity.epoch()) {
            return None;
        }
        let kind = self.ecs.cart_genre(entity.id())?;
        Some((entity.id(), kind))
    }

    /// Narrows an entity to a minecart of any kind, or None for another kind of
    /// entity, a stale handle, or a client.
    pub(super) fn minecart(&self, entity: EntityRef) -> Option<MinecartRef> {
        let cart = MinecartRef::new(entity);
        self.native_cart(cart).map(|_| cart)
    }

    /// Which `AbstractMinecart` subclass the cart is.
    pub(super) fn minecart_kind(&self, cart: MinecartRef) -> Option<MinecartKind> {
        Some(match self.native_cart(cart)?.1 {
            Kind::Rideable => MinecartKind::Rideable,
            Kind::Chest => MinecartKind::Chest,
            Kind::Furnace => MinecartKind::Furnace,
            Kind::Tnt => MinecartKind::Tnt,
            Kind::Hopper => MinecartKind::Hopper,
            Kind::CommandBlock => MinecartKind::CommandBlock,
            Kind::Spawner => MinecartKind::Spawner,
        })
    }

    /// `AbstractMinecart.getMaxSpeed`, by the cart's medium and genre.
    pub(super) fn minecart_max_speed(&self, cart: MinecartRef) -> Option<f64> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_max_speed(id)
    }

    /// `AbstractMinecart.isOnRails`: a rail under the cart, read at call time.
    pub(super) fn minecart_is_on_rails(&self, cart: MinecartRef) -> Option<bool> {
        let (id, _) = self.native_cart(cart)?;
        let mw = self.mob_world_base(&[]);
        self.ecs.cart_on_rails(&mw, id)
    }

    /// `AbstractMinecart.isFlipped`, the ECS `Flipped` state.
    pub(super) fn minecart_flipped(&self, cart: MinecartRef) -> Option<bool> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_flipped(id)
    }

    /// `AbstractMinecart.setFlipped`: false for a stale handle or a client.
    pub(super) fn minecart_set_flipped(&mut self, cart: MinecartRef, flipped: bool) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        self.ecs.set_cart_flipped(id, flipped)
    }

    /// `VehicleEntity.getDamage`, the wobble damage: the ECS `CartHurt`, 0 before any blow.
    pub(super) fn minecart_damage(&self, cart: MinecartRef) -> Option<f32> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_damage(id)
    }

    /// `VehicleEntity.setDamage`: false for a stale handle or a client.
    pub(super) fn minecart_set_damage(&mut self, cart: MinecartRef, damage: f32) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        self.ecs.set_cart_damage(id, damage)
    }

    /// `VehicleEntity.getHurtTime`, the wobble ticks: the ECS `CartHurt`.
    pub(super) fn minecart_hurt_time(&self, cart: MinecartRef) -> Option<i32> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_hurt_time(id)
    }

    /// `VehicleEntity.setHurtTime`: false for a stale handle or a client.
    pub(super) fn minecart_set_hurt_time(&mut self, cart: MinecartRef, ticks: i32) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        self.ecs.set_cart_hurt_time(id, ticks)
    }

    /// `AbstractMinecart.customDisplayBlockState`: None for the cart's own display.
    pub(super) fn minecart_custom_display_block(&self, cart: MinecartRef) -> Option<Option<BlockStateId>> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_display(self.pack.assets.registry, id).map(|(state, _)| state)
    }

    /// `AbstractMinecart.setCustomDisplayBlock`, the offset kept: false for a stale handle or a client.
    pub(super) fn minecart_set_custom_display_block(&mut self, cart: MinecartRef, state: Option<BlockStateId>) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        let registry = self.pack.assets.registry;
        let Some((_, offset)) = self.ecs.cart_display(registry, id) else {
            return false;
        };
        self.ecs.set_cart_display(registry, id, state, offset)
    }

    /// `AbstractMinecart.getDisplayOffset`, in sixteenths of a block.
    pub(super) fn minecart_display_offset(&self, cart: MinecartRef) -> Option<i32> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_display(self.pack.assets.registry, id).map(|(_, offset)| offset)
    }

    /// `AbstractMinecart.setDisplayOffset`, the block kept: false for a stale handle or a client.
    pub(super) fn minecart_set_display_offset(&mut self, cart: MinecartRef, offset: i32) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        let registry = self.pack.assets.registry;
        let Some((state, _)) = self.ecs.cart_display(registry, id) else {
            return false;
        };
        self.ecs.set_cart_display(registry, id, state, offset)
    }

    /// `MinecartFurnace`'s fuel ticks: None for another cart.
    pub(super) fn minecart_fuel(&self, cart: MinecartRef) -> Option<i32> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_fuel(id)
    }

    /// Sets a furnace minecart's fuel ticks: false for another cart.
    pub(super) fn minecart_set_fuel(&mut self, cart: MinecartRef, fuel: i32) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        self.ecs.set_cart_fuel(id, fuel)
    }

    /// `MinecartFurnace`'s push: None for another cart.
    pub(super) fn minecart_push(&self, cart: MinecartRef) -> Option<[f64; 3]> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_furnace_push(id)
    }

    /// Sets a furnace minecart's push (the value, not an impulse): false for another cart.
    pub(super) fn minecart_set_push(&mut self, cart: MinecartRef, push: [f64; 3]) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        self.ecs.set_cart_furnace_push(id, push)
    }

    /// `MinecartTNT`'s fuse ticks: -1 unprimed, None for another cart.
    pub(super) fn minecart_fuse(&self, cart: MinecartRef) -> Option<i32> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_fuse(id)
    }

    /// `MinecartTNT.primeFuse(null)`: false for another cart or one primed already.
    pub(super) fn minecart_prime_fuse(&mut self, cart: MinecartRef) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        self.ecs.prime_cart_fuse(id)
    }

    /// `MinecartHopper.isEnabled`: None for another cart.
    pub(super) fn minecart_hopper_enabled(&self, cart: MinecartRef) -> Option<bool> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_hopper_enabled(id)
    }

    /// `MinecartHopper.setEnabled`: false for another kind of cart.
    pub(super) fn minecart_set_hopper_enabled(&mut self, cart: MinecartRef, enabled: bool) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        self.ecs.set_cart_hopper_enabled(id, enabled)
    }

    /// A command block minecart's command, from the ECS `CommandBlock`; None for another kind of cart.
    pub(super) fn minecart_command(&self, cart: MinecartRef) -> Option<String> {
        let (id, _) = self.native_cart(cart)?;
        self.ecs.cart_command(id).map(|c| c.command)
    }

    /// Sets a command block minecart's command (`BaseCommandBlock.setCommand`, which resets the success count), as
    /// the ECS state holds it. False for another kind of cart.
    pub(super) fn minecart_set_command(&mut self, cart: MinecartRef, command: &str) -> bool {
        let Some((id, _)) = self.native_cart(cart) else {
            return false;
        };
        let Some(mut state) = self.ecs.cart_command(id) else {
            return false;
        };
        state.set_command(command);
        self.ecs.set_cart_command(id, &state)
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use super::*;
    use horizon_client::entities::ArmorStand;
    use horizon_core::geometry::Vec3;
    use horizon_ecs_proto::minecart::FUSE;
    use mod_api::WorldEpoch;

    /// Spawns a cart of the species `name` with the network id `id` in the ECS, and returns its handle.
    fn spawn(game: &mut Game<'_>, id: u32, name: &str) -> MinecartRef {
        let spawned = game.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, name, [0.0; 3], 0.0, &mut || id));
        assert_eq!(spawned, Some(id));
        game.minecart(game.entity(id).unwrap()).unwrap()
    }

    #[test]
    fn furnace_fuel_adds_with_java_int_arithmetic() {
        assert_eq!(super::super::minecarts::furnace_fuel_after(0), Some(3600));
        assert_eq!(
            super::super::minecarts::furnace_fuel_after(28_400),
            Some(32_000)
        );
        assert_eq!(super::super::minecarts::furnace_fuel_after(28_401), None);
        assert_eq!(
            super::super::minecarts::furnace_fuel_after(i32::MAX),
            Some(i32::MIN + 3599)
        );
    }

    #[test]
    #[ignore = "daily"]
    fn a_furnace_carts_push_is_read_and_set_through_its_handle() {
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            let furnace = spawn(game, 20, "minecraft:furnace_minecart");
            assert_eq!(furnace.push(game), Some([0.0; 3]));
            assert!(furnace.set_push(game, [0.5, 0.0, -0.25]));
            assert_eq!(furnace.push(game), Some([0.5, 0.0, -0.25]));
            let rideable = spawn(game, 21, "minecraft:minecart");
            assert_eq!(rideable.push(game), None);
            assert!(!rideable.set_push(game, [1.0, 0.0, 0.0]));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_carts_display_block_and_offset_round_trip_through_its_handle() {
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            let chest = spawn(game, 30, "minecraft:chest_minecart");
            assert_eq!(chest.display_offset(game), Some(8));
            assert_eq!(chest.custom_display_block(game), Some(None));
            let stone = game.pack.assets.registry.default_state("minecraft:stone");
            assert!(chest.set_custom_display_block(game, stone));
            assert_eq!(chest.custom_display_block(game), Some(stone));
            assert!(chest.set_display_offset(game, 12));
            assert_eq!(chest.display_offset(game), Some(12));
            assert_eq!(chest.custom_display_block(game), Some(stone), "the offset write keeps the block");
            assert!(chest.set_custom_display_block(game, None));
            assert_eq!(chest.custom_display_block(game), Some(None));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn minecart_handles_narrow_read_and_write_by_kind() {
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            let names = [
                ("minecraft:minecart", MinecartKind::Rideable),
                ("minecraft:chest_minecart", MinecartKind::Chest),
                ("minecraft:furnace_minecart", MinecartKind::Furnace),
                ("minecraft:tnt_minecart", MinecartKind::Tnt),
                ("minecraft:hopper_minecart", MinecartKind::Hopper),
                ("minecraft:command_block_minecart", MinecartKind::CommandBlock),
                ("minecraft:spawner_minecart", MinecartKind::Spawner),
            ];
            let carts: Vec<_> = names
                .iter()
                .zip(10..)
                .map(|((name, _), id)| spawn(game, id, name))
                .collect();
            for (cart, (_, expected)) in carts.iter().zip(names) {
                assert_eq!(cart.kind(game), Some(expected));
            }
            let rideable = carts[0];
            let furnace = carts[2];
            let tnt = carts[3];
            let hopper = carts[4];
            let command = carts[5];
            game.armor_stands.push(ArmorStand::new(99, Vec3::ZERO, 0.0));
            assert!(game.minecart(game.entity(99).unwrap()).is_none());

            assert_eq!(rideable.flipped(game), Some(false));
            assert_eq!(rideable.damage(game), Some(0.0));
            assert_eq!(rideable.max_speed(game), Some(0.4));
            assert_eq!(furnace.max_speed(game), Some(0.2));
            assert!(rideable.set_hurt_time(game, 10));
            assert_eq!(rideable.hurt_time(game), Some(10));
            assert!(rideable.set_damage(game, 4.0));
            assert_eq!(rideable.damage(game), Some(4.0));
            assert!(rideable.set_flipped(game, true));
            assert_eq!(rideable.flipped(game), Some(true));

            assert_eq!(furnace.fuel(game), Some(0));
            assert!(furnace.set_fuel(game, 3600));
            assert_eq!(furnace.fuel(game), Some(3600));
            assert_eq!(rideable.fuel(game), None);
            assert!(!rideable.set_fuel(game, 1));

            assert_eq!(tnt.fuse(game), Some(-1));
            assert!(tnt.prime_fuse(game));
            assert_eq!(tnt.fuse(game), Some(FUSE));
            assert!(!tnt.prime_fuse(game), "primed already");
            assert_eq!(rideable.fuse(game), None);
            assert!(!rideable.prime_fuse(game));

            assert_eq!(hopper.hopper_enabled(game), Some(true));
            assert!(hopper.set_hopper_enabled(game, false));
            assert_eq!(hopper.hopper_enabled(game), Some(false));
            assert_eq!(rideable.hopper_enabled(game), None);
            assert!(!rideable.set_hopper_enabled(game, true));

            assert_eq!(command.command(game).as_deref(), Some(""));
            assert!(command.set_command(game, "gamerule mob_griefing false"));
            assert_eq!(
                command.command(game).as_deref(),
                Some("gamerule mob_griefing false")
            );
            assert!(
                game.session
                    .rules
                    .bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING)
            );
            assert!(game.perform_cart_command(command.entity().id()));
            assert!(
                !game
                    .session
                    .rules
                    .bool_at(horizon_client::gamerules::vanilla::MOB_GRIEFING),
                "the new command ran"
            );
            assert!(!rideable.set_command(game, "say hi"));
            assert_eq!(rideable.command(game), None);

            let stale = MinecartRef::new(EntityRef::new(
                WorldEpoch::new(rideable.entity().epoch().get() + 1),
                rideable.entity().id(),
            ));
            assert_eq!(stale.kind(game), None);
            assert_eq!(stale.command(game), None);
        });
    }
}
