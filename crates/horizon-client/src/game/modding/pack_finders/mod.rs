//! Mod-supplied repository sources. Discovery owns the metadata and retained
//! readers; selection and resource/data reloads use the ordinary pack paths.
use super::{api_values::TryIntoEngine, mods::ModsState};
use horizon_assets::resource_source::{ResourceSource, stays_inside};
use horizon_client::datapack::DataPack;
use mod_api::{
    ClientHost, HookId, Host, HostSide, PackFinderError, PackFinders, PackRegistration, PackSource, PackType,
    ServerHost,
};

#[cfg(test)]
mod tests;

/// A repository layer a mod's finder added: its id and its opened source.
pub(super) struct Found {
    id: String,
    source: ResourceSource,
    /// From the feature source: its flags are the world's choices (`feature_repository`).
    feature: bool,
    /// `always_active`: the selection always holds it.
    required: bool,
}

struct Registry {
    kind: PackType,
    mods: ModsState,
    packs: Vec<Found>,
}
impl Registry {
    fn new(kind: PackType, mods: ModsState) -> Self {
        Self {
            kind,
            mods,
            packs: Vec::new(),
        }
    }
}
impl PackFinders for Registry {
    fn pack_type(&self) -> PackType {
        self.kind
    }
    fn add_pack_finders(&mut self, entry: PackRegistration) -> Result<(), PackFinderError> {
        if entry.pack_type != self.kind {
            return Ok(());
        }
        let location = &entry.location;
        if !stays_inside(location.path()) {
            return Err(PackFinderError::InvalidPath);
        }
        let id = format!("mod/{location}");
        if self.packs.iter().any(|pack| pack.id == id) {
            return Err(PackFinderError::Duplicate);
        }
        let package = self
            .mods
            .native
            .packages()
            .iter()
            .map(|package| (package.manifest.id.as_str(), &package.path))
            .chain(
                self.mods
                    .selected
                    .iter()
                    .map(|package| (package.package.manifest.id.as_str(), &package.path)),
            )
            .find(|(id, _)| *id == location.namespace())
            .map(|(_, path)| path)
            .ok_or(PackFinderError::MissingPackage)?;
        let source = if package.is_dir() {
            ResourceSource::directory(package.clone())
        } else {
            // Exactly the entries the package's checks read, as for its own layer.
            horizon_assets::zip::Zip::open_exact(package)
                .map(ResourceSource::opened_archive)
                .map_err(|_| PackFinderError::MissingPackage)?
        };
        let source = source
            .subdirectory(location.path())
            .map_err(|_| PackFinderError::InvalidPath)?;
        // The title is not kept (no screen shows it), but a form the engine refuses is refused here as before.
        entry.title.try_into_engine(&())?;
        if !matches!(self.kind, PackType::ClientResources | PackType::ServerData) {
            return Err(PackFinderError::InvalidMetadata);
        }
        // `Pack.readPackMetadata`: a layer without a readable `pack.mcmeta` is no pack.
        if source.read("pack.mcmeta").ok().and_then(|bytes| horizon_client::pack_meta::parse_meta(&bytes)).is_none() {
            return Err(PackFinderError::InvalidMetadata);
        }
        let feature = match entry.source {
            PackSource::BuiltIn => false,
            PackSource::Feature => true,
            _ => return Err(PackFinderError::InvalidMetadata),
        };
        self.packs.push(Found { id, source, feature, required: entry.always_active });
        Ok(())
    }
}

/// A repository may be discovered before Game exists. As with startup reload
/// listeners, registration has no world reads; only the supplied builder is live.
struct DiscoveryHost;
impl Host for DiscoveryHost {}
mod_api::impl_host_areas!(DiscoveryHost);
impl ClientHost for DiscoveryHost {}
mod_api::impl_client_host_areas!(DiscoveryHost);
impl ServerHost for DiscoveryHost {}
mod_api::impl_server_host_areas!(DiscoveryHost);

pub(super) fn discover(kind: PackType, mods: &ModsState) -> Vec<Found> {
    if !mod_api::__private::listened(HookId::AddPackFinders) {
        return Vec::new();
    }
    let mut registry = Registry::new(kind, mods.clone());
    let mut host = DiscoveryHost;
    let host = match kind {
        PackType::ServerData => HostSide::Server(&mut host),
        _ => return Vec::new(),
    };
    mod_api::dispatch_add_pack_finders(host, &mut registry);
    registry.packs
}

pub(super) fn data_packs(mods: &ModsState) -> Vec<std::sync::Arc<DataPack>> {
    discover(PackType::ServerData, mods)
        .into_iter()
        .map(|pack| {
            let mut data = DataPack::from_source(pack.id, pack.source);
            data.feature = pack.feature;
            data.required = pack.required;
            std::sync::Arc::new(data)
        })
        .collect()
}

/// The same repository is used by commands, content loading and world generation.
pub(super) fn data_repository(
    world: &std::path::Path,
    resources: &std::path::Path,
    additional: &[std::sync::Arc<DataPack>],
) -> Vec<DataPack> {
    let mut packs = horizon_client::datapack::discover_with_features(world, resources);
    packs.extend(additional.iter().map(|pack| (**pack).clone()));
    packs
}
