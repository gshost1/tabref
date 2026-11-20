use std::fs::Metadata;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
const CHECK_OWNER: &str = r"
    $path = $env:TABREF_PROFILE_OWNER_PATH
    if ($path.StartsWith('\\?\UNC\')) { $path = '\\' + $path.Substring(8) }
    elseif ($path.StartsWith('\\?\')) { $path = $path.Substring(4) }
    $owner = (Get-Acl -LiteralPath $path -ErrorAction Stop).GetOwner(
        [System.Security.Principal.SecurityIdentifier])
    $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
    if ($owner -eq $identity.User -or $owner -eq $identity.Owner) { exit 0 }
    exit 1
";

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
    let Some(system_root) = std::env::var_os("SystemRoot") else {
        return false;
    };
    let powershell =
        PathBuf::from(system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            CHECK_OWNER,
        ])
        .env("TABREF_PROFILE_OWNER_PATH", profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
