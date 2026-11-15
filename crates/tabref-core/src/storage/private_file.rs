use std::fs::{DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` through a temporary file in the same directory
/// and a rename, so a reader never sees half a file. Missing parent
/// directories are created private to the user (0700 on Unix), the file is
/// written private (0600), and the temporary file is created new, so a file
/// or link planted at its name is never written through. The file is not
/// synced to disk: everything kept this way (ref maps, the current tab, the
/// launch record) is a cache that a crash may lose without harm, and a sync
/// cost 10 to 20 ms per snapshot on a real disk.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    create_private_dir(parent)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    private_file_mode(&mut options);
    let mut file = options.open(&temp)?;
    if let Err(error) = file.write_all(bytes) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// Creates `path` and its missing parents, each private to the user (0700
/// on Unix). A directory that already exists keeps its mode.
pub fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    private_dir_mode(&mut builder);
    builder.create(path)
}

/// Creates the directory `path`, which must not exist yet, private to the
/// user (0700 on Unix). Its parent must exist.
pub fn create_new_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = DirBuilder::new();
    private_dir_mode(&mut builder);
    builder.create(path)
}

#[cfg(unix)]
fn private_dir_mode(builder: &mut DirBuilder) {
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
}

#[cfg(not(unix))]
fn private_dir_mode(_builder: &mut DirBuilder) {}

#[cfg(unix)]
fn private_file_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn private_file_mode(_options: &mut OpenOptions) {}

/// Reads at most `limit` bytes of `path`; a longer file is an error, so a
/// corrupt or hostile state file cannot exhaust memory.
pub fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} is larger than {limit} bytes", path.display()),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "private_file_tests.rs"]
mod tests;
