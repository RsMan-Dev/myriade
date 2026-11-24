//! The query states kept by the world: a `QueryState` is built once per (data, filter) pair, the first time an action asks for it, and read
//! afterwards. Building one costs thousands of instructions (every archetype is matched against the access), reading a kept one a few hundred.
//!
//! bevy 0.20.1 (`query/state.rs`): `QueryState::iter`, `iter_mut`, `get` and `query` call `update_archetypes` themselves, which matches only
//! the archetypes created since the state's `archetype_generation`; nothing else is needed to keep a state current. Only the `_manual`
//! methods skip it, and nothing here uses them.
use bevy_ecs::{
    prelude::*,
    query::{QueryData, QueryFilter, QueryState},
};
use std::any::{Any, TypeId};
use std::collections::HashMap;

/// The kept states, one per `QueryState<D, F>` type (the key is that type: two actions asking for the same data and filter share it).
#[derive(Resource, Default)]
pub struct Queries(HashMap<TypeId, Box<dyn Any + Send + Sync>>);

/// `f` on the kept state of `(D, F)` (built on the first call) and the world: the state is out of the world's resources for the call.
pub fn with<D: QueryData + 'static, F: QueryFilter + 'static, R>(w: &mut World, f: impl FnOnce(&mut QueryState<D, F>, &mut World) -> R) -> R {
    if !w.contains_resource::<Queries>() {
        w.init_resource::<Queries>();
    }
    w.resource_scope(|w, mut kept: Mut<Queries>| {
        let state = kept.0.entry(TypeId::of::<QueryState<D, F>>()).or_insert_with(|| Box::new(QueryState::<D, F>::new(w)));
        f(state.downcast_mut::<QueryState<D, F>>().expect("the key is the state's own type"), w)
    })
}

/// The entities that match `F`.
pub fn entities<F: QueryFilter + 'static>(w: &mut World) -> Vec<Entity> {
    with::<Entity, F, _>(w, |q, w| q.iter(w).collect())
}
