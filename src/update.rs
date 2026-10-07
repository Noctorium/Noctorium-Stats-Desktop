//! Whether a newer Noctorium Stats is out.
//!
//! Noctorium Stats is released with Noctorium, from the same tag, so the newest release of Noctorium is the
//! newest of this too. Asked once at launch, on a thread of its own; it only ever says so, with a link to the
//! release. Downloading and installing is the installer's job, which checks what it downloads.

use serde::Deserialize;
use std::time::Duration;

pub const LATEST: &str =
    "https://api.github.com/repos/Noctorium/Noctorium-Installer/releases/latest";
pub const RELEASES: &str = "https://github.com/Noctorium/Noctorium-Installer/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Newer {
    pub version: String,
    pub url: String,
}

/// Asks GitHub for the latest release, and says whether it is newer than [current]. Any failure -- no
/// network, the rate limit, a release with an odd tag -- is no news rather than an error: this is a courtesy,
/// and the window has better things to say.
pub fn check(current: &str) -> Option<Newer> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        html_url: Option<String>,
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(&format!("NoctoriumStats/{current}"))
        .build();
    let mut request = agent
        .get(LATEST)
        .set("Accept", "application/vnd.github+json");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", token.trim()));
        }
    }
    let release: Release = request.call().ok()?.into_json().ok()?;
    let version = release.tag_name.trim().trim_start_matches('v').to_string();
    is_newer(&version, current).then(|| Newer {
        version,
        url: release.html_url.unwrap_or_else(|| RELEASES.to_string()),
    })
}

/// 1.2.3-beta.1 as (1, 2, 3) and whether it is a pre-release, which comes before the release itself.
fn parse(version: &str) -> Option<([u64; 3], bool)> {
    let version = version.trim().trim_start_matches('v');
    let (numbers, pre) = match version.split_once(['-', '+']) {
        Some((numbers, rest)) => (
            numbers,
            version[numbers.len()..].starts_with('-') && !rest.is_empty(),
        ),
        None => (version, false),
    };
    let mut parts = numbers.split('.');
    let mut out = [0u64; 3];
    for slot in &mut out {
        *slot = match parts.next() {
            Some(part) => part.parse().ok()?,
            None => 0,
        };
    }
    parts.next().is_none().then_some((out, pre))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse(latest), parse(current)) {
        (Some((l, l_pre)), Some((c, c_pre))) => l > c || (l == c && c_pre && !l_pre),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_is_by_number_not_by_spelling() {
        assert!(is_newer("0.13.0", "0.12.2"));
        assert!(is_newer("v0.12.10", "0.12.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.12.2", "0.12.2"));
        assert!(!is_newer("0.12.1", "0.12.2"));
    }

    #[test]
    fn a_release_is_newer_than_its_own_pre_release() {
        assert!(is_newer("1.2.3", "1.2.3-beta.1"));
        assert!(!is_newer("1.2.3-beta.2", "1.2.3"));
        assert!(is_newer("1.2.4-beta.1", "1.2.3"));
    }

    #[test]
    fn a_tag_that_is_not_a_version_is_no_news() {
        assert!(!is_newer("nightly", "0.1.0"));
        assert!(!is_newer("1.2.3.4", "0.1.0"));
        assert!(!is_newer("0.13.0", "not a version"));
        assert!(is_newer("1.3", "1.2.9"));
    }
}
