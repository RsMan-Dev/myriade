//! Every block state's facts and looks by its id, for a client mod drawing blocks or a map off the main thread
//! (`native_block_states`; docs/native-mods.md): what the game knows of each state, what its model draws on each side
//! and the texture behind each of those sprites. A copy, so the game's own tables can change shape under it.
use crate::streaming::WorkerContext;
use horizon_items::blocks::{BlockFacts, MAP_COLORS, RenderShape};
use horizon_world::{
    block_collision::BlockCollisionRegistry, fluid::FluidState, registry::BlockStateId,
};
use std::sync::Arc;

/// What one block state is, as a renderer or a map reads it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BlockStateFacts {
    /// Its block, without properties (`minecraft:grass_block`).
    pub name: Arc<str>,
    /// `BlockState.isAir`: air, cave air or void air.
    pub air: bool,
    /// No model draws it (render shape `INVISIBLE`): air, barriers, light, and water and lava, drawn as fluids.
    pub invisible: bool,
    /// `BlockState.getFluidState`: the fluid in it, a waterlogged block's and kelp's included; empty for none.
    pub fluid: FluidState,
    /// Its `waterlogged` property is true.
    pub waterlogged: bool,
    /// The light it gives, 0 to 15.
    pub emission: u8,
    /// `BlockState.getShape`, the outline the cursor picks, as block-local boxes
    /// `[min_x, min_y, min_z, max_x, max_y, max_z]`: `[0, 0, 0, 1, 1, 1]` for a whole block, none for air. Without
    /// the random offset of flowers and grass.
    pub outline: Arc<[[f64; 6]]>,
    /// `MapColor.col` (`0xRRGGBB`), its colour on a vanilla map; none for `MapColor.NONE`, which maps leave out.
    pub map_color: Option<u32>,
    /// It can collide at all: false for a state whose collision shape is always empty (air, flowers, water).
    pub collides: bool,
}

// A mod builds its tables from these on a worker thread.
const _: () = {
    const fn shareable<T: Send + Sync + 'static>() {}
    shareable::<BlockStates>()
};

/// Every block state by its id: its facts, and what its model draws with the resources loaded when it was made.
#[derive(Clone)]
pub struct BlockStates {
    states: Arc<[BlockStateFacts]>,
}

impl BlockStates {
    /// The states of `context`'s registry, with their `facts` and `collision`.
    pub fn new(
        facts: &BlockFacts,
        collision: &BlockCollisionRegistry<'_>,
        context: Arc<WorkerContext>,
    ) -> Self {
        let registry = context.assets.registry;
        let states = facts
            .states
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let id = BlockStateId(i as u32);
                BlockStateFacts {
                    name: f.name.clone(),
                    air: facts.is_air(id),
                    invisible: f.render_shape == RenderShape::Invisible,
                    fluid: registry
                        .state(id)
                        .and_then(|state| FluidState::from_block(state).ok())
                        .unwrap_or(FluidState::EMPTY),
                    waterlogged: f.waterlogged,
                    emission: f.emission,
                    outline: f.outline.clone(),
                    map_color: MAP_COLORS
                        .get(usize::from(f.map_color))
                        .copied()
                        .filter(|_| f.map_color != 0),
                    collides: collision.may_collide(id),
                }
            })
            .collect();
        Self { states }
    }

    /// How many states there are: their ids run from 0 to one less.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// `state`'s facts; none for an id past the last state.
    pub fn get(&self, state: BlockStateId) -> Option<&BlockStateFacts> {
        self.states.get(state.0 as usize)
    }

    /// Every state with its id, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (BlockStateId, &BlockStateFacts)> {
        self.states
            .iter()
            .enumerate()
            .map(|(i, facts)| (BlockStateId(i as u32), facts))
    }
}
