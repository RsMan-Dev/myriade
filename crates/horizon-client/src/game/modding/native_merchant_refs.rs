//! Merchant handles for native server mods: villagers and wandering traders.
use super::{Game, commands::Target};
use horizon_client::{merchant::MerchantOffer, mobs::VILLAGER_TYPES};
use horizon_core::identifier::Identifier;
use mod_api::{EntityRef, MerchantRef, PlayerRef};

/// The index of a `minecraft:` registry id in a table of bare names.
fn registry_index(id: &str, names: &[&str]) -> Option<u8> {
    let id = Identifier::parse(id).ok()?;
    if id.namespace() != "minecraft" {
        return None;
    }
    names.iter().position(|n| *n == id.path()).map(|i| i as u8)
}

impl Game<'_> {
    /// An item stack a trade can hold: a nonempty stack of an item the game
    /// has (a client would otherwise be sent air for it).
    fn merchant_real(&self, stack: &horizon_client::items::ItemStack) -> bool {
        stack.count > 0
            && &*stack.item != "minecraft:air"
            && self.items().get(&stack.item).is_some()
    }

    fn merchant_valid_offer(&self, offer: &MerchantOffer) -> bool {
        self.merchant_real(&offer.base_cost_a.stack)
            && offer
                .cost_b
                .as_ref()
                .is_none_or(|b| self.merchant_real(&b.stack))
            && self.merchant_real(&offer.result)
    }
    /// The id of the villager or wandering trader a handle names: the ECS entity must carry a `Merchant`.
    fn native_merchant_id(&self, merchant: MerchantRef) -> Option<u32> {
        let Target::Mob(id) = self.native_entity_target(merchant.entity())? else {
            return None;
        };
        self.ecs.is_villager(id).map(|_| id)
    }

    fn native_villager_id(&self, merchant: MerchantRef) -> Option<u32> {
        let id = self.native_merchant_id(merchant)?;
        (self.ecs.is_villager(id) == Some(true)).then_some(id)
    }

    /// Reads the offers, experience and level (1 for a trader) of the merchant
    /// `id`: from a guest's open trading screen if there is one, whose copy
    /// is newer until it closes. A trader whose offers are not rolled yet has none.
    fn merchant_read<T>(
        &self,
        id: u32,
        read: impl FnOnce(&[MerchantOffer], i32, i32) -> T,
    ) -> T {
        if self.ecs.trader_opaque(id) {
            let level = self.ecs.trade_level(id).unwrap_or(1).max(1);
            return read(&[], self.ecs.trade_xp(id).unwrap_or(0), level);
        }
        if let Some(session) = self
            .lan_trade_conn(id)
            .and_then(|conn| self.lan_trade_session(conn))
        {
            return read(&session.offers, session.xp, session.level);
        }
        let level = self.ecs.trade_level(id).unwrap_or(1).max(1);
        let offers = self.ecs.trade_offers(id).unwrap_or_default();
        read(&offers, self.ecs.trade_xp(id).unwrap_or(0), level)
    }


    /// Changes the offers, experience and level of the merchant `id`: on a
    /// guest's open trading screen if there is one (and resent to them), else on
    /// the merchant. False when a trade is open somewhere this cannot reach (the
    /// host window, or inside a hook of that trade's own click), or `edit` refused.
    fn merchant_write(
        &mut self,
        id: u32,
        edit: impl FnOnce(&mut Vec<MerchantOffer>, &mut i32, &mut i32) -> bool,
    ) -> bool {
        if self.ecs.trader_opaque(id) {
            return false;
        }
        if let Some(conn) = self.lan_trade_conn(id) {
            let mut done = false;
            let sent = self.lan_trade_edit(conn, |session| {
                done = edit(&mut session.offers, &mut session.xp, &mut session.level);
                if done {
                    session
                        .events
                        .push(horizon_client::merchant::Event::OffersRefreshed);
                }
            });
            return sent && done;
        }
        if self.ecs.trade_busy(id) {
            return false;
        }
        let mut offers = self.ecs.trade_offers(id).unwrap_or_default();
        let mut xp = self.ecs.trade_xp(id).unwrap_or(0);
        let mut level = self.ecs.trade_level(id).unwrap_or(1).max(1);
        let (offers0, xp0, level0) = (offers.clone(), xp, level);
        if !edit(&mut offers, &mut xp, &mut level) {
            return false;
        }
        if offers != offers0 {
            self.ecs.trade_store(id, offers, xp);
        } else if xp != xp0 {
            self.ecs.set_trade_xp(id, xp);
        }
        if level != level0 && self.ecs.is_villager(id) == Some(true) {
            self.ecs.set_trade_level(id, level.max(1));
        }
        true
    }

    /// Narrows an entity to a villager or wandering trader, or None for
    /// another kind, a stale handle, or a client.
    pub(super) fn merchant(&self, entity: EntityRef) -> Option<MerchantRef> {
        let merchant = MerchantRef::new(entity);
        self.native_merchant_id(merchant).map(|_| merchant)
    }

    /// `AbstractVillager.getOffers`, as they are now (a copy). Offers not
    /// rolled yet read as empty: they are rolled when a player first trades,
    /// or by `add_offer`; reading never draws a random.
    pub(super) fn merchant_offers(&self, merchant: MerchantRef) -> Option<Vec<MerchantOffer>> {
        let id = self.native_merchant_id(merchant)?;
        Some(self.merchant_read(id, |offers, _, _| offers.to_vec()))
    }

    /// Replaces offer `index` (a player trading with it is sent the new
    /// offers). False for an index past the end, a stale handle, or an offer
    /// without a real item in `base_cost_a` or `result` (or `cost_b` when set).
    pub(super) fn merchant_set_offer(
        &mut self,
        merchant: MerchantRef,
        index: u32,
        offer: MerchantOffer,
    ) -> bool {
        let Some(id) = self.native_merchant_id(merchant) else {
            return false;
        };
        if !self.merchant_valid_offer(&offer) {
            return false;
        }
        self.merchant_write(id, |offers, _, _| {
            let Some(slot) = offers.get_mut(index as usize) else {
                return false;
            };
            *slot = offer;
            true
        })
    }

    /// Appends an offer, as `getOffers().add(offer)` does. Offers not rolled
    /// yet are rolled first, as `getOffers` does for a player's first trade
    /// (this draws the trade set's world random sequence, as that does).
    /// False for a stale handle or an invalid offer (see `set_offer`).
    pub(super) fn merchant_add_offer(
        &mut self,
        merchant: MerchantRef,
        offer: MerchantOffer,
    ) -> bool {
        let Some(id) = self.native_merchant_id(merchant) else {
            return false;
        };
        if !self.merchant_valid_offer(&offer) {
            return false;
        }
        if self.lan_trade_conn(id).is_none() && !self.ecs.trade_busy(id) {
            self.ecs.ensure_offers(id);
        }
        self.merchant_write(id, |offers, _, _| {
            offers.push(offer);
            true
        })
    }

    /// Removes offer `index`, the ones after it moving up. False for an index
    /// past the end or a stale handle.
    pub(super) fn merchant_remove_offer(&mut self, merchant: MerchantRef, index: u32) -> bool {
        let Some(id) = self.native_merchant_id(merchant) else {
            return false;
        };
        self.merchant_write(id, |offers, _, _| {
            if index as usize >= offers.len() {
                return false;
            }
            offers.remove(index as usize);
            true
        })
    }

    /// `Merchant.getTradingPlayer`: the player whose trading screen is open on
    /// it. None when nobody trades, or the screen is the host window's.
    pub(super) fn merchant_trading_player(&self, merchant: MerchantRef) -> Option<PlayerRef> {
        let id = self.native_merchant_id(merchant)?;
        let conn = self.lan_trade_conn(id)?;
        self.player(self.lan_trade_player(conn)?)
    }

    /// `VillagerData.profession`, a registry id (`minecraft:farmer`; none is
    /// `minecraft:none`). None for a wandering trader.
    pub(super) fn merchant_profession(&self, merchant: MerchantRef) -> Option<String> {
        self.ecs.trade_profession(self.native_villager_id(merchant)?)
    }

    /// `Villager.setVillagerData` with a new profession: a different one
    /// forgets the offers, which roll again on the next trade. Clients see the
    /// new profession through entity data. False for an unknown id, a wandering
    /// trader, or a villager with a trading screen open.
    pub(super) fn merchant_set_profession(
        &mut self,
        merchant: MerchantRef,
        profession: &str,
    ) -> bool {
        let Some(id) = self.native_villager_id(merchant) else {
            return false;
        };
        let Some(index) = Identifier::parse(profession)
            .ok()
            .and_then(|p| self.stations().professions.resolve(&p.to_string()))
            .and_then(|i| u8::try_from(i).ok())
        else {
            return false;
        };
        if self.ecs.trader_opaque(id) || self.ecs.trade_busy(id) || self.lan_trade_conn(id).is_some() {
            return false;
        }
        self.ecs.set_trade_profession(id, index);
        true
    }

    /// `VillagerData.level`, at least 1.
    pub(super) fn merchant_level(&self, merchant: MerchantRef) -> Option<i32> {
        let id = self.native_villager_id(merchant)?;
        Some(self.merchant_read(id, |_, _, level| level.max(1)))
    }

    /// `VillagerData.withLevel`: `Math.max(1, level)`, with no upper limit in
    /// vanilla. The offers stay as they are; the next level-up adds its trades
    /// (a level past 5 can't level up). A trading guest's session follows,
    /// and gets the sequences a level-up from the new level needs.
    pub(super) fn merchant_set_level(&mut self, merchant: MerchantRef, level: i32) -> bool {
        let Some(id) = self.native_villager_id(merchant) else {
            return false;
        };
        let level = level.max(1);
        self.merchant_write(id, |_, _, current| {
            *current = level;
            true
        })
    }

    /// `VillagerData.type`, a registry id (`minecraft:plains`).
    pub(super) fn merchant_villager_type(&self, merchant: MerchantRef) -> Option<String> {
        self.ecs.trade_variant(self.native_villager_id(merchant)?)
    }

    /// `VillagerData.withType`: also the type a trading guest's session
    /// rolls its next level-up offers with. False for an unknown id, a
    /// wandering trader, or a trade open where this can't reach.
    pub(super) fn merchant_set_villager_type(&mut self, merchant: MerchantRef, kind: &str) -> bool {
        let Some(id) = self.native_villager_id(merchant) else {
            return false;
        };
        let Some(kind) = registry_index(kind, &VILLAGER_TYPES) else {
            return false;
        };
        if let Some(conn) = self.lan_trade_conn(id) {
            let variant = format!("minecraft:{}", VILLAGER_TYPES[kind as usize]);
            if !self.lan_trade_edit(conn, |session| session.variant = variant) {
                return false;
            }
        } else if self.ecs.trade_busy(id) {
            return false;
        }
        self.ecs.set_trade_variant(id, kind);
        true
    }

    /// `Villager.getVillagerXp`.
    pub(super) fn merchant_xp(&self, merchant: MerchantRef) -> Option<i32> {
        let id = self.native_villager_id(merchant)?;
        Some(self.merchant_read(id, |_, xp, _| xp))
    }

    /// `Villager.setVillagerXp`: assigned as given, as vanilla does.
    pub(super) fn merchant_set_xp(&mut self, merchant: MerchantRef, xp: i32) -> bool {
        let Some(id) = self.native_villager_id(merchant) else {
            return false;
        };
        self.merchant_write(id, |_, current, _| {
            *current = xp;
            true
        })
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use super::*;
    use horizon_client::{items::ItemStack, merchant::ItemCost};

    pub(in crate::game) fn offer(cost: &str, result: &str) -> MerchantOffer {
        MerchantOffer {
            base_cost_a: ItemCost {
                stack: ItemStack::new(cost, 20),
            },
            cost_b: None,
            result: ItemStack::new(result, 1),
            uses: 1,
            max_uses: 12,
            reward_exp: true,
            special_price_diff: -2,
            demand: 3,
            price_multiplier: 0.05,
            xp: 7,
        }
    }

    /// Spawns `kind` on the ECS and gives it the network id `id`.
    pub(in crate::game) fn spawn_id(game: &mut Game<'_>, id: u32, kind: &str) {
        game.next_entity = id - 1;
        let got = game
            .ecs_spawn_mob(kind, [0.0; 3], true, None, crate::game::creatures::SpawnSite::Raw)
            .unwrap();
        assert_eq!(got, id);
    }

    pub(in crate::game) fn spawn(game: &mut Game<'_>, id: u32, kind: &str) -> MerchantRef {
        spawn_id(game, id, kind);
        let entity = game.entity(id).unwrap();
        game.merchant(entity).unwrap()
    }

    #[test]
    #[ignore = "daily"]
    fn unresolved_profession_preserves_offers_but_cannot_trade_them() {
        with_game(|game| {
            // A mod's profession that is not loaded: read from the save, kept by name.
            use horizon_core::nbt::{Compound, JavaString, Tag};
            let mut data = Compound::default();
            data.insert("type".into(), Tag::String(JavaString::from("minecraft:plains")));
            data.insert("profession".into(), Tag::String(JavaString::from("example:engineer")));
            data.insert("level".into(), Tag::Int(1));
            let mut saved = Compound::default();
            saved.insert("id".into(), Tag::String(JavaString::from("minecraft:villager")));
            saved.insert("Pos".into(), Tag::List(vec![Tag::Double(0.0f64.to_bits()); 3]));
            saved.insert("VillagerData".into(), Tag::Compound(data));
            saved.insert(
                "Offers".into(),
                horizon_client::entity_nbt::offers_to_nbt(&[offer("minecraft:wheat", "minecraft:emerald")]),
            );
            game.next_entity = 39;
            assert_eq!(game.ecs_load(&saved), Some(40));
            let villager = game.merchant(game.entity(40).unwrap()).unwrap();
            assert_eq!(villager.offers(game), Some(Vec::new()));
            assert!(!villager.set_profession(game, "minecraft:farmer"));
            assert!(game.interact_trader_ecs(40, "minecraft:air").unwrap());
            assert!(game.station.is_none());
            assert_eq!(game.ecs.trade_offers(40).unwrap().len(), 1);
            assert_eq!(
                villager.profession(game).as_deref(),
                Some("example:engineer")
            );
        });
    }

}
