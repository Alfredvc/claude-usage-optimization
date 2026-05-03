//! Daily "update available" banner. Standard `update-notifier` pattern:
//! a detached background thread refreshes the cache file at most once per
//! day; the banner is read from cache only and printed to stderr when
//! interactive. Never blocks the foreground command.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::cli::Command;

const CACHE_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct VersionCache {
    pub last_checked_unix: u64,
    pub latest_version: String,
}

pub(crate) fn cache_path() -> PathBuf {
    let cache_home = std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".cache")
        });
    cache_home.join("cct").join("update_check.json")
}

pub(crate) fn read_cache_at(path: &std::path::Path) -> Option<VersionCache> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn write_cache_atomic_at(
    path: &std::path::Path,
    cache: &VersionCache,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let json = serde_json::to_vec_pretty(cache)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub(crate) fn should_refresh(cache: Option<&VersionCache>, now_unix: u64) -> bool {
    match cache {
        None => true,
        Some(c) => now_unix.saturating_sub(c.last_checked_unix) > CACHE_TTL_SECS,
    }
}

pub(crate) fn format_banner_if_newer(current: &str, latest: &str) -> Option<String> {
    let latest_clean = latest.trim_start_matches('v');
    // self_update::version::bump_is_greater returns Err on unparseable input.
    if !self_update::version::bump_is_greater(current, latest_clean).unwrap_or(false) {
        return None;
    }
    Some(format!(
        "\n\
         ╭──────────────────────────────────────────────╮\n\
         │ Update available: {current} → {latest_clean}\n\
         │ Run `cct update` to upgrade.\n\
         ╰──────────────────────────────────────────────╯\n",
    ))
}

pub(crate) fn is_disabled_by_env(no_update_check: Option<&str>, ci: Option<&str>) -> bool {
    is_truthy(no_update_check) || is_truthy(ci)
}

fn is_truthy(v: Option<&str>) -> bool {
    matches!(
        v.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

pub(crate) fn skip_for_subcommand(cmd: &Command) -> bool {
    matches!(cmd, Command::Update(_))
}

pub(crate) fn deterministic_gates_open(cmd: &Command, env_disabled: bool) -> bool {
    !skip_for_subcommand(cmd) && !env_disabled
}

const REPO_OWNER: &str = "Alfredvc";
const REPO_NAME: &str = "claude-usage-optimization";

pub(crate) fn fetch_latest_version() -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Hit GitHub's `/repos/{owner}/{name}/releases/latest` endpoint directly
    // via `Update::get_latest_release` so we agree, by construction, with what
    // `cct update` and `install.sh` will install. That endpoint returns "the
    // most recent non-prerelease, non-draft release, sorted by created_at",
    // so we don't have to reimplement that selection on the client. The
    // earlier implementation iterated `ReleaseList::fetch()` and skipped tags
    // containing `-`, which silently disagreed when a stable patch (e.g.
    // 0.2.1) was published after a newer minor (e.g. 0.3.0).
    //
    // We don't call `update()`; we only need the tag. The `bin_name` and
    // `bin_install_path` values below are required by the builder but are
    // never read by `get_latest_release`. We pass an explicit dummy install
    // path so the builder doesn't shell out to `env::current_exe()`, which
    // would convert a sandbox quirk into a misleading config error.
    let release = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("cct")
        .bin_install_path("/dev/null")
        .current_version(self_update::cargo_crate_version!())
        .build()?
        .get_latest_release()?;
    // `Release.version` already has the leading `v` stripped by self_update.
    Ok(release.version)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

fn gates_open(cmd: &Command) -> bool {
    let no_update = read_env("CCT_NO_UPDATE_CHECK");
    let ci = read_env("CI");
    let env_disabled = is_disabled_by_env(no_update.as_deref(), ci.as_deref());
    if !deterministic_gates_open(cmd, env_disabled) {
        return false;
    }
    std::io::stderr().is_terminal()
}

/// Spawn a detached background thread that refreshes the cache if stale.
/// Never blocks. Errors and panics inside the thread are swallowed.
///
/// On fetch failure, we still write the timestamp so we don't retry on
/// every invocation when the user is offline or rate-limited. We
/// preserve the previously-known `latest_version` (or empty string on
/// first-ever attempt). `format_banner_if_newer` returns `None` for
/// empty / unparseable versions, so the empty case is safe.
///
/// The foreground process may exit before this thread finishes — that's
/// by design. The cache will be repopulated on the next slow-enough run.
pub fn maybe_spawn_check(cmd: &Command) {
    if !gates_open(cmd) {
        return;
    }
    let path = cache_path();
    let existing = read_cache_at(&path);
    if !should_refresh(existing.as_ref(), now_unix()) {
        return;
    }
    std::thread::spawn(move || {
        let _ = std::panic::catch_unwind(move || {
            let fetched = fetch_latest_version();
            let prev_version = read_cache_at(&path)
                .map(|c| c.latest_version)
                .unwrap_or_default();
            let cache = VersionCache {
                last_checked_unix: now_unix(),
                latest_version: fetched.unwrap_or(prev_version),
            };
            let _ = write_cache_atomic_at(&path, &cache);
        });
    });
}

/// Read the cache (no network) and print a banner to stderr if a newer
/// version is recorded. Called before the subcommand runs so that
/// long-running subcommands like `serve` still see the banner.
pub fn maybe_print_banner(cmd: &Command) {
    if !gates_open(cmd) {
        return;
    }
    let Some(cache) = read_cache_at(&cache_path()) else {
        return;
    };
    let current = self_update::cargo_crate_version!();
    if let Some(banner) = format_banner_if_newer(current, &cache.latest_version) {
        eprint!("{banner}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn read_returns_none_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update_check.json");
        assert!(read_cache_at(&path).is_none());
    }

    #[test]
    fn read_returns_none_when_file_malformed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update_check.json");
        std::fs::write(&path, b"not json").unwrap();
        assert!(read_cache_at(&path).is_none());
    }

    #[test]
    fn write_then_read_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("update_check.json");
        let cache = VersionCache {
            last_checked_unix: 1_700_000_000,
            latest_version: "0.2.0".to_string(),
        };
        write_cache_atomic_at(&path, &cache).unwrap();
        let read = read_cache_at(&path).unwrap();
        assert_eq!(read, cache);
    }

    #[test]
    fn write_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("dirs").join("cache.json");
        let cache = VersionCache {
            last_checked_unix: 1,
            latest_version: "0.0.1".to_string(),
        };
        write_cache_atomic_at(&path, &cache).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn should_refresh_when_no_cache() {
        assert!(should_refresh(None, 1_000_000_000));
    }

    #[test]
    fn should_refresh_when_cache_is_stale() {
        let cache = VersionCache {
            last_checked_unix: 1_000_000_000,
            latest_version: "0.1.9".to_string(),
        };
        let now = 1_000_000_000 + CACHE_TTL_SECS + 1;
        assert!(should_refresh(Some(&cache), now));
    }

    #[test]
    fn should_not_refresh_when_cache_is_fresh() {
        let cache = VersionCache {
            last_checked_unix: 1_000_000_000,
            latest_version: "0.1.9".to_string(),
        };
        let now = 1_000_000_000 + CACHE_TTL_SECS - 1;
        assert!(!should_refresh(Some(&cache), now));
    }

    #[test]
    fn should_not_refresh_when_cache_is_at_exact_ttl_boundary() {
        let cache = VersionCache {
            last_checked_unix: 1_000_000_000,
            latest_version: "0.1.9".to_string(),
        };
        let now = 1_000_000_000 + CACHE_TTL_SECS;
        assert!(!should_refresh(Some(&cache), now));
    }

    #[test]
    fn should_refresh_when_clock_went_backwards() {
        // If the cache says "checked at T+1000" but `now` is T, treat as fresh
        // (don't double-fetch). Saturating subtraction handles this.
        let cache = VersionCache {
            last_checked_unix: 2_000_000_000,
            latest_version: "0.1.9".to_string(),
        };
        let now = 1_000_000_000;
        assert!(!should_refresh(Some(&cache), now));
    }

    #[test]
    fn banner_returns_none_when_versions_equal() {
        assert!(format_banner_if_newer("0.1.9", "0.1.9").is_none());
    }

    #[test]
    fn banner_returns_none_when_latest_is_older() {
        assert!(format_banner_if_newer("0.2.0", "0.1.9").is_none());
    }

    #[test]
    fn banner_returns_none_when_latest_is_unparseable() {
        assert!(format_banner_if_newer("0.1.9", "not-a-version").is_none());
    }

    #[test]
    fn banner_renders_when_latest_is_newer() {
        let out = format_banner_if_newer("0.1.9", "0.2.0").expect("expected banner");
        assert!(out.contains("0.1.9"), "missing current version: {out}");
        assert!(out.contains("0.2.0"), "missing latest version: {out}");
        assert!(out.contains("cct update"), "missing upgrade hint: {out}");
    }

    #[test]
    fn banner_strips_leading_v_from_latest() {
        let out = format_banner_if_newer("0.1.9", "v0.2.0").expect("expected banner");
        assert!(out.contains("0.2.0"));
        assert!(
            !out.contains("v0.2.0"),
            "leading v should be stripped: {out}"
        );
    }

    #[test]
    fn disabled_when_cct_no_update_check_is_truthy() {
        assert!(is_disabled_by_env(Some("1"), None));
        assert!(is_disabled_by_env(Some("true"), None));
        assert!(is_disabled_by_env(Some("yes"), None));
    }

    #[test]
    fn disabled_when_ci_is_truthy() {
        assert!(is_disabled_by_env(None, Some("true")));
        assert!(is_disabled_by_env(None, Some("1")));
    }

    #[test]
    fn not_disabled_when_envs_are_unset_or_empty() {
        assert!(!is_disabled_by_env(None, None));
        assert!(!is_disabled_by_env(Some(""), Some("")));
        assert!(!is_disabled_by_env(Some("0"), Some("0")));
        assert!(!is_disabled_by_env(Some("false"), Some("false")));
    }

    #[test]
    fn skip_for_update_subcommand_only() {
        use crate::cli::{Command, InfoArgs, UpdateArgs};
        let info = Command::Info(InfoArgs {
            db: std::path::PathBuf::from("/tmp/x.duckdb"),
        });
        let update = Command::Update(UpdateArgs {
            version: None,
            yes: false,
        });
        assert!(!skip_for_subcommand(&info));
        assert!(skip_for_subcommand(&update));
    }

    #[test]
    fn deterministic_gates_open_for_info_when_env_clean() {
        use crate::cli::{Command, InfoArgs};
        let info = Command::Info(InfoArgs {
            db: std::path::PathBuf::from("/tmp/x.duckdb"),
        });
        assert!(deterministic_gates_open(&info, /*env_disabled=*/ false));
    }

    #[test]
    fn deterministic_gates_closed_for_update_even_when_env_clean() {
        use crate::cli::{Command, UpdateArgs};
        let update = Command::Update(UpdateArgs {
            version: None,
            yes: false,
        });
        assert!(!deterministic_gates_open(&update, /*env_disabled=*/ false));
    }

    #[test]
    fn deterministic_gates_closed_for_info_when_env_disabled() {
        use crate::cli::{Command, InfoArgs};
        let info = Command::Info(InfoArgs {
            db: std::path::PathBuf::from("/tmp/x.duckdb"),
        });
        assert!(!deterministic_gates_open(&info, /*env_disabled=*/ true));
    }
}
