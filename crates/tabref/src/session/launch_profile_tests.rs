#![cfg_attr(not(target_os = "linux"), allow(unused_imports, dead_code))]

use std::path::{Path, PathBuf};

use super::{PROFILE_MARKER, mark, normalize, refusal, same_dir, unfit_existing};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tabref-lp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

#[test]
fn a_dot_dot_past_a_missing_directory_is_refused_and_one_that_exists_resolves() {
    let base = temp("norm");
    std::fs::create_dir_all(base.join("real")).unwrap();
    let past_missing = normalize(&base.join("nope").join("..").join("real"));
    let through_existing = normalize(&base.join("real").join("..").join("new"));
    let _ = std::fs::remove_dir_all(&base);
    if cfg!(windows) {
        assert_eq!(past_missing.unwrap(), base.join("real"));
    } else {
        assert_eq!(
            past_missing.unwrap_err().kind(),
            Some("launch_profile_refused")
        );
    }
    assert_eq!(through_existing.unwrap(), base.join("new"));
}

#[cfg(unix)]
#[test]
fn a_symlinked_spelling_normalises_to_the_real_directory() {
    let base = temp("link");
    std::fs::create_dir_all(base.join("real")).unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("alias")).unwrap();
    let alias = normalize(&base.join("alias").join("p"));
    let slash = same_dir(&base.join("real/"), &base.join("real"));
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(alias.unwrap(), base.join("real").join("p"));
    assert!(slash);
}

#[test]
fn in_or_around_the_users_profiles_or_the_state_directory_is_refused() {
    let defaults = [PathBuf::from("/home/u/.config/google-chrome")];
    let root = Path::new("/home/u/.tabref");
    for dir in [
        "/home/u/.config/google-chrome",
        "/home/u/.config/google-chrome/Default",
        "/home/u/.config",
        "/home/u/.tabref",
        "/home/u/.tabref/profile",
        "/home/u",
    ] {
        assert!(refusal(Path::new(dir), root, &defaults).is_some(), "{dir}");
    }
    let theirs = refusal(
        Path::new("/home/u/.config/google-chrome/Default"),
        root,
        &defaults,
    );
    assert!(
        theirs
            .unwrap()
            .suggestion
            .unwrap()
            .contains("connect --user-chrome")
    );
    assert!(refusal(Path::new("/home/u/profiles/work"), root, &defaults).is_none());
}

#[test]
fn only_a_new_empty_or_marked_directory_is_launched_on() {
    let base = temp("fit");
    let empty = base.join("empty");
    let marked = base.join("marked");
    let foreign = base.join("foreign");
    for dir in [&empty, &marked, &foreign] {
        std::fs::create_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    for dir in [&empty, &marked, &foreign] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(marked.join(PROFILE_MARKER), "x").unwrap();
    std::fs::write(marked.join("Cookies"), "x").unwrap();
    std::fs::write(foreign.join("Local State"), "x").unwrap();

    let results = (
        unfit_existing(&base.join("new")).is_none(),
        unfit_existing(&empty).is_none(),
        unfit_existing(&marked).is_none(),
        unfit_existing(&foreign).map(|error| error.message),
    );
    let _ = std::fs::remove_dir_all(&base);
    assert!(results.0 && results.1 && results.2);
    assert!(results.3.unwrap().contains("not a tabref profile"));
}

#[cfg(target_os = "linux")]
#[test]
fn a_program_that_is_not_a_browser_is_never_adopted() {
    use std::io::BufRead;
    let profile = temp("adopt");
    let flag = format!("--user-data-dir={}", profile.display());
    let mut holder = std::process::Command::new("python3")
        .args([
            "-c",
            "import socket,sys\ns=socket.socket();s.bind(('127.0.0.1',0));s.listen()\n\
             print(s.getsockname()[1],flush=True);sys.stdin.read()",
            &flag,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(holder.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    std::fs::write(
        profile.join("DevToolsActivePort"),
        format!("{}\n/devtools/browser/x\n", line.trim()),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        format!("host-{}", holder.id()),
        profile.join("SingletonLock"),
    )
    .unwrap();

    let adopted = super::running_on_profile(&profile, true);
    let _ = holder.kill();
    let _ = holder.wait();
    let _ = std::fs::remove_dir_all(&profile);
    assert!(
        adopted.is_none(),
        "adopted a program that is not a browser: {adopted:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_directory_others_may_write_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let base = temp("perm");
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o777)).unwrap();
    let open = unfit_existing(&base).map(|error| error.message);
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
    let _ = std::fs::remove_dir_all(&base);
    assert!(open.unwrap().contains("writable by others"));
}

#[test]
fn a_profile_another_launch_just_marked_is_marked_again_without_error() {
    let base = temp("mark");
    let first = mark(&base);
    let second = mark(&base);
    let _ = std::fs::remove_dir_all(&base);
    assert!(first.is_ok() && second.is_ok(), "{second:?}");
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_to_nothing_is_refused_as_an_argument() {
    let base = temp("dangling");
    std::os::unix::fs::symlink(base.join("gone"), base.join("link")).unwrap();
    let error = normalize(&base.join("link")).map(|_| ());
    let _ = std::fs::remove_dir_all(&base);
    let error = error.unwrap_err();
    assert_eq!(error.code, tabref_core::ErrorCode::InvalidArgs, "{error:?}");
    assert_eq!(error.kind(), Some("launch_profile_refused"));
}

#[cfg(unix)]
#[test]
fn the_launch_lock_file_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let base = temp("lockmode");
    let held = super::lock(&base, tabref_core::Deadline::after(1000).unwrap());
    let mode =
        std::fs::metadata(base.join(super::LAUNCH_LOCK)).map(|m| m.permissions().mode() & 0o777);
    drop(held);
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(mode.unwrap(), 0o600);
}
