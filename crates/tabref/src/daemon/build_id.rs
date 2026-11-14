use std::time::UNIX_EPOCH;

/// Names this exact binary: the crate version plus the executable's size
/// and modification time. A client whose build differs from the daemon's
/// replaces the daemon, so a rebuilt or upgraded binary never talks to a
/// daemon running older code. Costs one `stat`.
pub(crate) fn build_id() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let metadata = std::env::current_exe().and_then(std::fs::metadata);
    let Ok(metadata) = metadata else {
        return version.to_owned();
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos());
    format!("{version}-{}-{modified}", metadata.len())
}
