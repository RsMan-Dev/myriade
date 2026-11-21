//! The rules of the blocks the dragon's death builds: which block of the exit podium and of the gateway stands at a cell.

/// The block of the podium of `origin` (the portal level) at `p`, or `None` outside it: a sphere of radius 2.5 is the portal's
/// centre, the rim goes to 3.5; `active` fills the rim's centre with the portal.
pub fn podium_block(origin: [i32; 3], p: [i32; 3], active: bool) -> Option<&'static str> {
    let d = f64::from((p[0] - origin[0]).pow(2) + (p[1] - origin[1]).pow(2) + (p[2] - origin[2]).pow(2));
    let inside = d < 2.5 * 2.5;
    if !(inside || d < 3.5 * 3.5) {
        return None;
    }
    Some(match p[1].cmp(&origin[1]) {
        std::cmp::Ordering::Less if inside => "minecraft:bedrock",
        std::cmp::Ordering::Less => "minecraft:end_stone",
        std::cmp::Ordering::Greater => "minecraft:air",
        _ if !inside => "minecraft:bedrock",
        _ if active => "minecraft:end_portal",
        _ => "minecraft:air",
    })
}

/// The block of the gateway at `o`, an offset from its origin: the gateway in the centre, a bedrock cross, air elsewhere.
pub fn gateway_block(o: [i32; 3]) -> &'static str {
    let (sx, sy, sz, end) = (o[0] == 0, o[1] == 0, o[2] == 0, o[1].abs() == 2);
    if sx && sy && sz {
        "minecraft:end_gateway"
    } else if sy {
        "minecraft:air"
    } else if end && sx && sz || (sx || sz) && !end {
        "minecraft:bedrock"
    } else {
        "minecraft:air"
    }
}
