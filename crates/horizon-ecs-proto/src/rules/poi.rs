use horizon_world::poi::PoiType;

/// The order of the points near a centre: squared distance, then chunk (z, then x), then y, z, x.
pub type NearbyKey = (i64, i32, i32, i32, i32, i32);

/// The section (16 blocks on a side) a block position lies in.
pub fn section(p: [i32; 3]) -> [i32; 3] {
    [p[0] >> 4, p[1] >> 4, p[2] >> 4]
}

/// `PoiTypes.IS_VILLAGE`: the points a village is made of.
pub fn is_village(k: PoiType) -> bool {
    k.is_job() || matches!(k, PoiType::Home | PoiType::Meeting)
}

/// Whether `p` lies in the cube of `radius` blocks around `center` (on each axis).
pub fn within_cube(p: [i32; 3], center: [i32; 3], radius: i32) -> bool {
    (0..3).all(|i| (p[i] - center[i]).abs() <= radius)
}

/// The sort key of `p` seen from `center` (`getInRange`'s order).
pub fn nearby_order(p: [i32; 3], center: [i32; 3]) -> NearbyKey {
    ((0..3).map(|i| i64::from(p[i] - center[i]).pow(2)).sum::<i64>(), p[2] >> 4, p[0] >> 4, p[1], p[2], p[0])
}
