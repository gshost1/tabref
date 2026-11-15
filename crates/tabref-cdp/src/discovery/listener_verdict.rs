/// Who holds a loopback DevTools port, as far as this platform can tell
/// without talking to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerVerdict {
    /// Nothing listens on the port.
    Nothing,
    /// Every socket that could take a connection to the port belongs to a
    /// process of this user that the caller trusts.
    Trusted,
    /// Something listens that is not a trusted process of this user; the
    /// text says what.
    Untrusted(String),
    /// This platform cannot tell cheaply.
    Unknown,
}
