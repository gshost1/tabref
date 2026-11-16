/// How a wait for an element's state ended: whether the state held, and
/// what the element was at the end (`disabled`, `hidden`, `value "67.00"`,
/// `gone`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateWait {
    pub held: bool,
    pub now: String,
}
