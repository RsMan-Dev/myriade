//! An entity's state flags for native server mods, through an `EntityRef`
//! (docs/modding/src/how-to/native-mods.md, "Entity and level handles", #7387): sneaking and
//! sprinting, water and lava, fire and air, silence, invisibility and pose.
//! Players and mobs have them all; the other kinds have the ones Horizon
//! keeps for them (an armor stand's fire and invisibility, an item's fire),
//! and `None` for the rest, as for a stale handle. Secondary use also reads
//! the local client's live semantic sneak state; the other flags are server-only.
//! Ticks lived (`Entity.tickCount`) are read for mobs, items, orbs and minecarts.
use super::{Game, commands::Target};
use horizon_client::mobs::{Species, sync};
use horizon_client::motion::PlayerMotion;
use horizon_client::pose::Pose;
use horizon_core::nbt::Tag;
use mod_api::EntityRef;

/// `Pose`'s serialized names by id (`Pose.STANDING` is 0).
pub(super) const POSES: [&str; 18] = [
    "standing",
    "fall_flying",
    "sleeping",
    "swimming",
    "spin_attack",
    "crouching",
    "long_jumping",
    "dying",
    "croaking",
    "using_tongue",
    "sitting",
    "roaring",
    "sniffing",
    "emerging",
    "digging",
    "sliding",
    "shooting",
    "inhaling",
];

/// `Entity.getMaxAirSupply`: a dolphin's 4800, an axolotl's 6000, else 300.
pub(super) fn max_air(species: Option<Species>) -> i32 {
    match species {
        Some(Species::Dolphin) => 4800,
        Some(Species::Axolotl) => horizon_client::mobs::axolotl::MAX_AIR,
        _ => 300,
    }
}

/// A player's pose as `Pose` names it.
fn player_pose(pose: Pose) -> &'static str {
    match pose {
        Pose::Standing => "standing",
        Pose::Crouching => "crouching",
        Pose::Swimming => "swimming",
        Pose::FallFlying => "fall_flying",
        Pose::SpinAttack => "spin_attack",
        Pose::Sleeping => "sleeping",
    }
}

/// What a handle names, as the flags read it.
enum Body<'a> {
    Player(&'a PlayerMotion),
    /// A mob of this level's ECS world, read by network id through its NBT.
    Ecs(u32),
    Other(Target),
}

impl Game<'_> {
    /// The player's body or the mob a handle names, or its target.
    fn native_entity_body(&self, entity: EntityRef) -> Option<Body<'_>> {
        self.native_target_body(self.native_entity_target(entity)?)
    }

    fn native_target_body(&self, target: Target) -> Option<Body<'_>> {
        Some(match target {
            Target::Player => Body::Player(self.player.as_ref()?),
            Target::Guest(conn) => Body::Player(self.command_guest(conn)?.motion.as_deref()?),
            Target::Mob(id) if self.ecs_has(id) => Body::Ecs(id),
            t => Body::Other(t),
        })
    }
    /// `isSecondaryUseActive`: the server's shift-key flag. False for non-player
    /// entities, None for a stale or unreachable handle.
    pub(super) fn entity_is_shift_key_down(&self, entity: EntityRef) -> Option<bool> {
        Some(match self.native_entity_body(entity)? {
            Body::Player(p) => p.shift_key_down,
            _ => false,
        })
    }
    /// Whether the entity crouches (`Entity.isCrouching`: the crouching
    /// pose).
    pub(super) fn entity_is_crouching(&self, entity: EntityRef) -> Option<bool> {
        Some(self.entity_pose(entity)? == "crouching")
    }
    /// Whether the entity sprints (`Entity.isSprinting`): a player, or a
    /// cat or ocelot dashing after its target (their shared sprinting flag);
    /// false for the other kinds.
    pub(super) fn entity_is_sprinting(&self, entity: EntityRef) -> Option<bool> {
        Some(match self.native_entity_body(entity)? {
            Body::Player(p) => p.sprinting,
            _ => false,
        })
    }
    /// Whether the entity touched water at its last fluid update
    /// (`Entity.isInWater`): players and mobs.
    pub(super) fn entity_is_in_water(&self, entity: EntityRef) -> Option<bool> {
        match self.native_entity_body(entity)? {
            Body::Player(p) => Some(p.fluids.was_touching_water),
            Body::Ecs(_) | Body::Other(_) => None,
        }
    }
    /// Whether the entity is in lava (`Entity.isInLava`): players and mobs.
    pub(super) fn entity_is_in_lava(&self, entity: EntityRef) -> Option<bool> {
        match self.native_entity_body(entity)? {
            Body::Player(p) => Some(p.is_in_lava()),
            Body::Ecs(_) | Body::Other(_) => None,
        }
    }
    /// The fluids the entity touched at its last fluid update, each with how deep its box is in
    /// it (`IEntityExtension.getFluidTypeHeight`), highest first; empty in no fluid. Players and
    /// mobs: water and lava, the fluids Horizon's entity update tracks. On a client these
    /// are the predicted local player's state or the displayed entity's last fluid update.
    pub(super) fn entity_fluid_heights(&self, entity: EntityRef) -> Option<Vec<(String, f64)>> {
        use horizon_client::motion::{LAVA, WATER};
        Some(
            match self.native_target_body(self.native_entity_read_target(entity)?)? {
                Body::Player(p) => touched_fluids(
                    (p.fluid_height(WATER), p.fluids.was_touching_water),
                    (p.fluid_height(LAVA), p.is_in_lava()),
                ),
                // An ECS mob's fluid state has no bridge read yet (listed in the report).
                Body::Ecs(_) | Body::Other(_) => return None,
            },
        )
    }
    /// The fluid at the entity's eyes from its last fluid update
    /// (`IEntityExtension.getFirstEyeInFluidType`): water or lava, none in air. Players and mobs.
    /// On a client this reads its own cached fluid update, without sampling the world again.
    pub(super) fn entity_eye_fluid(&self, entity: EntityRef) -> Option<Option<String>> {
        use horizon_client::motion::{LAVA, WATER};
        Some(
            match self.native_target_body(self.native_entity_read_target(entity)?)? {
                Body::Player(p) => eye_fluid(p.is_eye_in_fluid(WATER), p.is_eye_in_fluid(LAVA)),
                Body::Ecs(_) | Body::Other(_) => return None,
            },
        )
    }
    /// Whether the entity's type never burns (`EntityType.fireImmune`):
    /// players and mobs.
    pub(super) fn entity_fire_immune(&self, entity: EntityRef) -> Option<bool> {
        match self.native_entity_body(entity)? {
            Body::Player(_) => Some(false),
            Body::Ecs(_) | Body::Other(_) => None,
        }
    }
    /// The ticks the entity burns for (`Entity.getRemainingFireTicks`):
    /// players, mobs, armor stands and items.
    pub(super) fn entity_fire_ticks(&self, entity: EntityRef) -> Option<i32> {
        match self.native_entity_body(entity)? {
            Body::Player(p) => Some(p.fire_ticks),
            Body::Ecs(id) => self.ecs_number(id, "Fire"),
            Body::Other(Target::Stand(id)) => {
                Some(self.armor_stand_saved(id)?.fire_ticks)
            }
            Body::Other(Target::Item(id)) => {
                Some(self.ecs.item(id)?.fire_ticks)
            }
            Body::Other(_) => None,
        }
    }
    /// `Entity.getPortalCooldown`: a player's own portal state, a pearl's End gateway state, or the level's for the other
    /// kinds its portal walk carries (zero when it keeps no state for one).
    pub(super) fn entity_portal_cooldown(&self, entity: EntityRef) -> Option<i32> {
        let id = match self.native_entity_target(entity)? {
            // The acting guest's state is in place for its turn (`enter_guest`).
            Target::Player => return Some(self.portals.player.cooldown),
            Target::Guest(conn) => {
                let play = self.lan.as_ref()?.play.get(&conn);
                return Some(play.map_or(0, |p| p.portal.cooldown));
            }
            Target::Projectile(id) => {
                if self.ecs.kind(id) == Some("minecraft:ender_pearl") {
                    return Some(self.pearl_portal_cooldown(id));
                }
                // A client-side projectile has no portal state: the server's go through the ECS.
                return None;
            }
            Target::Mob(id)
            | Target::Item(id)
            | Target::Orb(id)
            | Target::Boat(id)
            | Target::Cart(id)
            | Target::Tnt(id)
            | Target::Falling(id)
            | Target::Rocket(id)
            | Target::Trident(id) => id,
            _ => return None,
        };
        Some(self.portals.entities.get(&id).map_or(0, |p| p.cooldown))
    }
    /// `Entity.setPortalCooldown`: writes the value `entity_portal_cooldown`
    /// reads, where that read keeps it — the level's portal state for the
    /// kinds its portal walk carries (mobs, items, experience orbs, boats,
    /// minecarts, primed TNT, falling blocks, fireworks, tridents, arrows and the thrown
    /// items and fireballs a chunk saves). Server-only; a negative value or a
    /// client-side write is refused. A positive value blocks portal entry on
    /// the next tick (and is refilled while the entity stands in a portal);
    /// 0 lets it enter at once. The write refuses a player, an acting guest,
    /// an ender pearl: their cooldown lives in a store of its own (the player
    /// and guest portal states, an End gateway traveller) that this write does
    /// not own, so it needs its own owner decision before it can reach them. False for a kind
    /// the read refuses or a stale handle.
    pub(super) fn entity_set_portal_cooldown(&mut self, entity: EntityRef, ticks: i32) -> bool {
        if ticks < 0 || !self.is_logical_server() {
            return false;
        }
        match self.native_entity_target(entity) {
            // A client-side projectile has no portal state: the server's go through the ECS.
            Some(Target::Projectile(_)) => false,
            Some(Target::Mob(id))
            | Some(Target::Item(id))
            | Some(Target::Orb(id))
            | Some(Target::Boat(id))
            | Some(Target::Cart(id))
            | Some(Target::Tnt(id))
            | Some(Target::Falling(id))
            | Some(Target::Rocket(id))
            | Some(Target::Trident(id)) => {
                self.portals.entities.entry(id).or_default().cooldown = ticks;
                true
            }
            _ => false,
        }
    }
    /// Sets the remaining fire ticks (`Entity.setRemainingFireTicks`).
    pub(super) fn entity_set_remaining_fire_ticks(
        &mut self,
        entity: EntityRef,
        ticks: i32,
    ) -> bool {
        match self.native_entity_target(entity) {
            Some(Target::Player) => self.player.as_mut().is_some_and(|p| {
                p.fire_ticks = ticks;
                true
            }),
            Some(Target::Guest(conn)) => self
                .lan
                .as_mut()
                .and_then(|lan| lan.hub.guests.motion_mut(conn))
                .is_some_and(|p| {
                    p.fire_ticks = ticks;
                    true
                }),
            Some(Target::Mob(id)) if self.ecs_has(id) => {
                self.ecs_put(id, "Fire", Tag::Short(ticks as i16))
            }
            Some(Target::Stand(id)) => self.armor_stand_edit(id, |s| s.fire_ticks = ticks),
            Some(Target::Item(id)) => self.ecs_put(id, "Fire", Tag::Short(ticks as i16)),
            _ => false,
        }
    }
    /// Clears positive fire ticks while retaining fire immunity (`Entity.clearFire`).
    pub(super) fn entity_clear_fire(&mut self, entity: EntityRef) -> bool {
        let Some(ticks) = self.entity_fire_ticks(entity) else {
            return false;
        };
        self.entity_set_remaining_fire_ticks(entity, ticks.min(0))
    }
    /// The entity's air (`Entity.getAirSupply`): players and mobs.
    pub(super) fn entity_air_supply(&self, entity: EntityRef) -> Option<i32> {
        match self.native_entity_body(entity)? {
            Body::Player(p) => Some(p.air),
            Body::Ecs(id) => self.ecs_number(id, "Air"),
            Body::Other(_) => None,
        }
    }
    /// Sets the entity's air (`Entity.setAirSupply`).
    pub(super) fn entity_set_air_supply(&mut self, entity: EntityRef, air: i32) -> bool {
        match self.native_entity_target(entity) {
            Some(Target::Player) => match self.player.as_mut() {
                Some(player) => {
                    player.air = air;
                    true
                }
                None => false,
            },
            Some(Target::Guest(conn)) => match self
                .lan
                .as_mut()
                .and_then(|lan| lan.hub.guests.motion_mut(conn))
            {
                Some(player) => {
                    player.air = air;
                    true
                }
                None => false,
            },
            Some(Target::Mob(id)) if self.ecs_has(id) => self.ecs_put(id, "Air", Tag::Short(air as i16)),
            _ => false,
        }
    }
    /// The most air the entity holds (`Entity.getMaxAirSupply`): 300, a
    /// dolphin's 4800, an axolotl's 6000.
    pub(super) fn entity_max_air_supply(&self, entity: EntityRef) -> Option<i32> {
        // An ECS mob's species has no bridge read yet: the default air (listed in the report).
        self.native_entity_body(entity)?;
        Some(max_air(None))
    }
    /// Whether the mob makes no sounds (`Entity.isSilent`, its `Silent`
    /// tag); false for a player.
    pub(super) fn entity_is_silent(&self, entity: EntityRef) -> Option<bool> {
        match self.native_entity_body(entity)? {
            Body::Player(_) => Some(false),
            Body::Ecs(id) => self.ecs_flag(id, "Silent"),
            Body::Other(_) => None,
        }
    }
    /// Silences the mob or lets it make sounds again (`Entity.setSilent`).
    /// False for the other kinds.
    pub(super) fn entity_set_silent(&mut self, entity: EntityRef, silent: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(entity) else {
            return false;
        };
        if self.ecs_has(id) {
            return self.ecs_put_flag(id, "Silent", silent);
        }
        false
    }
    /// Whether the entity's model is hidden (`Entity.isInvisible`): the
    /// invisibility effect on a player or a mob, a mob's or an armor stand's
    /// flag.
    pub(super) fn entity_is_invisible(&self, entity: EntityRef) -> Option<bool> {
        match self.native_entity_target(entity)? {
            Target::Player => Some(self.potions.effects.get("invisibility").is_some()),
            Target::Guest(conn) => Some(
                self.command_guest(conn)?
                    .effects
                    .get("invisibility")
                    .is_some(),
            ),
            Target::Mob(id) if self.ecs_has(id) => self.ecs_flag(id, "Invisible"),
            Target::Stand(id) => Some(self.armor_stand_saved(id)?.invisible),
            _ => None,
        }
    }
    /// Sets the entity's frozen ticks (`Entity.setTicksFrozen`).
    pub(super) fn entity_set_ticks_frozen(&mut self, entity: EntityRef, ticks: i32) -> bool {
        match self.native_entity_target(entity) {
            Some(Target::Player) => self.player.as_mut().is_some_and(|p| {
                p.ticks_frozen = ticks;
                true
            }),
            Some(Target::Guest(conn)) => self
                .lan
                .as_mut()
                .and_then(|lan| lan.hub.guests.motion_mut(conn))
                .is_some_and(|p| {
                    p.ticks_frozen = ticks;
                    true
                }),
            Some(Target::Mob(id)) if self.ecs_has(id) => self.ecs_put(id, "TicksFrozen", Tag::Int(ticks)),
            _ => false,
        }
    }
    /// Returns the mob's persistent glowing tag (`Entity.hasGlowingTag`).
    pub(super) fn entity_glowing(&self, entity: EntityRef) -> Option<bool> {
        self.native_entity_body(entity)?;
        None
    }
    /// Returns the mob's no-gravity flag (`Entity.isNoGravity`).
    pub(super) fn entity_no_gravity(&self, entity: EntityRef) -> Option<bool> {
        match self.native_entity_body(entity)? {
            Body::Ecs(id) => self.ecs_flag(id, "NoGravity"),
            _ => None,
        }
    }
    /// Returns the entity's frozen ticks (`Entity.getTicksFrozen`).
    pub(super) fn entity_ticks_frozen(&self, entity: EntityRef) -> Option<i32> {
        match self.native_entity_body(entity)? {
            Body::Player(p) => Some(p.ticks_frozen),
            Body::Ecs(id) => self.ecs_number(id, "TicksFrozen"),
            _ => None,
        }
    }
    /// Ticks the entity has lived (`Entity.tickCount`): mobs, item entities,
    /// experience orbs and minecarts. Wraps as vanilla's `int` does.
    pub(super) fn entity_tick_count(&self, entity: EntityRef) -> Option<i32> {
        Some(match self.native_entity_target(entity)? {
            Target::Mob(id) if self.ecs_has(id) => self.ecs.age(id)?,
            Target::Item(id) => self.ecs.item(id)?.ticks as i32,
            Target::Orb(id) => self.ecs.orb(id)?.age as i32,
            Target::Cart(id) => self.ecs.age(id)?,
            _ => return None,
        })
    }
    /// Sets the mob's persistent glowing tag (`Entity.setGlowingTag`). Returns false for players and guests.
    pub(super) fn entity_set_glowing(&mut self, entity: EntityRef, glowing: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(entity) else {
            return false;
        };
        if self.ecs_has(id) {
            return self.ecs.set_glowing(id, glowing);
        }
        false
    }
    /// Sets the mob's no-gravity flag (`Entity.setNoGravity`). Returns false for players and guests.
    pub(super) fn entity_set_no_gravity(&mut self, entity: EntityRef, no_gravity: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(entity) else {
            return false;
        };
        if self.ecs_has(id) {
            return self.ecs_put_flag(id, "NoGravity", no_gravity);
        }
        false
    }
    /// Sets the entity's shared invisibility flag (`Entity.setInvisible`). Returns false for players and guests.
    pub(super) fn entity_set_invisible(&mut self, entity: EntityRef, invisible: bool) -> bool {
        match self.native_entity_target(entity) {
            Some(Target::Mob(id)) if self.ecs_has(id) => self.ecs_put_flag(id, "Invisible", invisible),
            Some(Target::Stand(id)) => self.armor_stand_edit(id, |s| s.invisible = invisible),
            _ => false,
        }
    }
    /// The entity's pose (`Entity.getPose`) as `Pose` names it: `standing`,
    /// `crouching`, `swimming`, `sleeping`, a warden's `roaring`... Kinds
    /// with no pose stand.
    pub(super) fn entity_pose(&self, entity: EntityRef) -> Option<&'static str> {
        Some(match self.native_entity_body(entity)? {
            Body::Player(p) => player_pose(p.pose),
            // `mod_api::Pose` is declared in `Pose.id` order, so its discriminant indexes POSES.
            Body::Ecs(id) => self.ecs.pose(id).map_or(POSES[0], |p| POSES[p as usize]),
            Body::Other(_) => POSES[0],
        })
    }
}

/// `(fluid, height)` for each fluid `(height, touched)` the entity touched, highest first;
/// water before lava at the same height (`getMaxHeightFluidType`'s order).
fn touched_fluids(water: (f64, bool), lava: (f64, bool)) -> Vec<(String, f64)> {
    let mut heights: Vec<(String, f64)> = [("minecraft:water", water), ("minecraft:lava", lava)]
        .into_iter()
        .filter(|(_, (_, touched))| *touched)
        .map(|(fluid, (height, _))| (fluid.to_owned(), height))
        .collect();
    // A stable sort keeps water first in a tie.
    heights.sort_by(|a, b| b.1.total_cmp(&a.1));
    heights
}

/// The first fluid at the eyes: water, then lava, as the trackers are kept.
fn eye_fluid(water: bool, lava: bool) -> Option<String> {
    let fluid = if water {
        Some("minecraft:water")
    } else {
        lava.then_some("minecraft:lava")
    };
    fluid.map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reads' order: highest first, water before lava in a tie, an untouched fluid left
    /// out even with a height, and nothing for an entity in neither.
    #[test]
    fn fluid_reads_list_the_touched_fluids_highest_first() {
        let (water, lava) = ("minecraft:water".to_owned(), "minecraft:lava".to_owned());
        assert_eq!(
            touched_fluids((0.5, true), (1.25, true)),
            [(lava.clone(), 1.25), (water.clone(), 0.5)]
        );
        assert_eq!(
            touched_fluids((0.75, true), (0.75, true)),
            [(water.clone(), 0.75), (lava.clone(), 0.75)]
        );
        assert_eq!(
            touched_fluids((0.4, true), (0.2, false)),
            [(water.clone(), 0.4)]
        );
        assert!(touched_fluids((0.0, false), (0.0, false)).is_empty());
        assert_eq!(eye_fluid(true, true), Some(water));
        assert_eq!(eye_fluid(false, true), Some(lava));
        assert_eq!(eye_fluid(false, false), None);
    }

    #[test]
    fn poses_keep_vanillas_ids() {
        assert_eq!(POSES[usize::from(sync::SITTING)], "sitting");
        assert_eq!(POSES[usize::from(sync::INHALING)], "inhaling");
        for pose in [
            Pose::Standing,
            Pose::Crouching,
            Pose::Swimming,
            Pose::FallFlying,
            Pose::SpinAttack,
            Pose::Sleeping,
        ] {
            let id = POSES.iter().position(|p| *p == player_pose(pose));
            assert_eq!(id.map(|i| Pose::from_id(i as i32)), Some(pose));
        }
    }

    #[test]
    #[ignore = "daily"]
    fn mob_setters_update_stored_state() {
        use super::super::native_entity_refs::tests::with_game;
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            game.ecs = game.test_ecs_level();
            crate::game::ecs::test_support::place_test_mob(game, 42, "minecraft:zombie", [0.0; 3]).unwrap();
            let entity = game.entity(42).unwrap();
            assert!(entity.set_air_supply(game, -7));
            assert_eq!(game.ecs_number(42, "Air"), Some(-7));
            assert!(entity.set_remaining_fire_ticks(game, 37));
            assert_eq!(game.ecs_number(42, "Fire"), Some(37));
            assert!(entity.clear_fire(game));
            assert_eq!(game.ecs_number(42, "Fire"), Some(0));
            assert!(game.entity_set_remaining_fire_ticks(entity, -12));
            assert!(game.entity_clear_fire(entity));
            assert_eq!(game.ecs_number(42, "Fire"), Some(-12));
            assert!(entity.set_ticks_frozen(game, 31));
            assert_eq!(game.ecs_number(42, "TicksFrozen"), Some(31));
            assert!(entity.set_invisible(game, true));
            assert_eq!(game.ecs_flag(42, "Invisible"), Some(true));
            assert!(entity.set_invisible(game, false));
            assert_eq!(entity.is_invisible(game), Some(false));
            assert!(entity.set_glowing(game, true));
            assert_eq!(game.ecs_flag(42, "Glowing"), Some(true));
            assert!(entity.set_no_gravity(game, true));
            assert_eq!(game.ecs_flag(42, "NoGravity"), Some(true));
        });
    }

    fn with_player(test: impl FnOnce(&mut Game<'_>, EntityRef)) {
        use super::super::native_entity_refs::tests::with_game;
        use horizon_core::geometry::Vec3;
        with_game(|game| {
            game.options.dedicated = Some(Vec::new());
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            crate::game::native_entity_refs::tests::bind_local_player(game, 20, 123);
            let player = game.entity(20).unwrap();
            test(game, player);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn fire_ticks_reads_supported_entities_and_refuses_others() {
        use horizon_client::entities::ArmorStand;
        use horizon_client::items::ItemStack;
        use horizon_core::geometry::Vec3;
        use mod_api::WorldEpoch;

        with_player(|game, player| {
            game.player.as_mut().unwrap().fire_ticks = 0;
            assert_eq!(player.fire_ticks(game), Some(0));
            game.player.as_mut().unwrap().fire_ticks = -12;
            assert_eq!(player.fire_ticks(game), Some(-12));

            game.ecs = game.test_ecs_level();
            crate::game::ecs::test_support::place_test_mob(game, 42, "minecraft:zombie", [0.0; 3]).unwrap();
            let mob = game.entity(42).unwrap();
            assert!(mob.set_remaining_fire_ticks(game, 37));
            assert_eq!(mob.fire_ticks(game), Some(37));

            crate::game::ecs::test_support::place_test_stand(game, 43, &ArmorStand::new(43, Vec3::ZERO, 0.0));
            game.armor_stand_edit(game.armor_stand_ids()[0], |a| a.fire_ticks = 23);
            let stand = game.entity(43).unwrap();
            assert_eq!(stand.fire_ticks(game), Some(23));

            game.spawn_item(
                ItemStack {
                    item: "minecraft:stone".into(),
                    count: 1,
                    damage: 0,
                    extra: None,
                },
                [0.0; 3],
                [0.0; 3],
                0,
            );
            let id = game.ecs.item_ids()[0];
            game.ecs.set_on_fire(id, 19);
            let item = game.entity(id).unwrap();
            assert_eq!(item.fire_ticks(game), Some(19));

            // 43 is the stand and 44 the item: an allocator that keeps answering a taken id never ends.
            game.ecs_with(|ecs, mw| ecs.spawn_boat_id(mw, "oak", false, [0.0; 3], 0.0, &mut || 50));
            let boat = game.entity(50).unwrap();
            assert_eq!(boat.fire_ticks(game), None);
            let stale = EntityRef::new(WorldEpoch::new(mob.epoch().get() + 1), mob.id());
            assert_eq!(stale.fire_ticks(game), None);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn tick_count_reads_supported_entities_and_refuses_others() {
        use horizon_client::items::ItemStack;
        use mod_api::WorldEpoch;

        with_player(|game, player| {
            assert_eq!(player.tick_count(game), None);

            game.ecs = game.test_ecs_level();
            crate::game::ecs::test_support::place_test_mob(game, 42, "minecraft:zombie", [0.0; 3]).unwrap();
            let mob = game.entity(42).unwrap();
            assert_eq!(mob.tick_count(game), Some(0));

            game.spawn_item(
                ItemStack {
                    item: "minecraft:stone".into(),
                    count: 1,
                    damage: 0,
                    extra: None,
                },
                [0.0; 3],
                [0.0; 3],
                0,
            );
            let id = game.ecs.item_ids()[0];
            game.ecs.set_item_ticks(id, 77);
            game.ecs.set_item_age(id, 5000);
            let item = game.entity(id).unwrap();
            assert_eq!(item.tick_count(game), Some(77));

            let orb = game.ecs_orb_moving(7, [0.0; 3], [0.0; 3]).expect("an orb");
            assert_eq!(game.entity(orb).unwrap().tick_count(game), Some(0));


            game.ecs_with(|ecs, mw| ecs.spawn_boat_id(mw, "oak", false, [0.0; 3], 0.0, &mut || 45));
            let boat = game.entity(45).unwrap();
            assert_eq!(boat.tick_count(game), None);
            let stale = EntityRef::new(WorldEpoch::new(mob.epoch().get() + 1), mob.id());
            assert_eq!(stale.tick_count(game), None);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn portal_cooldown_reads_simulated_kinds_and_refuses_others() {
        use super::super::native_guest_turn_tests::with_guests;
        use super::super::{
            Dim,
        };
        use horizon_client::entities::ArmorStand;
        use horizon_client::items::ItemStack;
        use horizon_core::{geometry::Vec3, position::BlockPos};
        use mod_api::WorldEpoch;

        with_guests(|game| {
            // Players keep their own state: in place during their turn, beside the hub otherwise.
            let host = game.lan.as_mut().unwrap();
            host.play.entry(12).or_default().portal.cooldown = 7;
            host.play.entry(13).or_default().portal.cooldown = 4;
            game.portals.player.cooldown = 99;
            let (acting, other) = (game.entity(12).unwrap(), game.entity(13).unwrap());
            assert_eq!(acting.portal_cooldown(game), Some(7));
            game.as_guest(12, |game| {
                assert_eq!(acting.portal_cooldown(game), Some(7));
                assert_eq!(other.portal_cooldown(game), Some(4));
            })
            .unwrap();
            assert_eq!(acting.portal_cooldown(game), Some(7));
            assert_eq!(other.portal_cooldown(game), Some(4));

            // A mob counts down from `setPortalCooldown`'s 300 as the level ticks.
            // Numbered 60: the explicit spawns below take 42 to 50, and the allocator goes on from the mob.
            game.ecs = game.test_ecs_level();
            crate::game::ecs::test_support::place_test_mob(game, 60, "minecraft:cow", [0.0; 3]).unwrap();
            let mob = game.entity(60).unwrap();
            assert_eq!(mob.portal_cooldown(game), Some(0));
            game.set_portal_cooldown(40);
            assert_eq!(mob.portal_cooldown(game), Some(300));
            game.handle_entity_portals();
            assert_eq!(mob.portal_cooldown(game), Some(299));

            // The other kinds the level carries through portals.
            game.spawn_item(
                ItemStack {
                    item: "minecraft:stone".into(),
                    count: 1,
                    damage: 0,
                    extra: None,
                },
                [0.0; 3],
                [0.0; 3],
                0,
            );
            let item = game.ecs.item_ids()[0];
            let orb = game.ecs_orb_moving(1, [0.0; 3], [0.0; 3]).expect("an orb");
            game.ecs_with(|ecs, mw| ecs.spawn_boat_id(mw, "oak", false, [0.0; 3], 0.0, &mut || 42));
            game.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, "minecraft:minecart", [0.0; 3], 0.0, &mut || 43));
            game.add_primed_tnt(BlockPos::new(0, 64, 0), 0.0, 80, None, None);
            let tnt = game.ecs.species_ids("minecraft:tnt")[0];
            game.ecs_with(|ecs, mw| ecs.launch(mw, "minecraft:arrow", [0.0; 3], [0.0; 3], None, &mut || 48));
            game.ecs_with(|ecs, mw| ecs.launch(mw, "minecraft:snowball", [0.0; 3], [0.0; 3], None, &mut || 49));
            let (arrow, snowball) = (48, 49);
            game.ecs_with(|ecs, mw| {
                ecs.launch_rocket(mw, [0.0; 3], [0.0; 3], false, Vec::new(), 0, None, None, &mut || 44)
            });
            game.ecs_with(|ecs, mw| {
                ecs.throw_trident(
                    mw,
                    [0.0; 3],
                    [0.0; 3],
                    None,
                    0,
                    false,
                    horizon_ecs_proto::trident::Pickup::Allowed,
                    horizon_client::items::ItemStack::new("minecraft:trident", 1),
                    &mut || 45,
                )
            });
            // Entity defaults to 300; VehicleEntity overrides it to 10 and
            // Projectile to 2 (`getDimensionChangingDelay` in vanilla 26.3).
            for (id, delay) in [
                (item, 300),
                (orb, 300),
                (42, 10),
                (43, 10),
                (tnt, 300),
                (arrow, 2),
                (snowball, 2),
                (44, 2),
                (45, 2),
            ] {
                let entity = game.entity(id).unwrap();
                assert_eq!(entity.portal_cooldown(game), Some(0), "entity {id}");
                game.set_portal_cooldown(id);
                assert_eq!(entity.portal_cooldown(game), Some(delay), "entity {id}");
            }

            // A falling block is an ECS kind: its cooldown is the level's table, read and written as a mob's is.
            game.summon_with_nbt(
                "minecraft:falling_block",
                [1.0, 70.0, 1.0],
                "{BlockState:{Name:\"minecraft:sand\"},Time:1}",
            )
            .unwrap();
            let falling_id = game.ecs.species_ids("minecraft:falling_block")[0];
            let falling = game.entity(falling_id).unwrap();
            assert_eq!(falling.portal_cooldown(game), Some(0));
            assert!(falling.set_portal_cooldown(game, 25));
            assert_eq!(falling.portal_cooldown(game), Some(25));

            // No portal cooldown: an armor stand, a wind charge, a stale handle, a parked level.
            crate::game::ecs::test_support::place_test_stand(game, 46, &ArmorStand::new(46, Vec3::ZERO, 0.0));
            let stand = game.entity(46).unwrap();
            assert_eq!(stand.portal_cooldown(game), None);
            game.ecs_with(|ecs, mw| ecs.launch(mw, "minecraft:breeze_wind_charge", [0.0; 3], [0.0; 3], None, &mut || 50));
            let charge = game.entity(50).unwrap();
            assert_eq!(charge.portal_cooldown(game), None);
            let stale = EntityRef::new(WorldEpoch::new(mob.epoch().get() + 1), mob.id());
            assert_eq!(stale.portal_cooldown(game), None);
            game.enter_level(Dim::Nether).unwrap();
            assert_eq!(mob.portal_cooldown(game), None);
            game.enter_level(Dim::Overworld).unwrap();
            assert_eq!(mob.portal_cooldown(game), Some(299));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn portal_cooldown_write_blocks_entry_and_counts_down() {
        use super::super::native_entity_refs::tests::with_game;
        use horizon_client::chunk::Chunk;
        use horizon_client::motion::PlayerMotion;
        use horizon_core::geometry::Vec3;
        use std::sync::Arc;

        with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            game.settings.simulation_distance = 4;
            game.settings.render_distance = 12;
            let registry = game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let portal = registry.default_state("minecraft:nether_portal").unwrap();
            game.player = Some(PlayerMotion::new(game.motion, Vec3::new(8.5, 65.0, 8.5)).unwrap());
            for z in [8, 12] {
                game.world.set(0, 63, z, stone).unwrap();
            }
            game.world.set(0, 64, 8, portal).unwrap();
            game.world.set(0, 65, 8, portal).unwrap();
            game.ecs = game.test_ecs_level();
            crate::game::ecs::test_support::place_test_mob(game, 40, "minecraft:zombie", [0.5, 64.0, 8.5]).unwrap();
            game.load_ticking_neighbourhood(0, 0);
            game.update_chunk_tickets();
            assert!(
                game.entity_ticking_area()(0, 8),
                "the mob's chunk ticks entities"
            );
            let mob = game.entity(40).unwrap();

            // A written positive value blocks the portal: the mob stays, and
            // standing in the portal refills the countdown.
            assert!(mob.set_portal_cooldown(game, 40));
            assert_eq!(mob.portal_cooldown(game), Some(40));
            game.find_entity_portals();
            game.handle_entity_portals();
            assert!(game.ecs_has(40), "the mob stays in its portal");
            let held = mob.portal_cooldown(game).unwrap();
            assert_eq!(
                held,
                horizon_client::nether_portal::ENTITY_DELAY - 1,
                "standing in the portal refills the cooldown before the tick"
            );

            // Out of the portal, the value counts down one per tick.
            game.ecs_with(|ecs, mw| {
                let e = ecs.entity_of(40).unwrap();
                ecs.teleport(mw, e, [0.5, 64.0, 12.5]);
            });
            game.handle_entity_portals();
            assert_eq!(mob.portal_cooldown(game), Some(held - 1));

            // A negative value is refused and leaves the value alone.
            assert!(!mob.set_portal_cooldown(game, -1));
            assert_eq!(mob.portal_cooldown(game), Some(held - 1));

            // 0 lets it enter on the next tick.
            game.ecs_with(|ecs, mw| {
                let e = ecs.entity_of(40).unwrap();
                ecs.teleport(mw, e, [0.5, 64.0, 8.5]);
            });
            assert!(mob.set_portal_cooldown(game, 0));
            assert_eq!(mob.portal_cooldown(game), Some(0));
            game.find_entity_portals();
            game.handle_entity_portals();
            assert!(!game.ecs_has(40), "0 lets the mob through");
        });
    }

    #[test]
    #[ignore = "daily"]
    fn portal_cooldown_write_is_refused_for_a_player() {
        with_player(|game, player| {
            game.portals.player.cooldown = 7;
            assert!(!player.set_portal_cooldown(game, 40));
            assert_eq!(game.portals.player.cooldown, 7);
            assert_eq!(player.portal_cooldown(game), Some(7));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn portal_cooldown_write_is_refused_for_an_acting_guest() {
        use super::super::native_guest_turn_tests::with_guests;

        with_guests(|game| {
            let host = game.lan.as_mut().unwrap();
            host.play.entry(12).or_default().portal.cooldown = 7;
            let guest = game.entity(12).unwrap();
            assert!(!guest.set_portal_cooldown(game, 40));
            assert_eq!(
                game.lan
                    .as_ref()
                    .unwrap()
                    .play
                    .get(&12)
                    .unwrap()
                    .portal
                    .cooldown,
                7
            );
            assert_eq!(guest.portal_cooldown(game), Some(7));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn portal_cooldown_write_reaches_an_item() {
        use super::super::native_entity_refs::tests::with_game;
        use horizon_client::items::ItemStack;

        with_game(|game| {
            game.spawn_item(
                ItemStack {
                    item: "minecraft:stone".into(),
                    count: 1,
                    damage: 0,
                    extra: None,
                },
                [0.0; 3],
                [0.0; 3],
                0,
            );
            let item = game.entity(game.ecs.item_ids()[0]).unwrap();
            assert!(item.set_portal_cooldown(game, 40));
            assert_eq!(item.portal_cooldown(game), Some(40));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn portal_cooldown_write_reaches_an_arrow() {
        use super::super::native_entity_refs::tests::with_game;
        with_game(|game| {
            game.ecs_with(|ecs, mw| ecs.launch(mw, "minecraft:arrow", [0.0; 3], [0.0; 3], None, &mut || 51));
            let arrow = game.entity(51).unwrap();
            assert!(arrow.set_portal_cooldown(game, 40));
            assert_eq!(arrow.portal_cooldown(game), Some(40));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn portal_cooldown_write_is_refused_for_a_non_portal_projectile() {
        use super::super::native_entity_refs::tests::with_game;
        with_game(|game| {
            game.ecs_with(|ecs, mw| ecs.launch(mw, "minecraft:breeze_wind_charge", [0.0; 3], [0.0; 3], None, &mut || 52));
            let charge = game.entity(52).unwrap();
            assert!(!charge.set_portal_cooldown(game, 40));
            assert_eq!(charge.portal_cooldown(game), None);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn player_invisibility_write_is_refused() {
        with_player(|game, player| assert!(!player.set_invisible(game, true)));
    }

    #[test]
    #[ignore = "daily"]
    fn player_glowing_write_is_refused() {
        with_player(|game, player| assert!(!player.set_glowing(game, true)));
    }

    #[test]
    #[ignore = "daily"]
    fn player_no_gravity_write_is_refused() {
        with_player(|game, player| assert!(!player.set_no_gravity(game, true)));
    }

    #[test]
    #[ignore = "daily"]
    fn player_frozen_write_updates_server_state() {
        with_player(|game, player| {
            assert!(player.set_ticks_frozen(game, 53));
            assert_eq!(game.player.as_ref().unwrap().ticks_frozen, 53);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn secondary_use_keeps_server_flags() {
        with_player(|game, player| {
            game.player.as_mut().unwrap().shift_key_down = true;
            assert_eq!(
                player.is_shift_key_down(game),
                Some(true),
                "server keeps its semantic entity flag"
            );
        });
    }

    #[test]
    fn max_air_follows_the_species() {
        assert_eq!(max_air(None), 300);
        assert_eq!(max_air(Some(Species::Dolphin)), 4800);
        assert_eq!(max_air(Some(Species::Axolotl)), 6000);
        assert_eq!(max_air(Some(Species::Zombie)), 300);
    }
}
