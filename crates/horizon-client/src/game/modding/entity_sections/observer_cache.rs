//! Retain the live observer's two allocations across borrowed creature passes.
#[derive(Default)]
pub(crate) struct ObserverCache;

#[cfg(all(test, feature = "alloc-count", not(feature = "system-allocator")))]
mod tests {
    use super::*;

    #[test]
    fn subscribed_observer_pass_allocates_only_on_first_use() {
        use horizon_threads::alloc_count::here;
        let mut cache = ObserverCache::default();
        let mut sections = EntitySections::default();
        let scope = Scope {
            epoch: mod_api::WorldEpoch::new(1),
            level: crate::game::Dim::Overworld.key(),
        };
        for expected in [2, 0, 0] {
            let before = here();
            let (table, observer) = cache.begin(&mut sections, Some(scope));
            drop(observer);
            std::mem::swap(&mut sections, &mut table.unwrap().borrow_mut());
            let counts = here().since(before);
            assert_eq!(counts.allocs, expected);
            assert_eq!(counts.frees, 0);
        }
    }
}
