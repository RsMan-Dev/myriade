//! Calculations of the tame cat's goals: the halved delay of the selector's asks, and the block above a cat is empty.

/// `MathHelper`-free `reduced(n) = (n + 1) / 2`: the vanilla halving of a delay drawn for the selector's asks.
pub fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}

/// `Mob`'s `empty_above`: the block above `p` is air.
pub fn empty_above(mw: &myriade_engine_lib::mobs::MobWorld, p: [i32; 3]) -> bool {
    mw.block_name(p[0], p[1] + 1, p[2]) == "minecraft:air"
}
