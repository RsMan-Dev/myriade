//! Owning-thread, load-bound numeric bridge. Workers receive prepared values instead.
use super::{TryIntoApi, TryIntoEngine};
use horizon_world::registry::{BlockRegistry, BlockStateId};
use myriade_types::BlockState;
use myriade_types::BlockStateError;
use std::{cell::RefCell, sync::Arc};

/// A fully checked index, inert until explicitly installed by the owning game thread.
pub struct Index {
    forward: Vec<BlockState>,
    reverse: std::collections::HashMap<myriade_types::BlockKey, Vec<Option<BlockStateId>>>,
}
thread_local! {
    static INDEX: RefCell<Option<Arc<Index>>> = const { RefCell::new(None) };
}

/// Whether every part on `state` that answers `explosion_resistance` declares the answer pure.
pub fn explosion_resistance_pure(state: BlockStateId) -> bool {
    INDEX.with_borrow(|index| {
        index
            .as_ref()
            .and_then(|index| index.forward.get(state.0 as usize))
            .is_some()
    })
}

pub fn clear() {
    INDEX.with_borrow_mut(|index| *index = None);
}
pub fn snapshot() -> Result<Box<[BlockState]>, BlockStateError> {
    Ok(current()?.forward.clone().into_boxed_slice())
}
pub fn bind(registry: &BlockRegistry) -> Result<(), String> {
    clear();
    install(prepare(registry)?);
    Ok(())
}

pub fn install(prepared: Arc<Index>) {
    INDEX.with_borrow_mut(|index| *index = Some(prepared));
}

/// Prepare without activating thread-local state. A refused reload keeps its old live tables.
pub fn prepare(registry: &BlockRegistry) -> Result<Arc<Index>, String> {
    let forward = (0..registry.len())
        .map(|id| {
            registry
                .state(BlockStateId(id as u32))
                .expect("registry index")
                .try_into_api()
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("block state index: {error:?}"))?;
    let mut reverse = std::collections::HashMap::<_, Vec<Option<BlockStateId>>>::new();
    for (id, state) in forward.iter().enumerate() {
        let states = reverse.entry(state.block()).or_default();
        states.resize(states.len().max(state.ordinal() as usize + 1), None);
        if states[state.ordinal() as usize]
            .replace(BlockStateId(id as u32))
            .is_some()
        {
            return Err("duplicate block state in numeric registry".into());
        }
    }
    Ok(Arc::new(Index { forward, reverse }))
}
/// Capture this registry's immutable index explicitly; never resolves on a worker.
pub fn current() -> Result<Arc<Index>, BlockStateError> {
    INDEX.with_borrow(|index| index.clone().ok_or(BlockStateError::Unavailable))
}
impl Index {
    pub fn to_api(&self, state: BlockStateId) -> Result<BlockState, BlockStateError> {
        self.forward
            .get(state.0 as usize)
            .copied()
            .ok_or(BlockStateError::UnknownState)
    }
    pub fn to_engine(&self, state: BlockState) -> Result<BlockStateId, BlockStateError> {
        let states = self
            .reverse
            .get(&state.block())
            .filter(|states| !states.is_empty())
            .ok_or(BlockStateError::UnknownBlock)?;
        states
            .get(state.ordinal() as usize)
            .copied()
            .flatten()
            .ok_or(BlockStateError::UnknownState)
    }
}
impl TryIntoApi for BlockStateId {
    type Api = BlockState;
    type Error = BlockStateError;
    fn try_into_api(&self) -> Result<Self::Api, Self::Error> {
        INDEX.with_borrow(|index| {
            index
                .as_ref()
                .ok_or(BlockStateError::Unavailable)?
                .to_api(*self)
        })
    }
}
impl<C: ?Sized> TryIntoEngine<C> for BlockState {
    type Engine = BlockStateId;
    type Error = BlockStateError;
    fn try_into_engine(&self, _: &C) -> Result<Self::Engine, Self::Error> {
        INDEX.with_borrow(|index| {
            index
                .as_ref()
                .ok_or(BlockStateError::Unavailable)?
                .to_engine(*self)
        })
    }
}
