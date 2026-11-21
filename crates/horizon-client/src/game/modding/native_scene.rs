//! Scene blocks drawn in a screen, as `native_scene_live` takes them.
/// Prepared scene data. Entity fields remain borrowed for the duration of the draw.
pub(super) struct SceneBlock<'a> {
    pub pos: [i32; 3],
    pub state: horizon_world::registry::BlockStateId,
    pub entity: Option<&'a mod_api::SceneBlockEntity>,
}
