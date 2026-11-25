//! What the jar fixes per species for the sounds an entity makes: the category it plays in (`Entity.getSoundSource`), the volume of its voice
//! (`LivingEntity.getSoundVolume`) and its own step (`playStepSound` overrides: a fixed sound at 0.15 instead of the block's).
use bevy_ecs::prelude::*;

/// `SoundSource` indices (`SOUND_SOURCES` order): hostile, neutral, ambient.
pub fn source_of(name: &str) -> usize {
    use crate::species::SoundCategory::*;
    match crate::species::registry().by_name(name).map(|s| s.sound_source) {
        Ok(Hostile) => 5,
        Ok(Ambient) => 8,
        _ => 6,
    }
}

/// `getSoundVolume` of the type: 1 unless it overrides. A cube mob scales with its size, a happy ghast is quiet as a baby.
pub fn volume_of(w: &World, e: Entity) -> f32 {
    let name = crate::area::species_name(w, e);
    match name {
        "minecraft:bat" => 0.1,
        "minecraft:ghast" | "minecraft:ender_dragon" => 5.0,
        "minecraft:warden" => 4.0,
        "minecraft:happy_ghast" => {
            if w.get::<crate::cow::GrowAt>(e).is_some() {
                1.0
            } else {
                4.0
            }
        }
        "minecraft:cow" | "minecraft:mooshroom" | "minecraft:squid" | "minecraft:glow_squid" | "minecraft:bee" | "minecraft:wolf" | "minecraft:allay" => 0.4,
        "minecraft:slime" | "minecraft:magma_cube" => 0.4 * w.get::<crate::size::Size>(e).map_or(1, |s| s.0) as f32,
        "minecraft:horse" | "minecraft:donkey" | "minecraft:mule" | "minecraft:skeleton_horse" | "minecraft:zombie_horse" | "minecraft:llama"
        | "minecraft:trader_llama" | "minecraft:camel" | "minecraft:camel_husk" => 0.8,
        _ => 1.0,
    }
}

/// The step of a type that overrides `playStepSound` with a fixed sound: its name and volume (pitch 1).
pub fn step_of(name: &str) -> Option<(&'static str, f32)> {
    const Q: f32 = 0.15;
    Some(match name {
        "minecraft:cow" | "minecraft:mooshroom" => ("entity.cow.step", Q),
        "minecraft:pig" => ("entity.pig.step", Q),
        "minecraft:chicken" => ("entity.chicken.step", Q),
        "minecraft:sheep" => ("entity.sheep.step", Q),
        "minecraft:wolf" => ("entity.wolf.step", Q),
        "minecraft:zombie" | "minecraft:zombified_piglin" => ("entity.zombie.step", Q),
        "minecraft:husk" => ("entity.husk.step", Q),
        "minecraft:drowned" => ("entity.drowned.step", Q),
        "minecraft:zombie_villager" => ("entity.zombie_villager.step", Q),
        "minecraft:skeleton" => ("entity.skeleton.step", Q),
        "minecraft:wither_skeleton" => ("entity.wither_skeleton.step", Q),
        "minecraft:stray" => ("entity.stray.step", Q),
        "minecraft:bogged" => ("entity.bogged.step", Q),
        "minecraft:parched" => ("entity.parched.step", Q),
        "minecraft:spider" | "minecraft:cave_spider" => ("entity.spider.step", Q),
        "minecraft:ravager" => ("entity.ravager.step", Q),
        "minecraft:zoglin" => ("entity.zoglin.step", Q),
        "minecraft:silverfish" => ("entity.silverfish.step", Q),
        "minecraft:endermite" => ("entity.endermite.step", Q),
        "minecraft:iron_golem" => ("entity.iron_golem.step", 1.0),
        "minecraft:warden" => ("entity.warden.step", 10.0),
        "minecraft:panda" => ("entity.panda.step", Q),
        "minecraft:llama" | "minecraft:trader_llama" => ("entity.llama.step", Q),
        "minecraft:parrot" => ("entity.parrot.step", Q),
        "minecraft:sniffer" => ("entity.sniffer.step", Q),
        "minecraft:frog" => ("entity.frog.step", Q),
        "minecraft:armadillo" => ("entity.armadillo.step", Q),
        "minecraft:goat" => ("entity.goat.step", Q),
        "minecraft:polar_bear" => ("entity.polar_bear.step", Q),
        "minecraft:piglin" => ("entity.piglin.step", Q),
        "minecraft:piglin_brute" => ("entity.piglin_brute.step", Q),
        "minecraft:hoglin" => ("entity.hoglin.step", Q),
        "minecraft:creaking" => ("entity.creaking.step", Q),
        _ => return None,
    })
}
