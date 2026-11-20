use std::fs::Metadata;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
const CHECK_OWNER: &str = "
    $owner = (Get-Acl -LiteralPath $env:TABREF_PROFILE_OWNER_PATH -ErrorAction Stop).GetOwner(
        [System.Security.Principal.SecurityIdentifier])
    $user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    if ($owner -eq $user) { exit 0 }
    exit 1
";

pub(crate) fn is_reparse_point(metadata: &Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Checks the owner's SID against the process user. Pass the directory as
/// data so its spelling cannot change the PowerShell expression.
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
