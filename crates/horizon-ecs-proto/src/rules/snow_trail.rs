use horizon_client_lib::mobs::MobWorld;

/// The four corners of the box (±`corner` on x and z) at the feet row `y`, as cells, in the Mob's order.
pub fn corner_cells(x: f64, z: f64, y: i32, corner: f64) -> [[i32; 3]; 4] {
    std::array::from_fn(|i| {
        let i = i as i32;
        let xx = (x + f64::from(i % 2 * 2 - 1) * corner).floor() as i32;
        let zz = (z + f64::from(i / 2 % 2 * 2 - 1) * corner).floor() as i32;
        [xx, y, zz]
    })
}

/// The golem burns: its dimension melts snow golems, or the temperature is above the melting one.
pub fn melts(dimension_melts: bool, temperature: f64, melt_temperature: f64) -> bool {
    dimension_melts || temperature > melt_temperature
}

/// `SnowLayerBlock.canSurvive`: the tags decide first, then a full top face or a full snow layer.
pub fn supports(mw: &MobWorld, below: [i32; 3]) -> bool {
    let name = mw.block_name(below[0], below[1], below[2]);
    if mw.has_tag("minecraft:cannot_support_snow_layer", name) {
        return false;
    }
    mw.has_tag("minecraft:support_override_snow_layer", name)
        || mw.top_face_full(below)
        || (name == "minecraft:snow" && mw.prop(below[0], below[1], below[2], "layers") == Some("8"))
}
