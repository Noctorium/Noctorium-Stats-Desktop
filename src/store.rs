//! What is kept between runs: the sign-in, a few settings, and the figures last fetched.
//!
//! The sign-in is a token that opens the account for ninety days, so it goes into the system's own store
//! for secrets -- Credential Manager, the Keychain, the Secret Service. Not every Linux desktop runs one,
//! and a box without one is still somebody's computer, so when the store cannot be reached the token goes
//! into a file only this user can read, and the window says so.
//!
//! The figures are cached per range, so the window opens on something straight away and has something to
//! show when the service cannot be reached. Signing out removes them, with the token.

use crate::api::{Session, User};
use crate::stats::{Range, Stats};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The keyring's name for this program's entries. Each entry is named for the service it signs in to, so a
/// sign-in to a copy of the service on this machine never replaces the real one.
const KEYRING_SERVICE: &str = "Noctorium Stats";

/// Where things are kept.
#[derive(Debug, Clone)]
pub struct Places {
    pub config: PathBuf,
    pub cache: PathBuf,
    /// Whether the system's store for secrets is used at all. Not for the tests, which must never write
    /// into somebody's real Credential Manager, nor under NOCTORIUM_STATS_HOME.
    pub keyring: bool,
}

impl Places {
    /// Each system's own places: `%APPDATA%` and `%LOCALAPPDATA%` on Windows, Application Support and
    /// Caches on a Mac, `~/.config` and `~/.cache` on Linux. `NOCTORIUM_STATS_HOME` puts everything in one
    /// folder instead, the sign-in included, to try the program without touching any of them.
    pub fn system() -> Places {
        if let Some(home) = std::env::var_os("NOCTORIUM_STATS_HOME").filter(|h| !h.is_empty()) {
            return Places::at(PathBuf::from(home));
        }
        let name = if cfg!(target_os = "linux") {
            "noctorium-stats"
        } else {
            "Noctorium Stats"
        };
        let fallback = std::env::temp_dir();
        let config = dirs::config_dir()
            .unwrap_or_else(|| fallback.clone())
            .join(name);
        let cache = dirs::cache_dir().unwrap_or(fallback).join(name);
        Places {
            config,
            cache,
            keyring: true,
        }
    }

    /// Everything in one folder, with no keyring.
    pub fn at(folder: PathBuf) -> Places {
        Places {
            config: folder.clone(),
            cache: folder.join("cache"),
            keyring: false,
        }
    }

    fn token_file(&self) -> PathBuf {
        self.config.join("sign-in.json")
    }

    fn settings_file(&self) -> PathBuf {
        self.config.join("settings.json")
    }

    fn cache_file(&self, range: Range) -> PathBuf {
        self.cache.join(format!("stats-{}.json", range.code()))
    }
}

/// Where the token ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    Keyring,
    /// In this file, because the keyring could not be used; why, in words.
    File(PathBuf, String),
}

impl Kept {
    /// A line for the window, when the token is not where it would be expected.
    pub fn note(&self) -> Option<String> {
        match self {
            Kept::Keyring => None,
            Kept::File(path, why) => Some(format!(
                "Your sign-in is kept in {}, readable only by you, because the system's keyring could not be used ({why}).",
                path.display()
            )),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct TokenFile {
    service: String,
    token: String,
}

fn keyring_entry(service: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, service).map_err(|e| e.to_string())
}

/// The token saved for [service], from the keyring or the file.
pub fn load_token(places: &Places, service: &str) -> Option<(String, Kept)> {
    if places.keyring {
        if let Ok(entry) = keyring_entry(service) {
            if let Ok(token) = entry.get_password() {
                if !token.is_empty() {
                    return Some((token, Kept::Keyring));
                }
            }
        }
    }
    let path = places.token_file();
    let file: TokenFile = serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    (file.service == service && !file.token.is_empty()).then(|| {
        let why = if places.keyring {
            "it could not be reached when you signed in".to_string()
        } else {
            "NOCTORIUM_STATS_HOME is set".to_string()
        };
        (file.token, Kept::File(path, why))
    })
}

/// Saves the token for [service], in the keyring if it can be, in a file otherwise.
pub fn save_token(places: &Places, service: &str, token: &str) -> Result<Kept, String> {
    let why = if places.keyring {
        match keyring_entry(service).and_then(|e| e.set_password(token).map_err(|e| e.to_string()))
        {
            Ok(()) => {
                // A file left from a time the keyring was not working would otherwise outlive this sign-in.
                let _ = std::fs::remove_file(places.token_file());
                return Ok(Kept::Keyring);
            }
            Err(why) => why,
        }
    } else {
        "NOCTORIUM_STATS_HOME is set".to_string()
    };
    let path = places.token_file();
    let body = serde_json::to_string(&TokenFile {
        service: service.into(),
        token: token.into(),
    })
    .map_err(|e| e.to_string())?;
    write_private(&path, body.as_bytes())
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(Kept::File(path, why))
}

/// Forgets the token for [service] wherever it was kept, and the cached figures with it. Who was signed in
/// is the window's to forget, in its settings, which only it writes.
pub fn sign_out(places: &Places, service: &str) {
    if places.keyring {
        if let Ok(entry) = keyring_entry(service) {
            let _ = entry.delete_credential();
        }
    }
    let _ = std::fs::remove_file(places.token_file());
    for range in Range::ALL {
        let _ = std::fs::remove_file(places.cache_file(range));
    }
}

/// What the window remembers that is not a secret.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The range last looked at, which the window opens on.
    pub range: Range,
    /// The address last signed in with, so the form has it ready.
    pub email: String,
    /// Who is signed in, and to which service, so the window can name them before the service answers.
    pub user: Option<User>,
    pub service: String,
}

impl Settings {
    pub fn load(places: &Places) -> Settings {
        std::fs::read_to_string(places.settings_file())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, places: &Places) -> std::io::Result<()> {
        let body = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        write_replacing(&places.settings_file(), body.as_bytes())
    }

    /// Remembers who signed in.
    pub fn signed_in(&mut self, session: &Session, service: &str) {
        self.email = session.user.email.clone();
        self.user = Some(session.user.clone());
        self.service = service.to_string();
    }

    /// The user, if they were signed in to this same service.
    pub fn user_for(&self, service: &str) -> Option<&User> {
        self.user.as_ref().filter(|_| self.service == service)
    }
}

/// Figures as they were last fetched, and for whom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cached {
    pub service: String,
    pub user_id: u64,
    /// The clock they were counted on, in minutes east of UTC: figures counted for another time zone put
    /// the hours of the day in the wrong places, so they are not shown as if they were today's.
    pub tz: i32,
    pub fetched_at: DateTime<Utc>,
    pub stats: Stats,
}

pub fn load_cached(
    places: &Places,
    range: Range,
    service: &str,
    user_id: u64,
    tz: i32,
) -> Option<Cached> {
    let text = std::fs::read_to_string(places.cache_file(range)).ok()?;
    let cached: Cached = serde_json::from_str(&text).ok()?;
    (cached.service == service && cached.user_id == user_id && cached.tz == tz).then_some(cached)
}

pub fn save_cached(places: &Places, range: Range, cached: &Cached) -> std::io::Result<()> {
    let body = serde_json::to_string(cached).map_err(std::io::Error::other)?;
    write_replacing(&places.cache_file(range), body.as_bytes())
}

/// Written beside the file and then moved over it, so a crash part of the way leaves the old one whole.
fn write_replacing(path: &Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension("partial");
    std::fs::write(&partial, body)?;
    std::fs::rename(&partial, path)
}

/// As [write_replacing], readable by this user only. On Windows the folder under `%APPDATA%` is already the
/// user's own.
fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension("partial");
    let _ = std::fs::remove_file(&partial);
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&partial)?;
        file.write_all(body)?;
    }
    #[cfg(not(unix))]
    std::fs::write(&partial, body)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for each test, under the temporary folder, emptied first.
    fn places(name: &str) -> Places {
        let folder = std::env::temp_dir().join(format!(
            "noctorium-stats-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        Places::at(folder)
    }

    fn user() -> User {
        User {
            id: 7,
            email: "robin@example.test".into(),
            display_name: "Robin Vale".into(),
        }
    }

    fn stats() -> Stats {
        let path = format!(
            "{}/tests/fixtures/listener-7d.json",
            env!("CARGO_MANIFEST_DIR")
        );
        Stats::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn without_a_keyring_the_token_is_kept_in_a_file_and_said_so() {
        let places = places("token");
        assert_eq!(load_token(&places, "http://localhost:3000"), None);
        let kept = save_token(&places, "http://localhost:3000", "secret-token").unwrap();
        assert!(matches!(&kept, Kept::File(path, _) if path.ends_with("sign-in.json")));
        assert!(kept.note().unwrap().contains("readable only by you"));
        let (token, again) = load_token(&places, "http://localhost:3000").unwrap();
        assert_eq!(token, "secret-token");
        assert_eq!(again, kept);
        // A token for one service is not offered to another.
        assert_eq!(load_token(&places, api_live()), None);
    }

    fn api_live() -> &'static str {
        crate::api::LIVE
    }

    #[cfg(unix)]
    #[test]
    fn the_token_file_is_the_users_alone() {
        use std::os::unix::fs::PermissionsExt;
        let places = places("private");
        save_token(&places, "s", "t").unwrap();
        let mode = std::fs::metadata(places.token_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn signing_out_forgets_the_token_and_the_figures() {
        let places = places("sign-out");
        let service = "http://localhost:3000";
        save_token(&places, service, "secret-token").unwrap();
        let cached = Cached {
            service: service.into(),
            user_id: 7,
            tz: 60,
            fetched_at: Utc::now(),
            stats: stats(),
        };
        save_cached(&places, Range::Week, &cached).unwrap();

        sign_out(&places, service);
        assert_eq!(load_token(&places, service), None);
        assert_eq!(load_cached(&places, Range::Week, service, 7, 60), None);
    }

    #[test]
    fn settings_read_back_as_written() {
        let places = places("settings-round-trip");
        let mut settings = Settings {
            range: Range::Year,
            ..Settings::default()
        };
        settings.signed_in(
            &Session {
                token: "secret-token".into(),
                user: user(),
            },
            "http://localhost:3000",
        );
        settings.save(&places).unwrap();
        let again = Settings::load(&places);
        assert_eq!(again, settings);
        // The token is never in the settings, which are not kept as carefully.
        assert!(!std::fs::read_to_string(places.settings_file())
            .unwrap()
            .contains("secret-token"));
    }

    #[test]
    fn cached_figures_are_only_for_the_same_listener_service_and_clock() {
        let places = places("cache");
        let service = "http://localhost:3000";
        let cached = Cached {
            service: service.into(),
            user_id: 7,
            tz: 60,
            fetched_at: Utc::now(),
            stats: stats(),
        };
        save_cached(&places, Range::Week, &cached).unwrap();
        assert_eq!(
            load_cached(&places, Range::Week, service, 7, 60),
            Some(cached)
        );
        assert_eq!(load_cached(&places, Range::Week, service, 8, 60), None);
        assert_eq!(load_cached(&places, Range::Week, api_live(), 7, 60), None);
        assert_eq!(load_cached(&places, Range::Week, service, 7, 120), None);
        assert_eq!(load_cached(&places, Range::Month, service, 7, 60), None);
    }

    #[test]
    fn settings_survive_a_missing_or_broken_file() {
        let places = places("settings");
        assert_eq!(Settings::load(&places), Settings::default());
        std::fs::create_dir_all(&places.config).unwrap();
        std::fs::write(places.settings_file(), "{ not json").unwrap();
        assert_eq!(Settings::load(&places), Settings::default());
        // A field from a later version, and one missing, are both fine.
        std::fs::write(
            places.settings_file(),
            r#"{"range":"7d","colour":"violet"}"#,
        )
        .unwrap();
        assert_eq!(Settings::load(&places).range, Range::Week);
    }

    #[test]
    fn the_user_is_only_remembered_for_their_own_service() {
        let mut settings = Settings::default();
        settings.signed_in(
            &Session {
                token: "t".into(),
                user: user(),
            },
            "http://localhost:3000",
        );
        assert_eq!(settings.user_for("http://localhost:3000"), Some(&user()));
        assert_eq!(settings.user_for(api_live()), None);
    }
}
