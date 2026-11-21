//! Player skins from profile `textures` properties: authlib's
//! `MinecraftServicesSessionService.unpackTextures`, vanilla's
//! `SkinManager` lookup (which texture a player shows, and when the default
//! one) and `SkinTextureDownloader.processLegacySkin`.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// `Property.hasSignature` and the services key check, as authlib's
/// `SignatureState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signature {
    Unsigned,
    Signed,
    Invalid,
}

/// `MinecraftProfileTextures`: the texture URLs a profile names. Empty when
/// the payload doesn't decode or names a URL outside the allowed domains.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Textures {
    pub skin: Option<String>,
    /// The skin's `metadata.model` is `slim` (`PlayerModelType.SLIM`).
    pub slim: bool,
    pub cape: Option<String>,
    pub elytra: Option<String>,
    pub signed: bool,
}

/// The `getTexture` endpoint's valid URIs in production's services discovery
/// document (`MinecraftServicesEnvironment.PROD`).
pub const TEXTURE_URIS: [&str; 2] = [
    "https://textures.minecraft.net/texture/{textureId}",
    "http://textures.minecraft.net/texture/{textureId}",
];

/// `MinecraftServicesDiscoveryService`'s valid texture URIs: the discovery
/// document's `profiles` / `getTexture` `validUris`, which
/// `MinecraftServicesSessionService.isAllowedTextureDomain` checks texture URLs
/// against. Production's document lists [`TEXTURE_URIS`], so they are used
/// without asking it; another environment (`HORIZON_API_DISCOVERY`) is asked
/// once per game, and keeps [`TEXTURE_URIS`] when its document lists none or
/// can't be fetched.
pub fn texture_uris() -> &'static [String] {
    static URIS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    URIS.get_or_init(|| {
        crate::auth::discovery_overridden()
            .then(|| crate::auth::discovery().ok())
            .flatten()
            .and_then(|doc| discovery_texture_uris(&doc))
            .unwrap_or_else(|| TEXTURE_URIS.map(str::to_owned).to_vec())
    })
}

/// The `profiles` / `getTexture` endpoint's `validUris` in a discovery
/// document, when it lists any.
fn discovery_texture_uris(doc: &serde_json::Value) -> Option<Vec<String>> {
    let uris: Vec<String> = doc
        .pointer("/discovery/profiles/endpoints/getTexture/validUris")?
        .as_array()?
        .iter()
        .filter_map(|u| u.as_str().map(str::to_owned))
        .collect();
    (!uris.is_empty()).then_some(uris)
}

/// `java.util.Base64.getDecoder().decode`.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    use base64::{
        Engine,
        engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
    };
    STANDARD
        .decode(text)
        .or_else(|_| STANDARD_NO_PAD.decode(text))
        .ok()
}

// Limit owned string allocations as well as table entries: obsolete, rejected profile
// payloads must not accumulate a server's worth of maximum-size properties.
const SKIN_MEMO_BYTES: usize = 4 * 1024 * 1024;
struct SkinMemo<K, V> {
    values: std::collections::HashMap<K, (V, usize)>,
    bytes: usize,
}
impl<K, V> Default for SkinMemo<K, V> {
    fn default() -> Self {
        Self {
            values: std::collections::HashMap::new(),
            bytes: 0,
        }
    }
}
impl<K: Eq + std::hash::Hash, V> SkinMemo<K, V> {
    fn get(&self, key: &K) -> Option<&V> {
        self.values.get(key).map(|(value, _)| value)
    }
    fn remember(&mut self, key: K, value: V, bytes: usize) {
        if bytes > SKIN_MEMO_BYTES {
            return;
        }
        if self.values.len() >= 4096 || bytes > SKIN_MEMO_BYTES.saturating_sub(self.bytes) {
            self.values.clear();
            self.bytes = 0;
        }
        if let Some((_, previous)) = self.values.insert(key, (value, bytes)) {
            self.bytes -= previous;
        }
        self.bytes += bytes;
    }
}
thread_local! {
    static UNPACKED: std::cell::RefCell<SkinMemo<(String, u8), Textures>> =
        std::cell::RefCell::default();
    static LOCATIONS: std::cell::RefCell<SkinMemo<(String, String), String>> =
        std::cell::RefCell::default();
}

/// [`unpack`] with [`texture_uris`], remembered by the payload. A frame resolves every drawn
/// mannequin's skin, and decoding each one's base64 JSON again was most of that.
pub fn unpack_known(value: &str, signature: Signature) -> Textures {
    if value.len() > SKIN_MEMO_BYTES {
        return unpack(value, signature, texture_uris());
    }
    let key = (value.to_owned(), signature as u8);
    if let Some(textures) = UNPACKED.with_borrow(|known| known.get(&key).cloned()) {
        return textures;
    }
    let textures = unpack(value, signature, texture_uris());
    UNPACKED.with_borrow_mut(|known| {
        let cached = textures.clone();
        let bytes = key.0.capacity()
            + [&cached.skin, &cached.cape, &cached.elytra]
                .into_iter()
                .flatten()
                .map(String::capacity)
                .sum::<usize>();
        known.remember(key, cached, bytes);
    });
    textures
}

/// `unpackTextures(property)`: the textures of a `textures` property's
/// base64 JSON payload.
pub fn unpack(value: &str, signature: Signature, valid_uris: &[impl AsRef<str>]) -> Textures {
    let empty = Textures::default();
    let Some(json) = base64_decode(value)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    else {
        return empty;
    };
    let Some(textures) = json.get("textures").and_then(|t| t.as_object()) else {
        return empty;
    };
    if textures.is_empty() {
        return empty;
    }
    let mut out = Textures {
        signed: signature == Signature::Signed,
        ..empty.clone()
    };
    // Authlib's MinecraftServicesSessionService validates every URL in
    // unpackTextures before returning any textures. A bad cape rejects the skin too.
    for (kind, texture) in textures {
        let Some(url) = texture.get("url").and_then(|u| u.as_str()) else {
            return empty;
        };
        let allowed = valid_uris
            .iter()
            .any(|u| url.starts_with(&u.as_ref().replace("{textureId}", "")));
        if !allowed {
            return empty;
        }
        if kind == "SKIN" {
            out.slim = texture
                .get("metadata")
                .and_then(|m| m.get("model"))
                .and_then(|m| m.as_str())
                == Some("slim");
        }
        let slot = match kind.as_str() {
            "SKIN" => &mut out.skin,
            "CAPE" => &mut out.cape,
            "ELYTRA" => &mut out.elytra,
            _ => continue,
        };
        *slot = Some(url.to_owned());
    }
    out
}

/// The skin URL of a `minecraft:profile` component's property list. Player
/// heads and mannequins read their profile this way, and `SkinManager` lets
/// those textures be unsigned (`getInsecureSkinInformation`).
pub fn profile_skin_url(profile: &serde_json::Value) -> Option<String> {
    let (value, signature) = parse_profile(profile).textures?;
    unpack(&value, signature, texture_uris()).skin
}

/// The textures of a Minecraft services `/minecraft/profile` answer: the
/// `ACTIVE` skin and cape (`MinecraftProfileResponse`). Unsigned, like the
/// local player's own textures, and only URLs under the allowed domains count.
pub fn services_profile_textures(profile: &serde_json::Value) -> Textures {
    let active = |key: &str| {
        profile
            .get(key)?
            .as_array()?
            .iter()
            .find(|t| t.get("state").and_then(|s| s.as_str()) == Some("ACTIVE"))
    };
    let skin_entry = active("skins");
    let skin = skin_entry
        .and_then(|t| t.get("url"))
        .and_then(|u| u.as_str())
        .filter(|url| {
            texture_uris()
                .iter()
                .any(|u| url.starts_with(&u.replace("{textureId}", "")))
        })
        .map(str::to_owned);
    let slim = skin_entry
        .and_then(|t| t.get("variant"))
        .and_then(|v| v.as_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("slim"));
    let cape = active("capes")
        .and_then(|t| t.get("url"))
        .and_then(|u| u.as_str())
        .filter(|url| {
            texture_uris()
                .iter()
                .any(|u| url.starts_with(&u.replace("{textureId}", "")))
        })
        .map(str::to_owned);
    Textures {
        skin,
        slim,
        cape,
        elytra: None,
        signed: false,
    }
}

/// `profile_skin_url` on a block entity's `profile` NBT (`SkullBlockEntity`):
/// the same `properties` list, held as a compound. A name-only profile (a
/// string tag) has no properties and no skin URL.
pub fn profile_skin_url_nbt(profile: &horizon_core::nbt::Tag) -> Option<String> {
    let properties = match profile.get("properties")? {
        horizon_core::nbt::Tag::List(list) => list,
        _ => return None,
    };
    let value = properties.iter().find_map(|p| {
        let name = nbt_text(p.get("name"))?;
        (name == "textures")
            .then(|| nbt_text(p.get("value")))
            .flatten()
    })?;
    unpack(&value, Signature::Unsigned, texture_uris()).skin
}

/// `ResolvableProfile.skinPatch().body()` of a skull's `profile` NBT: the
/// `texture` id that replaces its skin.
pub fn profile_texture_nbt(profile: &horizon_core::nbt::Tag) -> Option<String> {
    nbt_text(profile.get("texture"))
}

/// The profile's uuid as u128: `id` as an int array (block entity NBT) or a
/// uuid string (older and component forms). Used for `DefaultPlayerSkin.get`
/// when a head's skin is not downloaded yet.
pub fn profile_uuid_nbt(profile: &horizon_core::nbt::Tag) -> Option<u128> {
    match profile.get("id")? {
        horizon_core::nbt::Tag::IntArray(v) if v.len() == 4 => Some(
            v.iter()
                .fold(0u128, |a, &w| (a << 32) | u128::from(w as u32)),
        ),
        horizon_core::nbt::Tag::String(s) => {
            let digits: String = s.to_string_lossy().chars().filter(|c| *c != '-').collect();
            u128::from_str_radix(&digits, 16).ok()
        }
        _ => None,
    }
}

pub(crate) fn nbt_text(tag: Option<&horizon_core::nbt::Tag>) -> Option<String> {
    match tag {
        Some(horizon_core::nbt::Tag::String(s)) => Some(s.to_string_lossy()),
        _ => None,
    }
}

pub use crate::chunk::block_entity::NameOrId;

/// `GameProfile` as `fetchProfile` returns it: the account's id, name and
/// property map (its `textures` property is the skin).
#[derive(Clone, Debug)]
pub struct ResolvedProfile {
    pub id: u128,
    pub name: String,
    /// `(name, value, signature)` properties.
    pub properties: Vec<(String, String, Option<String>)>,
}
impl ResolvedProfile {
    /// `SessionService.getPackedTextures`: the `textures` property.
    pub fn textures(&self) -> Option<(String, Signature)> {
        self.properties
            .iter()
            .find(|(n, _, _)| n == "textures")
            .map(|(_, v, s)| {
                (
                    v.clone(),
                    if s.is_some() {
                        Signature::Signed
                    } else {
                        Signature::Unsigned
                    },
                )
            })
    }
}

/// Where a `Dynamic` profile's resolution stands (`resolveProfile`'s future).
#[derive(Clone, Debug)]
pub enum Resolve {
    Pending,
    /// The services' answer: `None` for no profile or no answer, which the
    /// cache holds like any result (`Optional.empty`).
    Done(Option<ResolvedProfile>),
}

/// `ProfileResolver.Cached`: resolves a `Dynamic` profile through the
/// services — `profileCacheByName` asks `profiles`/`getByName` for the id,
/// `profileCacheById` asks `session`/`getProfileById` for the profile —
/// at most 256 entries kept 10 minutes after their last use, fetched on
/// background threads.
pub struct Resolver {
    entries: std::collections::HashMap<NameOrId, Entry>,
    done: std::sync::mpsc::Receiver<(NameOrId, Option<ResolvedProfile>)>,
    send: std::sync::mpsc::Sender<(NameOrId, Option<ResolvedProfile>)>,
    /// Fetch threads still running; [`MAX_FETCHES`] bounds them.
    active: Arc<AtomicUsize>,
    /// New lookups left this minute; one comes back each second.
    lookups: u32,
    last_lookup_refill: std::time::Instant,
    last_sweep: std::time::Instant,
    heads: HeadProfiles,
}
struct Entry {
    touched: std::time::Instant,
    /// `None` while the fetch is in flight.
    resolved: Option<Option<ResolvedProfile>>,
}
/// `ProfileResolver.Cached`'s cache bounds.
const MAX_PROFILES: usize = 256;
const PROFILE_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// The most profile fetches that may run at once. Each is an OS thread and an HTTP request; a
/// mod calling `resolve_id` in a loop must not exhaust them.
const MAX_FETCHES: usize = 16;
/// The fetch slots a mod's lookups may use. The last one is kept for the game's own lookups
/// (`/fetchprofile`, skins), so a mod polling every tick cannot stall them.
const MOD_FETCHES: usize = MAX_FETCHES - 1;
/// New profile lookups that may start a minute. Mojang rate-limits by IP, so one mod must not be
/// able to spend the server's whole allowance at once; one slot comes back each second.
const LOOKUPS_PER_MINUTE: u32 = 60;

/// `StringUtil.isValidPlayerName` (the check `fetchByName` makes first).
fn valid_player_name(name: &str) -> bool {
    name.encode_utf16().count() <= 16 && name.encode_utf16().all(|c| c > 32 && c < 127)
}

/// `fetchByNameOrId`: `getByName` answers `{id, name}`; `getProfileById`
/// (`fetchProfile(id, requireSecure)`) answers `{id, name, properties}` —
/// asked `unsigned=false` so the `textures` property is signed.
fn resolve_profile(key: &NameOrId, doc: &serde_json::Value) -> Option<ResolvedProfile> {
    let id = match key {
        NameOrId::Id(id) => *id,
        NameOrId::Name(name) => {
            let url = crate::auth::endpoint(doc, "profiles", "getByName")?
                .replace("{name}", &name.to_lowercase());
            let body = http_get(&url)?;
            let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
            crate::auth::parse_uuid(v.get("id")?.as_str()?)?
        }
    };
    let url = crate::auth::endpoint(doc, "session", "getProfileById")?
        .replace("{profileId}", &format!("{id:032x}"));
    let separator = if url.contains('?') { '&' } else { '?' };
    let body = http_get(&format!("{url}{separator}unsigned=false"))?;
    let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
    Some(ResolvedProfile {
        id: crate::auth::parse_uuid(v.get("id")?.as_str()?)?,
        name: v.get("name")?.as_str()?.to_owned(),
        properties: v
            .get("properties")?
            .as_array()?
            .iter()
            .map(|p| {
                Some((
                    p.get("name")?.as_str()?.to_owned(),
                    p.get("value")?.as_str()?.to_owned(),
                    p.get("signature")
                        .and_then(|s| s.as_str())
                        .map(str::to_owned),
                ))
            })
            .collect::<Option<Vec<_>>>()?,
    })
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}
impl Resolver {
    pub fn new() -> Self {
        let (send, done) = std::sync::mpsc::channel();
        Self {
            entries: Default::default(),
            done,
            send,
            active: Arc::new(AtomicUsize::new(0)),
            lookups: LOOKUPS_PER_MINUTE,
            last_lookup_refill: std::time::Instant::now(),
            last_sweep: std::time::Instant::now(),
            heads: HeadProfiles::default(),
        }
    }
    /// Adds back the slots `elapsed` earns, capped at the minute's budget, and advances the
    /// timestamp by every whole second: after an idle period the budget must not refill again
    /// on each of the next calls.
    fn refill_lookup_budget(&mut self, elapsed: std::time::Duration) {
        let seconds = elapsed.as_secs();
        if seconds == 0 {
            return;
        }
        self.lookups = (self.lookups + seconds.min(u64::from(LOOKUPS_PER_MINUTE)) as u32)
            .min(LOOKUPS_PER_MINUTE);
        self.last_lookup_refill += std::time::Duration::from_secs(seconds);
    }
    /// Takes one of the minute's new-lookup slots, adding back one a second.
    fn take_lookup_budget(&mut self) -> bool {
        self.refill_lookup_budget(self.last_lookup_refill.elapsed());
        if self.lookups == 0 {
            return false;
        }
        self.lookups -= 1;
        true
    }
    /// Whether a new fetch may start: within the caller's worker cap, and for a mod within the
    /// minute's budget. Takes a budget slot when it answers true.
    fn may_start_fetch(&mut self, paced: bool) -> bool {
        let cap = if paced { MOD_FETCHES } else { MAX_FETCHES };
        if self.active.load(Ordering::Relaxed) >= cap {
            return false;
        }
        !paced || self.take_lookup_budget()
    }
    /// `CacheBuilder.expireAfterAccess` with `maximumSize`.
    fn sweep(&mut self) {
        if self.last_sweep.elapsed() < std::time::Duration::from_secs(30) {
            return;
        }
        self.last_sweep = std::time::Instant::now();
        self.entries
            .retain(|_, e| e.touched.elapsed() < PROFILE_TTL);
        while self.entries.len() > MAX_PROFILES
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.touched)
                .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest);
        }
    }
    /// How the resolution of `key` stands, starting it the first time. A mod's lookups share
    /// the minute's budget and leave the last fetch slot to the game; the game's own callers use
    /// [`Resolver::state_for_game`].
    pub fn state(&mut self, key: &NameOrId) -> Resolve {
        self.state_with(key, true)
    }
    /// As [`Resolver::state`], for the game's own lookups (`/fetchprofile`, skins): they skip
    /// the mod budget and may use the fetch slot mods leave free, so a mod polling every tick
    /// cannot stall them.
    pub fn state_for_game(&mut self, key: &NameOrId) -> Resolve {
        self.state_with(key, false)
    }
    fn state_with(&mut self, key: &NameOrId, paced: bool) -> Resolve {
        while let Ok((key, resolved)) = self.done.try_recv() {
            self.entries.insert(
                key,
                Entry {
                    touched: std::time::Instant::now(),
                    resolved: Some(resolved),
                },
            );
        }
        self.sweep();
        if let Some(entry) = self.entries.get_mut(key) {
            entry.touched = std::time::Instant::now();
            return match &entry.resolved {
                Some(resolved) => Resolve::Done(resolved.clone()),
                None => Resolve::Pending,
            };
        }
        // `fetchByName` refuses an invalid name without a fetch. Answer without
        // caching: distinct invalid names must not fill the cache between sweeps.
        if matches!(key, NameOrId::Name(name) if !valid_player_name(name)) {
            return Resolve::Done(None);
        }
        // At the cap, leave the key uncached and answer `Pending`: the caller polls, and a later
        // call starts the fetch once a thread is free. Mods also leave the last slot to the
        // game and spend the minute's budget; at either limit they answer `Pending` without
        // caching, so a later call retries.
        if !self.may_start_fetch(paced) {
            return Resolve::Pending;
        }
        let entry_key = key.clone();
        self.entries.insert(
            entry_key.clone(),
            Entry {
                touched: std::time::Instant::now(),
                resolved: None,
            },
        );
        self.active.fetch_add(1, Ordering::Relaxed);
        let active = Arc::clone(&self.active);
        let (key, send) = (key.clone(), self.send.clone());
        let spawned = std::thread::Builder::new()
            .name("profile-fetch".into())
            .spawn(move || {
                /// Releases the fetch slot when the thread ends, including on a panic.
                struct Fetch(Arc<AtomicUsize>);
                impl Drop for Fetch {
                    fn drop(&mut self) {
                        self.0.fetch_sub(1, Ordering::Relaxed);
                    }
                }
                let _fetch = Fetch(active);
                let resolved = crate::auth::discovery()
                    .ok()
                    .and_then(|doc| resolve_profile(&key, &doc));
                let _ = send.send((key, resolved));
            });
        if spawned.is_err() {
            // The OS refused a thread: release the slots and let a later call retry.
            self.active.fetch_sub(1, Ordering::Relaxed);
            if paced {
                self.lookups = (self.lookups + 1).min(LOOKUPS_PER_MINUTE);
            }
            self.entries.remove(&entry_key);
        }
        Resolve::Pending
    }
}

/// `ResolvableProfile` (an item's `minecraft:profile` component, a skull's
/// `profile` tag): the `textures` property it carries, the key a `Dynamic`
/// profile resolves by, and `partialProfile.id` — the fallback skin's key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileInfo {
    /// `GameProfile.id` (`DefaultPlayerSkin.get`).
    pub id: u128,
    /// The `textures` property's value and signature state, when present.
    pub textures: Option<(String, Signature)>,
    /// `Dynamic` profiles resolve by name or id.
    pub dynamic: Option<NameOrId>,
}

/// A profile's `id`: `UUIDUtil.CODEC` (an int array of four, or a UUID
/// string).
fn profile_id(v: &serde_json::Value) -> Option<u128> {
    match v {
        serde_json::Value::Array(a) => {
            let ints: Option<Vec<i64>> = a.iter().map(|x| x.as_i64()).collect();
            let i = ints?;
            (i.len() == 4).then(|| {
                ((i[0] as u32 as u128) << 96)
                    | ((i[1] as u32 as u128) << 64)
                    | ((i[2] as u32 as u128) << 32)
                    | i[3] as u32 as u128
            })
        }
        serde_json::Value::String(s) => crate::auth::parse_uuid(s),
        _ => None,
    }
}

/// `ResolvableProfile`'s read of a `profile` value: a bare name is
/// `createUnresolved` (the `PLAYER_NAME` codec's alternative), a compound
/// `name` + `id` + `properties` (`Partial.MAP_CODEC` or a `GameProfile`).
pub fn parse_profile(v: &serde_json::Value) -> ProfileInfo {
    if let serde_json::Value::String(name) = v {
        return ProfileInfo {
            id: horizon_protocol::crypto::offline_uuid(name),
            textures: None,
            dynamic: valid_player_name(name).then(|| NameOrId::Name(name.clone())),
        };
    }
    let name = v.get("name").and_then(|n| n.as_str()).map(str::to_owned);
    let id = v.get("id").and_then(profile_id);
    let properties = v.get("properties").and_then(|p| p.as_array());
    let textures = properties
        .into_iter()
        .flatten()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures"))
        .and_then(|p| {
            Some((
                p.get("value")?.as_str()?.to_owned(),
                if p.get("signature").is_some() {
                    Signature::Signed
                } else {
                    Signature::Unsigned
                },
            ))
        });
    // `ResolvableProfile.create`: a Partial is Dynamic with no properties
    // and exactly one of a name or an id.
    let dynamic =
        (properties.is_none_or(|p| p.is_empty()) && name.is_some() != id.is_some()).then(|| {
            name.as_deref().map_or_else(
                || NameOrId::Id(id.unwrap()),
                |n| NameOrId::Name(n.to_owned()),
            )
        });
    ProfileInfo {
        // `createPartialProfile`: the id, else the name's offline UUID, else nil.
        id: id
            .or_else(|| name.as_deref().map(horizon_protocol::crypto::offline_uuid))
            .unwrap_or(0),
        textures,
        dynamic,
    }
}

/// `ResolvableProfile.create`'s `Dynamic` key from a skull's `profile` tag:
/// a bare name string is `createUnresolved`, and a compound without
/// properties and with exactly one of a name or an id is `Dynamic`.
pub fn dynamic_nbt(profile: &horizon_core::nbt::Tag) -> Option<NameOrId> {
    if let horizon_core::nbt::Tag::String(s) = profile {
        let name = s.to_string_lossy();
        return valid_player_name(&name).then_some(NameOrId::Name(name));
    }
    if matches!(profile.get("properties"), Some(horizon_core::nbt::Tag::List(l)) if !l.is_empty()) {
        return None;
    }
    match (nbt_text(profile.get("name")), profile_uuid_nbt(profile)) {
        (Some(name), None) => Some(NameOrId::Name(name)),
        (None, Some(id)) => Some(NameOrId::Id(id)),
        _ => None,
    }
}

const MAX_HEAD_PROFILES: usize = 4096;

#[derive(Debug, PartialEq)]
struct HeadProfile {
    info: ProfileInfo,
    skin: Option<(String, String)>,
}

fn prepare_head(profile: &serde_json::Value) -> HeadProfile {
    let info = parse_profile(profile);
    let skin = info.textures.as_ref().and_then(|(value, signature)| {
        let url = unpack(value, *signature, texture_uris()).skin?;
        let location = skin_location(&texture_hash(&url)?);
        Some((url, location))
    });
    HeadProfile { info, skin }
}

#[derive(Default)]
struct HeadProfiles {
    entries: std::collections::HashMap<serde_json::Value, std::sync::Arc<HeadProfile>>,
    nbt: std::collections::HashMap<horizon_core::nbt::Tag, std::sync::Arc<serde_json::Value>>,
    resolved: [std::collections::HashMap<String, std::sync::Arc<HeadProfile>>; 2],
}

impl HeadProfiles {
    fn get(&mut self, profile: &serde_json::Value) -> std::sync::Arc<HeadProfile> {
        if let Some(parsed) = self.entries.get(profile) {
            return parsed.clone();
        }
        if self.entries.len() >= MAX_HEAD_PROFILES {
            self.entries.clear();
        }
        let parsed = std::sync::Arc::new(prepare_head(profile));
        self.entries.insert(profile.clone(), parsed.clone());
        parsed
    }
}

impl Resolver {
    /// Cache by NBT value rather than address: equipment can be replaced or
    /// mutated in place, and neither must leave a stale profile behind.
    pub fn head_profile_nbt(
        &mut self,
        profile: &horizon_core::nbt::Tag,
    ) -> std::sync::Arc<serde_json::Value> {
        if let Some(json) = self.heads.nbt.get(profile) {
            return json.clone();
        }
        if self.heads.nbt.len() >= MAX_HEAD_PROFILES {
            self.heads.nbt.clear();
        }
        let json = std::sync::Arc::new(crate::scoreboard::tag_to_json(profile));
        self.heads.nbt.insert(profile.clone(), json.clone());
        json
    }
}

/// `MinecraftProfileTexture.getHash`: the base name of the URL's path.
pub fn texture_hash(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let path = &rest[rest.find('/').unwrap_or(rest.len())..];
    let path = &path[..path.find(['?', '#']).unwrap_or(path.len())];
    let name = &path[path.rfind(['/', '\\']).map_or(0, |i| i + 1)..];
    Some(name[..name.rfind('.').unwrap_or(name.len())].to_owned())
}

/// `SkinManager.TextureCache.registerTexture`: the texture id of a skin,
/// `skins/` and the SHA-1 of its hash's UTF-16 code units.
pub fn skin_location(hash: &str) -> String {
    location("skins/", hash)
}
/// `registerTexture` in the cape cache: `capes/` and the same SHA-1.
pub fn cape_location(hash: &str) -> String {
    location("capes/", hash)
}
fn location(prefix: &str, hash: &str) -> String {
    // Remembered: every frame resolves every drawn player's skin, and the SHA-1 and its hex
    // each time were most of that.
    if hash.len() > SKIN_MEMO_BYTES {
        return location_of(prefix, hash);
    }
    let key = (prefix.to_owned(), hash.to_owned());
    if let Some(known) = LOCATIONS.with_borrow(|known| known.get(&key).cloned()) {
        return known;
    }
    let out = location_of(prefix, hash);
    LOCATIONS.with_borrow_mut(|known| {
        let cached = out.clone();
        let bytes = key.0.capacity() + key.1.capacity() + cached.capacity();
        known.remember(key, cached, bytes);
    });
    out
}
fn location_of(prefix: &str, hash: &str) -> String {
    let units: Vec<u8> = hash.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let digest = sha1(&units);
    let mut out = String::from(prefix);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// `PlayerSkin.cape`: the cape texture of textures whose lookup gave a skin
/// (every download ready), if they name one.
pub fn cape(textures: &Textures) -> Option<String> {
    textures
        .cape
        .as_deref()
        .and_then(texture_hash)
        .map(|hash| cape_location(&hash))
}

/// The signed-in account's own profile properties (`Minecraft.profileFuture`:
/// `fetchProfile(id, true)`, so its `textures` property is signed), or empty
/// without an answer. Blocks on the network.
pub fn fetch_own_properties(id: u128) -> Vec<(String, String, Option<String>)> {
    crate::auth::discovery()
        .ok()
        .and_then(|doc| resolve_profile(&NameOrId::Id(id), &doc))
        .map(|p| p.properties)
        .unwrap_or_default()
}

/// A GET over HTTP or HTTPS: the body of a 2xx answer, none otherwise.
fn http_get(url: &str) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    std::io::Read::read_to_end(
        &mut crate::auth::agent().get(url).call().ok()?.into_reader(),
        &mut body,
    )
    .ok()?;
    Some(body)
}
/// SHA-1 (FIPS 180-4).
pub fn sha1(data: &[u8]) -> [u8; 20] {
    use sha1::{Digest, Sha1};
    Sha1::digest(data).into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_invalid_name_is_answered_without_being_cached() {
        let mut resolver = super::Resolver::new();
        let key = super::NameOrId::Name("not a name!".into());
        assert!(matches!(resolver.state(&key), super::Resolve::Done(None)));
        assert!(resolver.entries.is_empty());
    }

    #[test]
    fn the_fetch_cap_answers_pending_without_caching() {
        let mut resolver = super::Resolver::new();
        resolver
            .active
            .store(super::MAX_FETCHES, std::sync::atomic::Ordering::Relaxed);
        let key = super::NameOrId::Id(7);
        assert!(matches!(resolver.state(&key), super::Resolve::Pending));
        assert!(!resolver.entries.contains_key(&key));
        resolver
            .active
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    #[test]
    fn the_lookup_budget_answers_pending_without_caching() {
        let mut resolver = super::Resolver::new();
        resolver.lookups = 0;
        resolver.last_lookup_refill = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let key = super::NameOrId::Id(7);
        assert!(matches!(resolver.state(&key), super::Resolve::Pending));
        assert!(!resolver.entries.contains_key(&key));
    }

    #[test]
    fn new_lookups_are_paced_per_minute() {
        let mut resolver = super::Resolver::new();
        resolver.last_lookup_refill =
            std::time::Instant::now() + std::time::Duration::from_secs(60);
        for _ in 0..super::LOOKUPS_PER_MINUTE {
            assert!(resolver.take_lookup_budget());
        }
        assert!(!resolver.take_lookup_budget());
        resolver.last_lookup_refill = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(2))
            .expect("the process has run for two seconds");
        assert!(resolver.take_lookup_budget());
        assert!(resolver.take_lookup_budget());
        assert!(!resolver.take_lookup_budget());
    }

    #[test]
    fn an_idle_period_refills_the_budget_once() {
        let mut resolver = super::Resolver::new();
        resolver.lookups = 0;
        let before = resolver.last_lookup_refill;
        resolver.refill_lookup_budget(std::time::Duration::from_secs(3600));
        assert_eq!(resolver.lookups, super::LOOKUPS_PER_MINUTE);
        assert_eq!(
            resolver.last_lookup_refill.duration_since(before),
            std::time::Duration::from_secs(3600),
            "the whole idle period is consumed, not just the minute the budget caps at"
        );
        for _ in 0..super::LOOKUPS_PER_MINUTE {
            assert!(resolver.take_lookup_budget());
        }
        assert!(!resolver.take_lookup_budget());
    }

    #[test]
    fn a_game_lookup_keeps_a_fetch_slot_and_skips_the_mod_budget() {
        let mut resolver = super::Resolver::new();
        resolver
            .active
            .store(super::MOD_FETCHES, std::sync::atomic::Ordering::Relaxed);
        resolver.lookups = 0;
        resolver.last_lookup_refill = std::time::Instant::now() + std::time::Duration::from_secs(1);
        assert!(
            !resolver.may_start_fetch(true),
            "mods leave the last fetch slot to the game"
        );
        assert!(
            resolver.may_start_fetch(false),
            "the game skips the mod budget and may use the last slot"
        );
        resolver
            .active
            .store(super::MAX_FETCHES, std::sync::atomic::Ordering::Relaxed);
        assert!(!resolver.may_start_fetch(false));
    }

    #[test]
    fn head_profile_cache_matches_uncached_and_is_bounded() {
        use base64::Engine;
        let mut cache = super::HeadProfiles::default();
        for (url, slim) in [
            ("https://textures.minecraft.net/texture/abc", false),
            ("https://textures.minecraft.net/texture/abc", true),
            ("https://evil.example/texture/abc", false),
        ] {
            let payload = serde_json::json!({"textures": {
                "SKIN": {"url": url, "metadata": {"model": if slim { "slim" } else { "classic" }}},
                "CAPE": {"url": "http://textures.minecraft.net/texture/cape"}
            }});
            let value = base64::engine::general_purpose::STANDARD.encode(payload.to_string());
            for signed in [false, true] {
                let mut property = serde_json::json!({"name": "textures", "value": value});
                if signed {
                    property["signature"] = serde_json::Value::Null;
                }
                let profile = serde_json::json!({"properties": [property]});
                let parsed = cache.get(&profile);
                assert_eq!(*parsed, super::prepare_head(&profile));
                let textures = super::unpack(
                    &value,
                    if signed {
                        super::Signature::Signed
                    } else {
                        super::Signature::Unsigned
                    },
                    &super::TEXTURE_URIS,
                );
                assert_eq!(textures.slim, slim && !url.contains("evil"));
                // Authlib rejects the whole payload if any URL is invalid,
                // including the allowed cape when the skin is disallowed.
                assert_eq!(textures.cape.is_some(), !url.contains("evil"));
                assert_eq!(parsed.skin.is_some(), !url.contains("evil"));
                assert!(std::sync::Arc::ptr_eq(&parsed, &cache.get(&profile)));
            }
        }
        for profile in [
            serde_json::json!({"properties": [{"name": "textures", "value": "!bad"}]}),
            serde_json::json!({"name": "Notch"}),
            serde_json::json!({}),
        ] {
            let parsed = cache.get(&profile);
            assert_eq!(*parsed, super::prepare_head(&profile));
            assert!(parsed.skin.is_none());
            assert!(std::sync::Arc::ptr_eq(&parsed, &cache.get(&profile)));
        }
        cache.entries.clear();
        for i in 0..super::MAX_HEAD_PROFILES {
            cache.get(&serde_json::json!({"id": i}));
        }
        assert_eq!(cache.entries.len(), super::MAX_HEAD_PROFILES);
        cache.get(&serde_json::json!({}));
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn head_profile_nbt_cache_tracks_value_changes() {
        let mut resolver = super::Resolver::new();
        let mut tag = horizon_core::nbt::Tag::String("Notch".into());
        let first = resolver.head_profile_nbt(&tag);
        assert_eq!(*first, crate::scoreboard::tag_to_json(&tag));
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &resolver.head_profile_nbt(&tag)
        ));
        tag = horizon_core::nbt::Tag::String("Alex".into());
        assert_eq!(*resolver.head_profile_nbt(&tag), serde_json::json!("Alex"));
        for i in 0..super::MAX_HEAD_PROFILES {
            resolver.head_profile_nbt(&horizon_core::nbt::Tag::Int(i as i32));
        }
        assert!(resolver.heads.nbt.len() < super::MAX_HEAD_PROFILES);
    }

    /// A profile's `texture` names a pack texture, never a file outside the
    /// packs: bad characters and `..` segments draw nothing.
    #[test]
    fn cached_skins_keep_payload_signature_and_location() {
        use super::{
            Signature, Textures, cape_location, skin_location, texture_uris, unpack_known,
        };
        use base64::Engine;
        let url = texture_uris()[0].replace("{textureId}", "");
        let skin_url = format!("{url}skin");
        let cape_url = format!("{url}cape");
        let elytra_url = format!("{url}elytra");
        let json = serde_json::json!({"textures": {
            "SKIN": {"url": skin_url, "metadata": {"model": "slim"}},
            "CAPE": {"url": cape_url},
            "ELYTRA": {"url": elytra_url},
        }});
        let value = base64::engine::general_purpose::STANDARD.encode(json.to_string());
        let mut expected = Textures {
            skin: Some(skin_url),
            slim: true,
            cape: Some(cape_url),
            elytra: Some(elytra_url),
            signed: false,
        };
        let hash = "abc";
        let skin = super::location_of("skins/", hash);
        let cape = super::location_of("capes/", hash);
        for _ in 0..2 {
            for signature in [Signature::Unsigned, Signature::Signed, Signature::Invalid] {
                expected.signed = signature == Signature::Signed;
                assert_eq!(unpack_known(&value, signature), expected);
                assert_eq!(unpack_known("not base64!", signature), Textures::default());
            }
            assert_eq!(skin_location(hash), skin);
            assert_eq!(cape_location(hash), cape);
        }
    }

    #[test]
    fn cached_skins_bound_obsolete_string_allocations() {
        use super::{LOCATIONS, SKIN_MEMO_BYTES, Signature, Textures, UNPACKED};
        UNPACKED.with_borrow_mut(|memo| *memo = Default::default());
        LOCATIONS.with_borrow_mut(|memo| *memo = Default::default());
        // Each rejected payload fits Property's 32767 UTF-16-unit network limit.
        let padding = "\u{0800}".repeat(32759);
        let mut first = String::new();
        for i in 0..64 {
            let value = format!("{i:08}{padding}");
            if i == 0 {
                first = value.clone();
            }
            assert_eq!(
                super::unpack_known(&value, Signature::Unsigned),
                Textures::default()
            );
            assert_eq!(
                super::skin_location(&value),
                super::location_of("skins/", &value)
            );
        }
        UNPACKED.with_borrow(|memo| {
            let bytes: usize = memo.values.keys().map(|(value, _)| value.capacity()).sum();
            assert!(
                bytes <= SKIN_MEMO_BYTES,
                "rejected payloads retained {bytes} bytes"
            );
            assert!(
                memo.get(&(first.clone(), Signature::Unsigned as u8))
                    .is_none()
            );
        });
        LOCATIONS.with_borrow(|memo| {
            let bytes: usize = memo
                .values
                .iter()
                .map(|((prefix, hash), (location, _))| {
                    prefix.capacity() + hash.capacity() + location.capacity()
                })
                .sum();
            assert!(bytes <= SKIN_MEMO_BYTES, "locations retained {bytes} bytes");
            assert!(memo.get(&("skins/".to_owned(), first)).is_none());
        });
        // Bypassing retention is not rejecting input or changing its result.
        let huge = "!".repeat(SKIN_MEMO_BYTES + 1);
        assert_eq!(
            super::unpack_known(&huge, Signature::Unsigned),
            Textures::default()
        );
        assert_eq!(
            super::skin_location(&huge),
            super::location_of("skins/", &huge)
        );
    }

    // Set HORIZON_MEASURE_SKINS=1 and run this test with --nocapture to print the timings.
    #[test]
    fn measure_repeated_skin_lookups() {
        if std::env::var("HORIZON_MEASURE_SKINS").as_deref() != Ok("1") {
            return;
        }
        use super::{Signature, location_of, skin_location, texture_uris, unpack, unpack_known};
        use base64::Engine;
        use std::{hint::black_box, time::Instant};

        const RUNS: usize = 21;
        const CALLS: usize = 256;
        fn measure<T>(mut call: impl FnMut() -> T) -> u128 {
            let start = Instant::now();
            for _ in 0..CALLS {
                black_box(call());
            }
            start.elapsed().as_nanos() / CALLS as u128
        }

        let hash = "abcdef0123456789";
        let uri = texture_uris()[0].replace("{textureId}", "");
        let payload = serde_json::json!({"textures":{"SKIN":{
            "url":format!("{uri}{hash}"), "metadata":{"model":"slim"}
        }}});
        let value = base64::engine::general_purpose::STANDARD.encode(payload.to_string());
        let decode = || measure(|| unpack(black_box(&value), Signature::Unsigned, texture_uris()));
        let recall = || measure(|| unpack_known(black_box(&value), Signature::Unsigned));
        let compute = || measure(|| location_of("skins/", black_box(hash)));
        let lookup = || measure(|| skin_location(black_box(hash)));
        for _ in 0..2 {
            decode();
            recall();
            compute();
            lookup();
        }
        let mut samples = [[0; RUNS]; 4];
        for run in 0..RUNS {
            let (decode_ns, recall_ns, compute_ns, lookup_ns) = if run % 2 == 0 {
                (decode(), recall(), compute(), lookup())
            } else {
                let (lookup_ns, compute_ns, recall_ns, decode_ns) =
                    (lookup(), compute(), recall(), decode());
                (decode_ns, recall_ns, compute_ns, lookup_ns)
            };
            for (series, nanos) in samples
                .iter_mut()
                .zip([decode_ns, recall_ns, compute_ns, lookup_ns])
            {
                series[run] = nanos;
            }
        }
        println!("one textures payload and skin hash, {RUNS} runs of {CALLS} calls");
        for (name, mut series) in ["unpack", "unpack_cached", "location", "location_cached"]
            .into_iter()
            .zip(samples)
        {
            series.sort_unstable();
            let p95 = (RUNS * 95).div_ceil(100) - 1;
            println!(
                "{name}_ns median={} p95={} worst={}",
                series[RUNS / 2],
                series[p95],
                series[RUNS - 1]
            );
        }
    }

    /// `MinecraftServicesDiscoveryService`: another environment's document
    /// names its own texture host; one that lists none names nothing.
    #[test]
    fn texture_uris_come_from_the_discovery_document() {
        use super::{Signature, discovery_texture_uris, unpack};
        let doc = serde_json::json!({"discovery": {"profiles": {"endpoints": {"getTexture": {
            "validUris": ["https://skins.example.org/t/{textureId}"]
        }}}}});
        let uris = discovery_texture_uris(&doc).unwrap();
        assert_eq!(uris, ["https://skins.example.org/t/{textureId}"]);
        // {"textures":{"SKIN":{"url":"https://skins.example.org/t/abc"}}}
        let ours =
            "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHBzOi8vc2tpbnMuZXhhbXBsZS5vcmcvdC9hYmMifX19";
        assert_eq!(
            unpack(ours, Signature::Signed, &uris).skin.as_deref(),
            Some("https://skins.example.org/t/abc")
        );
        assert_eq!(
            unpack(ours, Signature::Signed, &super::TEXTURE_URIS).skin,
            None
        );
        let none = serde_json::json!({"discovery": {"profiles": {"endpoints": {}}}});
        assert_eq!(discovery_texture_uris(&none), None);
        let empty = serde_json::json!({"discovery": {"profiles": {"endpoints": {"getTexture": {"validUris": []}}}}});
        assert_eq!(discovery_texture_uris(&empty), None);
    }

    /// `MinecraftProfileTexture.getMetadata("model")`: `slim` marks a slim
    /// skin; no metadata is the wide (classic) model.
    #[test]
    fn a_skins_model_comes_from_its_metadata() {
        use super::{Signature, TEXTURE_URIS, unpack};
        let slim = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL2FiYyIsIm1ldGFkYXRhIjp7Im1vZGVsIjoic2xpbSJ9fX19";
        let wide = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL2FiYyJ9fX0=";
        let t = unpack(slim, Signature::Signed, &TEXTURE_URIS);
        assert_eq!(
            t.skin.as_deref(),
            Some("https://textures.minecraft.net/texture/abc")
        );
        assert!(t.slim);
        assert!(!unpack(wide, Signature::Signed, &TEXTURE_URIS).slim);
    }
    /// Authlib's `MinecraftServicesSessionService` rejects the whole profile in
    /// `unpackTextures` when any texture URL is outside the allowed origins.
    #[test]
    fn unpack_rejects_the_skin_when_another_texture_is_rejected() {
        use super::{Signature, TEXTURE_URIS, unpack};
        let mixed = "eyJ0ZXh0dXJlcyI6IHsiU0tJTiI6IHsidXJsIjogImh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL3NraW4xIn0sICJDQVBFIjogeyJ1cmwiOiAiaHR0cHM6Ly9ldmlsLmV4YW1wbGUvdGV4dHVyZS9jYXBlMSJ9fX0=";
        let t = unpack(mixed, Signature::Unsigned, &TEXTURE_URIS);
        assert_eq!(t, super::Textures::default());
    }
    #[test]
    fn profiles_parse_like_resolvable_profiles() {
        use super::{NameOrId, Signature, parse_profile};
        // A bare name is `createUnresolved`: dynamic by name, and its partial
        // id is the name's offline UUID.
        let p = parse_profile(&serde_json::json!("Notch"));
        assert_eq!(p.dynamic, Some(NameOrId::Name("Notch".into())));
        assert_eq!(p.id, horizon_protocol::crypto::offline_uuid("Notch"));
        assert!(p.textures.is_none());
        // A name-only compound resolves the same way.
        let p = parse_profile(&serde_json::json!({"name": "Notch"}));
        assert_eq!(p.dynamic, Some(NameOrId::Name("Notch".into())));
        // An id-only compound resolves by id (int array or UUID string).
        let id = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef_u128;
        let ints: Vec<i64> = [
            (id >> 96) as i32 as i64,
            (id >> 64) as i32 as i64,
            (id >> 32) as i32 as i64,
            id as i32 as i64,
        ]
        .into();
        let p = parse_profile(&serde_json::json!({"id": ints}));
        assert_eq!(p.dynamic, Some(NameOrId::Id(id)));
        let p = parse_profile(&serde_json::json!({"id": "01234567-89ab-cdef-0123-4567abcdef00"}));
        assert_eq!(
            p.dynamic,
            Some(NameOrId::Id(0x0123_4567_89ab_cdef_0123_4567_abcd_ef00))
        );
        // Name and id but no properties is Static: it never resolves, and its
        // default skin keys off the id.
        let p = parse_profile(&serde_json::json!({"name": "Notch", "id": ints}));
        assert!(p.dynamic.is_none());
        assert_eq!(p.id, id);
        // A `textures` property makes it Static (unsigned and signed).
        let p = parse_profile(&serde_json::json!({
            "properties": [{"name": "textures", "value": "v"}]
        }));
        assert!(p.dynamic.is_none());
        assert_eq!(p.textures, Some(("v".into(), Signature::Unsigned)));
        let p = parse_profile(&serde_json::json!({
            "properties": [{"name": "textures", "value": "v", "signature": "s"}]
        }));
        assert_eq!(p.textures, Some(("v".into(), Signature::Signed)));
        // Empty properties leave it dynamic.
        let p = parse_profile(&serde_json::json!({"name": "Notch", "properties": []}));
        assert_eq!(p.dynamic, Some(NameOrId::Name("Notch".into())));
    }

    #[test]
    fn services_profile_answers_give_the_active_skin_and_cape() {
        let profile = serde_json::json!({
            "skins": [
                {"state": "INACTIVE", "url": "https://textures.minecraft.net/texture/old"},
                {"state": "ACTIVE", "url": "https://textures.minecraft.net/texture/skin1", "variant": "CLASSIC"}
            ],
            "capes": [{"state": "ACTIVE", "url": "http://textures.minecraft.net/texture/cape1"}]
        });
        let t = super::services_profile_textures(&profile);
        assert_eq!(
            t.skin.as_deref(),
            Some("https://textures.minecraft.net/texture/skin1")
        );
        assert_eq!(
            t.cape.as_deref(),
            Some("http://textures.minecraft.net/texture/cape1")
        );
        assert!(!t.signed);
        let elsewhere = serde_json::json!({"skins": [{"state": "ACTIVE", "url": "https://evil.example/texture/x"}]});
        assert_eq!(
            super::services_profile_textures(&elsewhere),
            super::Textures::default()
        );
        assert_eq!(
            super::services_profile_textures(&serde_json::json!({})),
            super::Textures::default()
        );
    }

    /// `getByName` and `getProfileById` answer from the services the
    /// discovery document names; a profile resolves through both.
    #[test]
    #[ignore = "daily"]
    fn profiles_resolve_through_the_services() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let reply = |status: &str, body: &str| {
                format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            for stream in listener.incoming().take(3) {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                reader.read_line(&mut line).unwrap();
                while reader.read_line(&mut String::new()).unwrap() > 2 {}
                let body = if line.contains("/users/profiles/minecraft/") {
                    r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#
                } else if line.contains("/session/minecraft/profile/")
                    && line.contains("unsigned=false")
                {
                    r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[{"name":"textures","value":"dmFsdWU=","signature":"c2ln"}]}"#
                } else {
                    "{}"
                };
                stream.write_all(reply("200 OK", body).as_bytes()).unwrap();
            }
        });
        let doc = serde_json::json!({"discovery": {
            "profiles": {"endpoints": {"getByName": {"uri": format!("{base}/users/profiles/minecraft/{{name}}")}}},
            "session": {"endpoints": {"getProfileById": {"uri": format!("{base}/session/minecraft/profile/{{profileId}}")}}},
        }});
        let resolved =
            super::resolve_profile(&super::NameOrId::Name("Notch".into()), &doc).unwrap();
        assert_eq!(resolved.id, 0x069a_79f4_44e9_4726_a5be_fca9_0e38_aaf5);
        assert_eq!(resolved.name, "Notch");
        assert_eq!(
            resolved.properties,
            vec![(
                "textures".to_owned(),
                "dmFsdWU=".to_owned(),
                Some("c2ln".to_owned())
            )]
        );
        let by_id = super::resolve_profile(&super::NameOrId::Id(resolved.id), &doc).unwrap();
        assert_eq!(by_id.name, "Notch");
        // A discovery without `profiles`/`getByName` resolves no name.
        let doc = serde_json::json!({"discovery": {
            "session": {"endpoints": {"getProfileById": {"uri": format!("{base}/session/minecraft/profile/{{profileId}}")}}},
        }});
        assert!(super::resolve_profile(&super::NameOrId::Name("Notch".into()), &doc).is_none());
    }

    /// More skins drawn every frame than there are slots (#3829): a frame's
    /// uploads used to push out skins drawn the frame before, which then
    /// uploaded again and pushed out others, so the drawn skins went missing
    /// in turn. Those drawn keep their slots; the rest wait.
    #[test]
    fn profile_components_supply_unsigned_head_skins() {
        let value = "eyJ0ZXh0dXJlcyI6IHsiU0tJTiI6IHsidXJsIjogImh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL2FiYzEyMyJ9fX0=";
        let profile = serde_json::json!({"properties": [{"name": "textures", "value": value}]});
        assert_eq!(
            super::profile_skin_url(&profile).as_deref(),
            Some("https://textures.minecraft.net/texture/abc123")
        );
        let other = serde_json::json!({"properties": [{"name": "other", "value": value}]});
        assert_eq!(super::profile_skin_url(&other), None);
        let bad = serde_json::json!({"properties": [{"name": "textures", "value": "not base64"}]});
        assert_eq!(super::profile_skin_url(&bad), None);
        assert_eq!(super::profile_skin_url(&serde_json::json!({})), None);
    }

    /// Textures come over plain HTTP too (a local server here; https is the
    /// same request through rustls), and only 2xx answers count.
    #[test]
    #[ignore = "daily"]
    fn http_get_reads_2xx_bodies() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                reader.read_line(&mut line).unwrap();
                while reader.read_line(&mut String::new()).unwrap() > 2 {}
                let reply: &[u8] = if line.contains("/texture/abc") {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nPNG!"
                } else {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                };
                stream.write_all(reply).unwrap();
            }
        });
        assert_eq!(
            http_get(&format!("{base}/texture/abc")),
            Some(b"PNG!".to_vec())
        );
        assert_eq!(http_get(&format!("{base}/texture/missing")), None);
    }

    use super::*;

    #[test]
    fn skull_block_entity_profiles_supply_head_skins() {
        let value = "eyJ0ZXh0dXJlcyI6IHsiU0tJTiI6IHsidXJsIjogImh0dHBzOi8vdGV4dHVyZXMubWluZWNyYWZ0Lm5ldC90ZXh0dXJlL2FiYzEyMyJ9fX0=";
        let profile = horizon_core::nbt::parse_snbt(&format!(
            r#"{{name:"T",id:[I;-158327916,1572358581,-2125825222,-487948715],properties:[{{name:"textures",value:"{value}"}}]}}"#
        ))
        .unwrap()
        .0;
        assert_eq!(
            super::profile_skin_url_nbt(&profile).as_deref(),
            Some("https://textures.minecraft.net/texture/abc123")
        );
        assert_eq!(
            super::profile_uuid_nbt(&profile),
            Some(
                [-158327916, 1572358581, -2125825222, -487948715]
                    .iter()
                    .fold(0u128, |a, &w: &i32| (a << 32) | u128::from(w as u32))
            )
        );
        // A name-only profile (a string tag) has no properties: no url, no id.
        let named = horizon_core::nbt::Tag::String("MHF_Blaze".into());
        assert_eq!(super::profile_skin_url_nbt(&named), None);
        assert_eq!(super::profile_uuid_nbt(&named), None);
        // A uuid in string form decodes too.
        let text_id = horizon_core::nbt::parse_snbt(
            r#"{name:"T",id:"e843a1f4-0000-0001-0000-000200000003"}"#,
        )
        .unwrap()
        .0;
        assert_eq!(
            super::profile_uuid_nbt(&text_id),
            Some(u128::from_str_radix("e843a1f4000000010000000200000003", 16).unwrap())
        );
    }

    #[test]
    fn sha1_and_base64() {
        let hex: String = sha1(b"abc").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(base64_decode("aGk=").unwrap(), b"hi");
        assert_eq!(base64_decode("aGk").unwrap(), b"hi");
        assert!(base64_decode("a").is_none());
        assert!(base64_decode("aGk=x").is_none());
        assert_eq!(
            texture_hash("http://textures.minecraft.net/texture/abc.png?x=1").unwrap(),
            "abc"
        );
    }
}
