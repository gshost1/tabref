use std::path::{Path, PathBuf};

pub(super) fn move_aside(path: &Path) -> std::io::Result<PathBuf> {
    let timestamp = backups(path)?
        .iter()
        .map(|(time, _)| time.saturating_add(1))
        .max()
        .unwrap_or_default()
        .max(tabref_page::now_ms());
    for attempt in 0..100 {
        let backup = path.with_file_name(format!(
            "downloads-ledger.corrupt-{timestamp}-{}-{attempt}.json",
            std::process::id()
        ));
        match std::fs::hard_link(path, &backup) {
            Ok(()) => {
                let mut saved = backups(path)?;
                saved.sort_unstable_by(|left, right| right.cmp(left));
                for (_, old) in saved.into_iter().skip(3) {
                    match std::fs::remove_file(old) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error),
                    }
                }
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

fn backups(path: &Path) -> std::io::Result<Vec<(u64, PathBuf)>> {
    let mut saved = Vec::new();
    for entry in std::fs::read_dir(path.parent().unwrap_or(Path::new(".")))? {
        let entry = entry?;
        let name = entry.file_name();
        if let Some(time) = name.to_str().and_then(backup_time)
            && entry.file_type()?.is_file()
        {
            saved.push((time, entry.path()));
        }
    }
    Ok(saved)
}

fn backup_time(name: &str) -> Option<u64> {
    let (time, rest) = name
        .strip_prefix("downloads-ledger.corrupt-")?
        .strip_suffix(".json")?
        .split_once('-')?;
    let (pid, attempt) = rest.split_once('-')?;
    pid.parse::<u32>().ok()?;
    attempt.parse::<u32>().ok()?;
    time.parse().ok()
}
