//! Borrowed adapters for load and loop hooks shared by client-lib and gameplay.
use horizon_world::native_load_context as engine;
use std::sync::Arc;



/// Borrows one owning load's frozen metadata, without scanning or copying it.
pub struct LoadReader<'a>(pub &'a engine::LoadContext);
impl LoadReader<'_> {
}

/// Borrows one loop's frozen metadata, without scanning or copying it.
pub struct LoopReader<'a>(pub &'a engine::LoopContext);
impl LoopReader<'_> {
}


#[cfg(test)]
mod tests {
    use super::*;

}
