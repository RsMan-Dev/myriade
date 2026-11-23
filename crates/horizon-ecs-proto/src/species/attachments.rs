//! The seats of each species: its passenger attachments (vehicle side) and its own vehicle attachment (rider side). Values are the
//! old Mob's table (`riders.rs`); a float in the builder's form is widened from f32 as vanilla's `float` is.
use crate::mount::{PassengerSeats, RiderSeat};
use std::borrow::Cow;

/// The passenger attachments of a vehicle, by species name. `None` is the default: the top of the bounding box.
pub fn passenger_seats(name: &str) -> Option<PassengerSeats> {
    let up = |adult: &'static [[f64; 3]]| Some(PassengerSeats { adult: Cow::Borrowed(adult), baby: None });
    Some(match name {
        "minecraft:chicken" => return up(&[[0.0, 0.7, -0.1]]),
        "minecraft:spider" => return up(&[[0.0, 0.765_f32 as f64, 0.0]]),
        "minecraft:ravager" => return up(&[[0.0, 2.2625, -0.0625]]),
        "minecraft:horse" => return up(&[[0.0, 1.44375_f32 as f64, 0.0]]),
        "minecraft:donkey" => return up(&[[0.0, 1.1125_f32 as f64, 0.0]]),
        "minecraft:mule" => return up(&[[0.0, 1.2125_f32 as f64, 0.0]]),
        "minecraft:skeleton_horse" | "minecraft:zombie_horse" => return up(&[[0.0, 1.31875_f32 as f64, 0.0]]),
        "minecraft:pig" => PassengerSeats {
            adult: Cow::Borrowed(&[[0.0, 0.86875_f32 as f64, 0.0]]),
            baby: Some([0.0, 0.5_f32 as f64, 0.0]),
        },
        "minecraft:hoglin" => PassengerSeats {
            adult: Cow::Borrowed(&[[0.0, 1.49375_f32 as f64, 0.0]]),
            baby: Some([0.0, 0.875_f32 as f64, 0.0]),
        },
        "minecraft:zoglin" => return up(&[[0.0, 1.49375_f32 as f64, 0.0]]),
        "minecraft:nautilus" | "minecraft:zombie_nautilus" => return up(&[[0.0, 1.1375_f32 as f64, 0.0]]),
        "minecraft:cat" => return up(&[[0.0, 0.5125_f32 as f64, 0.0]]),
        "minecraft:cow" | "minecraft:mooshroom" => return up(&[[0.0, 1.36875_f32 as f64, 0.0]]),
        "minecraft:sheep" => return up(&[[0.0, 1.2375_f32 as f64, 0.0]]),
        "minecraft:goat" => return up(&[[0.0, 1.1125_f32 as f64, 0.0]]),
        "minecraft:ocelot" => return up(&[[0.0, 0.6375_f32 as f64, 0.0]]),
        "minecraft:parrot" => return up(&[[0.0, 0.4625_f32 as f64, 0.0]]),
        "minecraft:phantom" => return up(&[[0.0, 0.3375_f32 as f64, 0.0]]),
        "minecraft:vex" => return up(&[[0.0, 0.7375_f32 as f64, 0.0]]),
        "minecraft:guardian" => return up(&[[0.0, 0.975_f32 as f64, 0.0]]),
        "minecraft:elder_guardian" => return up(&[[0.0, 2.350625_f32 as f64, 0.0]]),
        "minecraft:endermite" | "minecraft:silverfish" => return up(&[[0.0, 0.2375_f32 as f64, 0.0]]),
        "minecraft:enderman" => return up(&[[0.0, 2.80625_f32 as f64, 0.0]]),
        "minecraft:ghast" => return up(&[[0.0, 4.0625_f32 as f64, 0.0]]),
        "minecraft:happy_ghast" => return up(&[[0.0, 4.0, 1.7], [-1.7, 4.0, 0.0], [0.0, 4.0, -1.7], [1.7, 4.0, 0.0]]),
        "minecraft:warden" => return up(&[[0.0, 3.15_f32 as f64, 0.0]]),
        "minecraft:sniffer" => return up(&[[0.0, 2.09375_f32 as f64, 0.0]]),
        "minecraft:fox" => return up(&[[0.0, 0.6375, -0.25]]),
        "minecraft:frog" => return up(&[[0.0, 0.375, -0.25]]),
        "minecraft:llama" | "minecraft:trader_llama" => PassengerSeats {
            adult: Cow::Borrowed(&[[0.0, 1.37, -0.3]]),
            baby: Some([0.0, 0.81_f32 as f64, -0.15_f32 as f64]),
        },
        "minecraft:turtle" => return up(&[[0.0, 0.55625, -0.25]]),
        "minecraft:wolf" => return up(&[[0.0, 0.81875, -0.0625]]),
        "minecraft:witch" => return up(&[[0.0, 2.2625_f32 as f64, 0.0]]),
        "minecraft:evoker" | "minecraft:illusioner" | "minecraft:pillager" | "minecraft:vindicator" => {
            return up(&[[0.0, 2.0, 0.0]])
        }
        "minecraft:zombified_piglin" => return up(&[[0.0, 2.0, 0.0]]),
        "minecraft:zombie" | "minecraft:drowned" | "minecraft:piglin" | "minecraft:piglin_brute" => {
            return up(&[[0.0, 2.0125_f32 as f64, 0.0]])
        }
        "minecraft:husk" => return up(&[[0.0, 2.075_f32 as f64, 0.0]]),
        "minecraft:zombie_villager" => return up(&[[0.0, 2.125_f32 as f64, 0.0]]),
        _ => return None,
    })
}

/// The rider's own vehicle attachment (its feet hang this far below the attachment point), by species name. `None` is no offset.
pub fn rider_seat(name: &str) -> Option<RiderSeat> {
    let adult = |adult: f64| Some(RiderSeat { adult, baby: None, narrow: None });
    Some(match name {
        "minecraft:player" | "minecraft:mannequin" => return adult(0.6),
        "minecraft:spider" => RiderSeat { adult: 0.0, baby: None, narrow: Some(0.3125) },
        "minecraft:cave_spider" => RiderSeat { adult: 0.0, baby: None, narrow: Some(0.21875) },
        "minecraft:zombie" | "minecraft:husk" | "minecraft:drowned" => {
            RiderSeat { adult: 0.7_f32 as f64, baby: Some(0.1875), narrow: None }
        }
        "minecraft:zombie_villager" => RiderSeat { adult: 0.7_f32 as f64, baby: Some(0.125), narrow: None },
        "minecraft:piglin" => RiderSeat { adult: 0.7_f32 as f64, baby: Some(0.1875), narrow: None },
        "minecraft:zombified_piglin" | "minecraft:piglin_brute" | "minecraft:skeleton" | "minecraft:stray" | "minecraft:bogged"
        | "minecraft:parched" => return adult(0.7_f32 as f64),
        "minecraft:wither_skeleton" => return adult(0.875_f32 as f64),
        "minecraft:ghast" | "minecraft:happy_ghast" => return adult(-0.5_f32 as f64),
        "minecraft:allay" | "minecraft:vex" => return adult(-0.04_f32 as f64),
        "minecraft:phantom" => return adult(0.125_f32 as f64),
        "minecraft:giant" => return adult(3.75_f32 as f64),
        "minecraft:pillager" | "minecraft:vindicator" | "minecraft:evoker" | "minecraft:illusioner" => {
            return adult(0.6_f32 as f64)
        }
        _ => return None,
    })
}
