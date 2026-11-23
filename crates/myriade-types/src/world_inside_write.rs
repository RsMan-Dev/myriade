use crate::*;


/// Vanilla `InsideBlockEffectType`: what a block does to the entity inside it, beyond
/// the writes it makes directly. The collector flushes them per step in this order.
///
/// The effects apply to the swept entity inside its own tick: `Freeze` is
/// `setIsInPowderSnow` plus a frozen tick, `ClearFreeze` thaws, `FireIgnite` is
/// `BaseFireBlock.fireIgnite` (with the level's random draw for a player), `LavaIgnite`
/// ignites for 15 seconds, `Extinguish` is `clearFire`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsideEffect {
    /// `InsideBlockEffectType.FREEZE`: powder snow's frost, one frozen tick a step.
    Freeze,
    /// `InsideBlockEffectType.CLEAR_FREEZE`: fire and lava thaw what powder snow froze.
    ClearFreeze,
    /// `InsideBlockEffectType.FIRE_IGNITE`: `BaseFireBlock.fireIgnite`, 8 seconds.
    FireIgnite,
    /// `InsideBlockEffectType.LAVA_IGNITE`: `Entity.lavaIgnite`, 15 seconds.
    LavaIgnite,
    /// `InsideBlockEffectType.EXTINGUISH`: water and powder snow put the fire out.
    Extinguish,
}

/// The order `StepBasedCollector` flushes a step's effects in, `InsideBlockEffectType.values()`.
const EFFECT_ORDER: [InsideEffect; 5] = [
    InsideEffect::Freeze,
    InsideEffect::ClearFreeze,
    InsideEffect::FireIgnite,
    InsideEffect::LavaIgnite,
    InsideEffect::Extinguish,
];

/// A typed write to the entity the sweep is inside, as vanilla's `entityInside` blocks
/// make them. Each applies inside the entity's own tick when the collector flushes, in
/// the step and type order vanilla's `StepBasedCollector` uses, and the rest stop when
/// the entity dies.
///
/// These are the writes vanilla blocks make: the bubble columns' push and fall reset,
/// the honey slide's movement, the cobweb's and berry bush's `makeStuckInBlock`, the
/// cactus's, campfire's and fire's `hurt`, the fire ticks fire and lava leave, and the
/// mob effects a wither rose and an eyeblossom add. A portal is not a write a part
/// makes: it stays the game's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntityWrite {
    /// Add to the entity's movement (`entity.push`), as a bubble column does.
    Push([f64; 3]),
    /// Replace the entity's movement (`setDeltaMovement`), as the honey slide does.
    SetMovement([f64; 3]),
    /// `makeStuckInBlock`: scale the entity's movement by these factors until it leaves.
    /// Resets its fall distance, as every stuck block does.
    StuckIn([f64; 3]),
    /// `hurt(source, amount)`. `source` is the id of the entity the damage came from,
    /// resolved through the game's one entity table when the write applies: a table that
    /// answers [`EntityLookup::Taken`] (its list is lifted out of the game that moment,
    /// ) refuses the write with [`HurtSourceError::Taken`] instead of retrying or
    /// panicking, and the refusal refuses the callback's sibling queued writes too, as an
    /// invalid landing write does.
    Hurt {
        /// The damaging entity's id, or none for the block itself.
        source: Option<u32>,
        /// The damage dealt.
        amount: f32,
    },
    /// `resetFallDistance`.
    ResetFallDistance,
    /// `setRemainingFireTicks`: fire's and lava's ignition, or water's extinguish (a
    /// negative or zero value).
    SetFireTicks(i32),
    /// `LivingEntity.addEffect(MobEffectInstance(effect, duration, amplifier))`, as a
    /// wither rose's wither (40 ticks) or an eyeblossom's bee poison (25) does. Applies
    /// when the swept entity is living; a kind that takes no effects skips it.
    AddEffect {
        /// The effect to add, a key requested once at registration.
        effect: EffectKey,
        /// How long the effect lasts, in ticks.
        duration: i32,
        /// The effect's amplifier, 0 for the plain effect.
        amplifier: u8,
    },
}

/// A write at the block an entity is inside, asked for when the part is told and applied
/// once the entity's batch has ticked, in the order asked, if the block is still the
/// state the entity touched. The same contract as a landing write: the complete
/// requested batch is checked before queuing, and an invalid state refuses all writes
/// and sibling outputs from that callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, )]
pub enum InsideWrite {
    /// `Level.setBlockAndUpdate(pos, state)`, notifying neighbors and clients.
    SetBlock(BlockState),
    /// `Level.destroyBlock(pos, drop)`, with or without resource drops.
    DestroyBlock {
        /// Whether the destroyed block drops its resources.
        drop: bool,
    },
}

/// One planned item of the flush: an effect, or an entity write that runs before or
/// after one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlannedInside {
    /// An [`InsideEffect`] applied to the swept entity.
    Effect(InsideEffect),
    /// An [`EntityWrite`] applied to the swept entity.
    Write(EntityWrite),
}

/// `InsideBlockEffectApplier.StepBasedCollector`: the effects and entity writes of one
/// block sweep. The walk advances a step per block it reaches; a step's effects flush
/// once each, in [`InsideEffect`] order, with the writes asked `run_before` an effect
/// before it and those asked `run_after` after it. Steps flush in the order the walk
/// reached them, and [`InsideEffects::take_planned`] hands the whole ordered plan to the
/// entity's own tick, which stops applying at the first item after the entity dies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InsideEffects {
    /// `lastStep`: the step the walk last advanced, `-1` before the first.
    pub last_step: i32,
    /// `effectsInStep`: the effect types queued for the current step.
    pub in_step: [bool; 5],
    /// `beforeEffectsInStep`: the writes asked before each effect this step.
    pub before: [Vec<EntityWrite>; 5],
    /// `afterEffectsInStep`: the writes asked after each effect this step.
    pub after: [Vec<EntityWrite>; 5],
    /// `finalEffects`: the flushed plan, steps in the order they were reached.
    pub planned: Vec<PlannedInside>,
}

impl InsideEffects {
    /// `advanceStep`: flush the current step when the walk reaches a new one. The walk
    /// calls this before a block's effects, as vanilla does.
    pub fn advance_step(&mut self, step: i32) {
        if self.last_step != step {
            self.last_step = step;
            self.flush_step();
        }
    }
    /// `apply`: queue an effect for the current step. A type queued twice in one step
    /// still applies once.
    pub fn apply(&mut self, effect: InsideEffect) {
        self.in_step[effect as usize] = true;
    }
    /// `runBefore`: a write that applies just before `effect` in the flush order. It
    /// runs even when nothing queued `effect` itself that step, as vanilla's do.
    pub fn run_before(&mut self, effect: InsideEffect, write: EntityWrite) {
        self.before[effect as usize].push(write);
    }
    /// `runAfter`: a write that applies just after `effect` in the flush order.
    pub fn run_after(&mut self, effect: InsideEffect, write: EntityWrite) {
        self.after[effect as usize].push(write);
    }
    /// `applyAndClear`: flush the last step and take the whole plan, in order. The
    /// caller applies the writes to the swept entity inside its own tick and stops at
    /// the first item after it dies.
    pub fn take_planned(&mut self) -> Vec<PlannedInside> {
        self.flush_step();
        self.last_step = -1;
        std::mem::take(&mut self.planned)
    }
    /// Commit another collector's current step into this one: a staged callback
    /// answered, so what it queued joins this step. The staged collector must not
    /// have flushed (its plan stays empty); a panicking callback is never absorbed.
    pub fn absorb_step(&mut self, staged: Self) {
        let mut staged = staged;
        for index in 0..5 {
            self.in_step[index] |= staged.in_step[index];
            let before = std::mem::take(&mut staged.before[index]);
            self.before[index].extend(before);
            let after = std::mem::take(&mut staged.after[index]);
            self.after[index].extend(after);
        }
    }

    /// Whether nothing is queued: the walk builds no collector work when no part listens.
    pub fn is_empty(&self) -> bool {
        self.planned.is_empty()
            && self.in_step == [false; 5]
            && self.before.iter().all(Vec::is_empty)
            && self.after.iter().all(Vec::is_empty)
    }
    /// `flushStep`: this step's writes and effects into the plan, in `InsideEffect`
    /// order, before the effect, the effect itself, then after it.
    fn flush_step(&mut self) {
        for (index, effect) in EFFECT_ORDER.into_iter().enumerate() {
            for write in self.before[index].drain(..) {
                self.planned.push(PlannedInside::Write(write));
            }
            if std::mem::take(&mut self.in_step[index]) {
                self.planned.push(PlannedInside::Effect(effect));
            }
            for write in self.after[index].drain(..) {
                self.planned.push(PlannedInside::Write(write));
            }
        }
    }
}
