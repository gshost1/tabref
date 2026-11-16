use serde::{Deserialize, Serialize};

/// How a launch record's browser relates to its profile and to this state
/// directory. `persistent` marks a profile the user named (`--profile`),
/// which tabref never deletes. `adopted` marks a browser found already
/// running on that profile (another state directory started it): tabref
/// asks it to close but never kills it. Both sit at the record's top level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileUse {
    #[serde(default)]
    pub persistent: bool,
    #[serde(default)]
    pub adopted: bool,
}

impl ProfileUse {
    /// A throwaway profile tabref made for this launch.
    pub const THROWAWAY: Self = Self {
        persistent: false,
        adopted: false,
    };
    /// A `--profile` the browser was started on by this launch.
    pub const PERSISTENT: Self = Self {
        persistent: true,
        adopted: false,
    };
    /// A `--profile` whose running browser another launch started.
    pub const ADOPTED: Self = Self {
        persistent: true,
        adopted: true,
    };
}
