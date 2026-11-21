use std::path::{Path, PathBuf};

pub(super) fn move_aside(path: &Path) -> std::io::Result<PathBuf> {
    for attempt in 0..100 {
        let backup = path.with_file_name(format!(
            "downloads-ledger.corrupt-{}-{}-{attempt}.json",
            tabref_page::now_ms(),
            std::process::id()
        ));
        match std::fs::hard_link(path, &backup) {
            Ok(()) => {
                std::fs::remove_file(path)?;
                return Ok(backup);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not reserve a corrupt download ledger backup",
    ))
}
