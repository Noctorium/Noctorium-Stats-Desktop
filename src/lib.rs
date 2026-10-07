//! Noctorium Stats: a listener's statistics from their Noctorium account, in a window.
//!
//! The service counts everything; this asks it, remembers the answer, and draws it. [api] is the service,
//! [stats] what it answers, [format] how the answer reads, [store] what is kept between runs, [update]
//! whether a newer release is out, and [gui] the window.

pub mod api;
pub mod format;
pub mod gui;
pub mod stats;
pub mod store;
pub mod update;

/// This build's version: the release tag's, which the release build passes in as NOCTORIUM_VERSION, or
/// Cargo's for a build of one's own.
pub fn version() -> &'static str {
    const FROM_TAG: Option<&str> = option_env!("NOCTORIUM_VERSION");
    match FROM_TAG
        .map(|v| v.trim().trim_start_matches('v'))
        .filter(|v| !v.is_empty())
    {
        Some(version) => version,
        None => env!("CARGO_PKG_VERSION"),
    }
}
