use std::fs::Metadata;
use std::io;
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
const CHECK_OWNER: &str = concat!(
    r"$ErrorActionPreference = 'Stop'; $path = $env:TABREF_PROFILE_OWNER_PATH; ",
    r"if ($path.StartsWith('\\?\UNC\')) { $path = '\\' + $path.Substring(8) } ",
    r"elseif ($path.StartsWith('\\?\')) { $path = $path.Substring(4) }; ",
    r"$acl = [System.IO.Directory]::GetAccessControl($path, [System.Security.AccessControl.AccessControlSections]::Owner); ",
    r"$owner = $acl.GetOwner([System.Security.Principal.SecurityIdentifier]); ",
    r"$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent(); ",
    r"if ($owner.Value -eq $identity.User.Value -or $owner.Value -eq $identity.Owner.Value) { exit 0 }; ",
    r"exit 1",
);

pub(crate) fn is_reparse_point(metadata: &Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Accepts the process user's SID or the default owner SID assigned by
/// Windows when that token creates a directory. Pass the path as data.
pub(crate) fn is_own_directory(profile: &Path) -> bool {
    if !std::fs::symlink_metadata(profile)
        .is_ok_and(|metadata| metadata.is_dir() && !is_reparse_point(&metadata))
    {
        return false;
    }
    check_owner(profile).is_ok_and(|output| output.status.success())
}

/// PowerShell parses the remaining command line as script source after
/// `-Command`. Pass only constant source there and carry the path separately.
fn check_owner(profile: &Path) -> io::Result<Output> {
    let system_root = std::env::var_os("SystemRoot")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "SystemRoot is unset"))?;
    let powershell =
        PathBuf::from(system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    Command::new(powershell)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .raw_arg(CHECK_OWNER)
        .env("TABREF_PROFILE_OWNER_PATH", profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_canonicalized_directory_created_by_this_user_has_the_expected_owner() {
        let dir =
            std::env::temp_dir().join(format!("tabref profile [owner] {}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let result = check_owner(&dir.canonicalize().unwrap());
        std::fs::remove_dir(&dir).unwrap();
        let output = result.expect("Windows PowerShell should start");
        assert!(
            output.status.success(),
            "owner check returned {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
