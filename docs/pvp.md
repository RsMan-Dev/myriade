# PvP

## 1.8 combat on a dedicated server (`combatRules`, default off)

The rules apply on a dedicated server whose `server.toml` has `[combat]` `legacy = true` ([hosting.md](hosting.md)).
Melee hits follow 1.8.9's `EntityPlayer.attackTargetEntityWithCurrentItem`:

- no attack cooldown (every hit is full strength) and no sweep;
- weapon damage as in 1.8.9: base 1, plus 4 for a sword, 3 for an axe, 2 for a pickaxe, 1 for a shovel, plus the
  material (wood and gold 0, stone 1, iron 2, diamond 3; netherite 4 and copper 1, which 1.8.9 didn't have);
  items 1.8.9 didn't have (trident, mace...) keep 26.3's damage;
- Strength multiplies by 1 + 1.3 per level, Weakness subtracts 0.5 per level;
- Sharpness adds 1.25 per level, Smite and Bane of Arthropods 2.5 per level against their targets;
- a critical hit (×1.5) needs a fall, not a charge, and can be a sprinting hit.
- knockback on players as `EntityLivingBase.knockBack`: it lifts a player in the air too (combos), and knockback
  resistance is a chance to take none instead of a cut; the sprint and Knockback push is added on top (0.1 up)
  instead of halving the motion first.

Code: `crates/horizon-client/src/legacy_combat.rs` (the formulas, with their 1.8.9 sources),
`game/world/legacy_rules.rs`.
