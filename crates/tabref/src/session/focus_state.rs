/// Whether tabref told a tab it holds the window focus: `On`, `Off`, or
/// `Unknown` after a switch-off that did not finish (it timed out or was
/// cancelled), which the next attach of the current tab and the next
/// switch-off both repeat; telling a tab either again is safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusState {
    On,
    Off,
    Unknown,
}
