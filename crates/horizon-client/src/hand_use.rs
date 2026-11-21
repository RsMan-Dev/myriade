//! The local player's hand: the arm swing (`LivingEntity.SwingState`,
//! `getModifiedSwingDuration`), the use results of `startUseItem` and the
//! [`HandWorld`] the click handling reads and changes.
use mod_api::client::first_person::Hand;

/// `SwingAnimation`: its type (`SwingAnimationType` ordinal) and duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwingAnimation {
    pub kind: u8,
    pub duration: i32,
}
impl SwingAnimation {
    /// `SwingAnimation.DEFAULT`: WHACK for 6 ticks.
    pub const DEFAULT: SwingAnimation = SwingAnimation {
        kind: 1,
        duration: 6,
    };
}

/// `LivingEntity.SwingDescription`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwingDescription {
    pub hand: Hand,
    pub animation: SwingAnimation,
    pub duration_ticks: i32,
}

/// `LivingEntity.SwingState`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwingState {
    pub current: Option<SwingDescription>,
    pub ticks: i32,
    pub old_animation: f32,
    pub animation: f32,
}
impl SwingState {
    /// `startIfAble`: a swing restarts unless the current one is in its
    /// first half (past its first tick).
    pub fn start_if_able(&mut self, hand: Hand, animation: SwingAnimation, duration: i32) -> bool {
        if let Some(c) = self.current
            && self.ticks <= c.duration_ticks / 2
            && self.ticks > 0
        {
            return false;
        }
        let swing = SwingDescription {
            hand,
            animation,
            duration_ticks: duration,
        };
        if self.current != Some(swing) {
            self.old_animation = 0.0;
            self.animation = 0.0;
        }
        self.current = Some(swing);
        self.ticks = 0;
        true
    }
    /// `tick` (from `LivingEntity.baseTick`).
    pub fn tick(&mut self) {
        self.old_animation = self.animation;
        if let Some(c) = self.current {
            if c.duration_ticks > 0 {
                self.animation = (self.ticks as f32 / c.duration_ticks as f32).min(1.0);
            }
            let ticks = self.ticks;
            self.ticks += 1;
            if ticks > c.duration_ticks {
                self.current = None;
                self.animation = 0.0;
            }
        } else {
            self.animation = 0.0;
        }
    }
    pub fn swinging(&self) -> bool {
        self.current.is_some()
    }
    /// The hand of the current swing (`state.attackHand`), main by default.
    pub fn attack_hand(&self) -> Hand {
        self.current.map_or(Hand::Main, |c| c.hand)
    }
}

/// `LivingEntity.getModifiedSwingDuration`: `dig_speed` is
/// `MobEffectUtil.getDigSpeedAmplification` while haste or conduit power is
/// active, `fatigue` the mining fatigue amplifier.
pub fn swing_duration(
    animation: SwingAnimation,
    dig_speed: Option<i32>,
    fatigue: Option<i32>,
) -> i32 {
    let duration = animation.duration;
    if let Some(amplification) = dig_speed {
        duration - (1 + amplification)
    } else if let Some(amplifier) = fatigue {
        duration + (1 + amplifier) * 2
    } else {
        duration
    }
}

/// `HitResult` at the crosshair (`Minecraft.hitResult`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Miss,
    Block { pos: [i32; 3], face: u8 },
    Entity,
}

/// `InteractionResult` as `startUseItem` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseResult {
    /// `Success` with its `SwingSource`: whether the client swings.
    Success {
        swing: bool,
    },
    Pass,
    Fail,
}

impl UseResult {
    pub fn should_swing(self, before: u32, after: u32) -> bool {
        matches!(self, Self::Success { swing: true }) && before == after
    }
}

/// What the click handling reads and does in the world.
pub trait HandWorld {
    /// What a break keeps of the main-hand stack it began with
    /// (`destroyingItem`): the stack itself in the game.
    type Held: Clone + Default + PartialEq;
    fn hit(&mut self) -> Hit;
    fn is_air(&mut self, pos: [i32; 3]) -> bool;
    /// `MultiPlayerGameMode.localPlayerMode`.
    fn creative(&self) -> bool;
    fn spectator(&self) -> bool;
    /// `Abilities.instabuild`.
    fn instabuild(&self) -> bool;
    fn hands_busy(&self) -> bool;
    fn using_item(&self) -> bool;
    /// `ItemStack.isItemEnabled` for the item in a hand.
    fn item_enabled(&mut self, hand: Hand) -> bool;
    fn held_empty(&mut self, hand: Hand) -> bool;
    fn held_count(&mut self, hand: Hand) -> i32;
    /// `Player.hasInfiniteMaterials`.
    fn infinite_materials(&self) -> bool;
    /// `Player.cannotAttackWithItem(heldItem, 0)`.
    fn cannot_attack(&mut self) -> bool;
    /// The main-hand item is a `PIERCING_WEAPON`.
    fn piercing_weapon(&mut self) -> bool;
    fn attack_animation(&mut self) -> SwingAnimation;
    fn interact_animation(&mut self, hand: Hand) -> SwingAnimation;
    /// `MultiPlayerGameMode.piercingAttack`.
    fn piercing_attack(&mut self, animation: SwingAnimation);
    /// `Player.attack` on the targeted entity when the item's attack range
    /// allows it; false when out of range.
    fn attack_entity(&mut self) -> bool;
    /// `Player.blockActionRestricted` for the game mode.
    fn block_action_restricted(&mut self, pos: [i32; 3]) -> bool;
    /// `WorldBorder.isWithinBounds`.
    fn within_border(&mut self, pos: [i32; 3]) -> bool;
    /// `BlockState.attack` on the first hit of a break.
    fn attack_block(&mut self, pos: [i32; 3]);
    /// `BlockState.getDestroyProgress` for the player.
    fn destroy_progress(&mut self, pos: [i32; 3]) -> f32;
    /// `MultiPlayerGameMode.destroyBlock`.
    fn destroy_block(&mut self, pos: [i32; 3]) -> bool;
    /// The main-hand stack, kept as `destroyingItem`.
    fn main_hand_key(&mut self) -> Self::Held;
    /// `ItemStack.isSameItemSameComponents(selected, destroyingItem)` for
    /// what `main_hand_key` gave when the break started. Vanilla keeps a
    /// reference to that stack, so it compares what it holds now.
    fn same_destroying_item(&mut self, began: &Self::Held) -> bool {
        self.main_hand_key() == *began
    }
    /// Breaking progress shown (`destroyBlockProgress`), or cleared with `None`.
    fn destroy_stage(&mut self, pos: [i32; 3], stage: Option<i32>);
    /// `player.swing(hand, animation, false)`.
    fn swing(&mut self, hand: Hand, animation: SwingAnimation);
    /// `Player.resetAttackStrengthTicker`.
    fn reset_attack_strength(&mut self);
    /// The targeted entity's block position is inside the world border.
    fn entity_within_border(&mut self) -> bool;
    /// The use attempts of `startUseItem`: on the targeted entity (`None`
    /// when it is out of interaction range), the targeted block, and the
    /// item alone.
    fn use_on_entity(&mut self, hand: Hand) -> Option<UseResult>;
    fn use_on_block(&mut self, hand: Hand) -> UseResult;
    fn use_item(&mut self, hand: Hand) -> UseResult;
    /// `LocalPlayer.itemUsed`.
    fn item_used(&mut self, hand: Hand);
    /// `MultiPlayerGameMode.releaseUsingItem`.
    fn release_using(&mut self);
}
