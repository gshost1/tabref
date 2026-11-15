use super::held_port::HeldPort;

/// Who holds a loopback DevTools port, as far as this platform can tell
/// without talking to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerVerdict {
    /// Nothing listens on the port.
    Nothing,
    /// Every socket listening on the port belongs to processes of this user
    /// that the caller trusts, and to no other process.
    Trusted(HeldPort),
    /// Something else holds the port: `holder` names a process of this user
    /// that is not trusted (`python3 (pid 1234)`), or is `None` when no
    /// process this user can see holds it (another user's, or root's).
    Untrusted { holder: Option<String> },
    /// The socket or process tables could not be read; the text says which.
    CantCheck(String),
    /// This platform cannot tell cheaply.
    Unknown,
}
