//! Profession preparation participates in the existing WorldTables root publication.
use super::*;
impl Game<'_> {
    pub(super) fn prepare_professions(
        &self,
        items: &Items,
        stations: &mut myriade_engine::stations::StationData,
    ) -> Result<()> {
        let pois = horizon_world::poi::PoiSnapshot::prepare(Vec::new())
            .map_err(|reason| format!("{reason}"))?;
        let mut candidate = myriade_engine::mobs::villager::ResolvedProfessions::prepare();
        candidate.add_secondary_tags(&items.block_tags);
        candidate.pois = pois;
        stations.professions = Arc::new(candidate);
        stations.trades.professions = stations.professions.clone();
        Ok(())
    }
}

impl Game<'_> {
    pub(super) fn install_profession_save_snapshots(&self) {
        let table = self.stations().professions.clone();
        self.save.install_profession_snapshot(table.clone());
        self.levels.install_profession_save_snapshots(table);
    }
}

impl Game<'_> {
    pub(in crate::game) fn with_profession_save_snapshot(
        &self,
        save: Arc<SaveDir>,
    ) -> Arc<SaveDir> {
        save.install_profession_snapshot(self.stations().professions.clone());
        save
    }
}
