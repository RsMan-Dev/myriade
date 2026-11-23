//! Player profiles resolved through the services (`getByName` and
//! `getProfileById` of the discovery document), kept like vanilla's
//! `ProfileResolver.Cached`.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

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
    fn base64_decode_takes_padded_and_bare_text() {
        assert_eq!(super::base64_decode("aGk=").unwrap(), b"hi");
        assert_eq!(super::base64_decode("aGk").unwrap(), b"hi");
        assert!(super::base64_decode("a").is_none());
        assert!(super::base64_decode("aGk=x").is_none());
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
            super::http_get(&format!("{base}/texture/abc")),
            Some(b"PNG!".to_vec())
        );
        assert_eq!(super::http_get(&format!("{base}/texture/missing")), None);
    }
}
