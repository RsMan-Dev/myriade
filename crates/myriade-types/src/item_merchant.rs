use crate::*;


/// An item payment, with an optional exact component predicate.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemCost {
    /// Item and minimum count required for this payment.
    pub stack: ItemStack,
}

impl ItemCost {
    /// Construct a payment from a nonempty item value.
    pub fn new(stack: ItemStack) -> Self {
        Self { stack }
    }
    /// Whether `stack` matches the payment's identity and explicit component predicate.
    pub fn test(&self, stack: &ItemStack) -> bool {
        stack.same_item(&self.stack)
            && (self.stack.components().is_none() || stack.same_components(&self.stack))
    }
}

/// A merchant offer copied at an API boundary, not a reference into the game's trade table.
#[derive(Clone, Debug, PartialEq)]
pub struct MerchantOffer {
    /// Undiscounted first payment.
    pub base_cost_a: ItemCost,
    /// Optional second payment.
    pub cost_b: Option<ItemCost>,
    /// Items received in exchange.
    pub result: ItemStack,
    /// Trades already performed since restocking.
    pub uses: i32,
    /// Trades allowed before the offer is out of stock.
    pub max_uses: i32,
    /// Whether completing this trade awards experience.
    pub reward_exp: bool,
    /// Player-specific price adjustment.
    pub special_price_diff: i32,
    /// Demand accumulated during restocking.
    pub demand: i32,
    /// Multiplier for demand and reputation price adjustments.
    pub price_multiplier: f32,
    /// Villager experience awarded by this trade.
    pub xp: i32,
}

impl MerchantOffer {
    /// Construct an unused offer with explicit payments, output, limits and experience.
    /// Discounts and demand begin at zero; completing it rewards experience.
    pub fn new(
        base_cost_a: ItemCost,
        cost_b: Option<ItemCost>,
        result: ItemStack,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
    ) -> Self {
        Self {
            base_cost_a,
            cost_b,
            result,
            uses: 0,
            max_uses,
            reward_exp: true,
            special_price_diff: 0,
            demand: 0,
            price_multiplier,
            xp,
        }
    }
    /// The optional second payment.
    pub fn cost_b(&self) -> Option<ItemStack> {
        self.cost_b.as_ref().map(|cost| cost.stack.clone())
    }
    /// Whether this offer has exhausted its uses.
    pub fn out_of_stock(&self) -> bool {
        self.uses >= self.max_uses
    }
    /// Apply `MerchantOffer.updateDemand` to this snapshot.
    pub fn update_demand(&mut self) {
        self.demand = self.demand + self.uses - (self.max_uses - self.uses);
    }
}
